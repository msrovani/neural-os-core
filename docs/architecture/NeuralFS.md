# NeuralFS — Plano de Arquitetura Completo

## 1. Fontes de Referência Analisadas

| Fonte | Relevância |
|-------|-----------|
| **BAFS** (bazzulto-bafs, ~3.500 LOC) | Cópia direta de referência — CoW, B-tree, journal, CRC32C, extent allocator, no_std. Estrutura e API prontas para adaptação. |
| **btree-store** (crates.io) | CoW B-Tree + ACID. std-only, muito genérico. **Não usar.** |
| **lcpfs** (crates.io) | ZFS-inspired CoW FS. std-only, imaturo (<100 downloads). **Não usar.** |
| **embedded-crc32c** (crates.io) | 0.1.0, 1.7k downloads. Poderia usar, mas CRC32C são 50 linhas — **hand-roll.** |
| **Neural-OS `block_dev.rs`** | Trait `BlockDevice` com `read_sectors`/`write_sectors`. Extender para `Send+Sync` + `total_sector_count()`. |
| **Neural-OS `gpt.rs`** | CRC32C inline bit-a-bit (lento). Substituir pela versão lookup-table. |
| **Neural-OS `vfs/mod.rs`** | `VfsRegistry` + `FilesystemAgent` trait. Integração direta. |
| **Neural-OS `fs/mod.rs`** | `FilesystemAgent` trait: `read`, `write`, `list`. Implementar `NeuralFsAgent`. |

---

## 2. Decisão de Dependências Externas

**Hand-roll 100%.** Nenhuma crate externa para o core. Razões:

- BAFS prova que um FS CoW completo cabe em **~3.500 LOC** sem dependências
- O ambiente `no_std + alloc` no kernel neural já existe e funciona
- Crc32C lookup-table = 50 linhas (BAFS `checksum.rs`)
- xxHash-64 = 80 linhas (BAFS `dir.rs`)
- B-tree = 600 linhas (BAFS `btree.rs`)
- Evita versionamento conflitante com as 19 crates já no Cargo.toml
- A única dependência adicional no `Cargo.toml` é adicionar o módulo `neural_fs/` ao crate `neural-kernel`

---

## 3. Block Layout no Disco

```
Setor 0           LBA 0        MBR (proteção GPT)
Setor 1-33        LBA 1-33     GPT primária
Setor 34-2047     LBA 34-2047  Reservado (bootloader, alignment)

Bloco 0           LBA 2048     Reservado (boot sector / alignment padding)
Bloco 1           LBA 2056     Superbloco primário (512 bytes + padding 4K)
Bloco 2           LBA 2064     Superbloco backup
Bloco 3..N        LBA 2072..   Journal (1% do disco, min 256 blocos, max 16384)
Bloco N+1..end                 Área de dados
  ├─ B-tree roots              (inode tree, free-extent tree, checksum tree)
  ├─ CoW metadata nodes        (bump allocator cresce pra frente)
  └─ File data extents         (free-extent tree, alocado top-down)
```

Cada bloco = **4096 bytes** (8 setores × 512 bytes). Setores 512 bytes.

### Layout do Superbloco (512 bytes)

| Offset | Tamanho | Campo |
|--------|---------|-------|
| 0 | 8 | magic (`b"NEURALFS"`) |
| 8 | 4 | version (1) |
| 12 | 4 | block_size (4096) |
| 16 | 8 | total_block_count |
| 24 | 8 | free_block_count |
| 32 | 8 | allocated_inode_count |
| 40 | 8 | last_committed_tx_id |
| 48 | 8 | root_inode_number (1) |
| 56 | 8 | inode_tree_root_block |
| 64 | 8 | free_extent_tree_root_block |
| 72 | 8 | checksum_tree_root_block |
| 80 | 8 | journal_start_block |
| 88 | 8 | journal_size_in_blocks |
| 96 | 16 | volume_uuid (`[u64;2]`) |
| 112 | 32 | volume_label (`[u64;4]`) |
| 144 | 4 | feature_flags |
| 148 | 4 | checksum_algorithm (0=CRC32C) |
| 152 | 352 | reserved |
| 504 | 4 | next_cow_block_address |
| 508 | 4 | superblock_checksum |

---

## 4. Formato do Nó B-tree (4096 bytes cada)

| Offset | Tamanho | Campo |
|--------|---------|-------|
| 0 | 4 | CRC32C checksum (bytes 4..4096) |
| 4 | 1 | level (0=leaf, 1+=internal) |
| 5 | 1 | flags |
| 6 | 2 | item_count |
| 8 | 8 | self_block_address |
| 16 | 8 | generation (tx_id) |
| 24 | ? | items |

**Leaf node**: headers (25 bytes cada) crescem de 24 pra cima; valores empacotam do fim do bloco pra baixo.

**Internal node**: cada item = key(17) + child_block(8) + child_generation(8) = 33 bytes.

**Key** (17 bytes): `object_id(8) | item_type(1) | offset(8)`

| item_type | Uso |
|-----------|-----|
| 0x01 | Inode metadata |
| 0x02 | Directory entry |
| 0x03 | File extent data |
| 0x04 | Free extent |
| 0x05 | Checksum entry |

---

## 5. Formato da Transação do Journal

| Offset | Tamanho | Campo |
|--------|---------|-------|
| 0 | 8 | magic (`b"NRFSJRNL"`) |
| 8 | 8 | transaction_id |
| 16 | 4 | dirty_block_count |
| 20 | count×8 | block_addresses[] |
| 20+ | count×4096 | block_data[] |
| 20+ | 4 | CRC32C checksum |

O journal **não é circular** em v1 — só um commit record por vez. Recovery replay se `journal.transaction_id > superblock.last_committed_transaction_id`.

---

## 6. Estratégia de Alocação CoW

**Dois alocadores completamente separados:**

| Alocador | Tipo | Estratégia | Persistido em |
|----------|------|------------|---------------|
| **Metadata CoW** | Bump pointer | Aloca do `next_cow_block_address` pra frente. Node CoW sempre pega o próximo bloco livre | `superblock.next_cow_block_address` |
| **File data** | Free-extent B-tree | Last-fit: aloca do topo do maior extent livre (cresce pra baixo). Metadata bumps pra cima → colidem só quando o disco está cheio. | `free_extent_tree_root_block` |

**Por que dois alocadores?** Metadata CoW nunca pode alocar do free-extent tree porque a própria operação de alocar (inserir/deletar na free-extent tree) produz nós CoW, causando o paradoxo "CoW do alocador". Bump pointer linear evita isso completamente.

**Abandono vs reclaim:** Nós CoW intra-sessão são simplesmente abandonados (bump pointer já passou). Nós CoW cross-sessão (commitados em sessão anterior e superseded por CoW novo) são devolvidos à free-extent tree via `free_blocks()`.

---

## 7. Integração FilesystemDriver Trait com VFS

```
VFS Registry
  │
  ├── resolve("/mnt/neural/foo") → ("/mnt/neural", "foo", "neuralfs")
  │
  └── FS_AGENTS
        │
        └── [NeuralFsAgent]
              │
              ├── name() → "neuralfs"
              ├── mount_point() → "/mnt/neural"
              ├── read(path)   → volume_read_file_data()
              ├── write(path)  → resolve_parent() → volume_create_file() + volume_write_file_data()
              └── list(path)   → directory_list_all_entries()
```

### Implementação concreta

```rust
pub struct NeuralFsAgent {
    volume: Mutex<BafsVolume<AtaDriver>>,  // ou AhciDriver, etc.
}

impl FilesystemAgent for NeuralFsAgent {
    fn read(&self, path: &str) -> Result<Vec<u8>, &str> {
        let vol = self.volume.lock();
        let parent_ino = resolve_parent(&vol, path)?;
        let filename = path::filename(path);
        let child_ino = volume_lookup_directory_entry(&vol, parent_ino, filename)?
            .ok_or("NotFound")?;
        let mut buf = vec![0u8; 65536];
        let n = volume_read_file_data(&vol, child_ino, 0, &mut buf).or(Err("IO"))?;
        buf.truncate(n);
        Ok(buf)
    }
}
```

O `BafsVolume<D>` é agnóstico ao driver de bloco. Integramos via:

```rust
impl crate::block_dev::BlockDevice for AtaDriver {
    fn read_sectors(&self, lba: u64, buf: &mut [u8]) -> bool { ... }
    fn write_sectors(&self, lba: u64, buf: &[u8]) -> bool { ... }
    fn total_sectors(&self) -> u64 { ... }
}
```

**Extensão necessária no `block_dev.rs`:** Adicionar `total_sectors()`.

---

## 8. LOC Estimado por Módulo

| Módulo | LOC | Fonte |
|--------|:---:|-------|
| `superblock.rs` | 180 | Adaptado BAFS |
| `btree.rs` | 600 | Adaptado BAFS |
| `inode.rs` | 120 | Adaptado BAFS |
| `extent.rs` | 250 | Adaptado BAFS |
| `dir.rs` | 200 | Adaptado BAFS (inclui xxHash-64) |
| `journal.rs` | 250 | Adaptado BAFS |
| `checksum.rs` | 80 | Portar de BAFS |
| `checksum_tree.rs` | 100 | Adaptado BAFS |
| `volume.rs` | 400 | Adaptado BAFS |
| `block_dev.rs` | 30 | Estender trait existente |
| `neural_fs_agent.rs` | 100 | Integração com FilesystemAgent |
| `mod.rs` | 20 | Re-export |
| **Total** | **~2.330** | |

vs BAFS original ~3.500 LOC (diferença: BAFS tem kernel adapter + error.rs + bafs-tools + testes). Nosso código core será ~2.330 LOC.

---

## 9. Pipeline de Implementação

| Fase | Módulos | Deps |
|:----:|---------|------|
| **F1** | `block_dev.rs` (estender), `checksum.rs` | Nenhuma |
| **F2** | `superblock.rs` | checksum, block_dev |
| **F3** | `btree.rs` | block_dev, checksum, superblock |
| **F4** | `inode.rs`, `dir.rs`, `extent.rs`, `checksum_tree.rs` | btree, superblock |
| **F5** | `journal.rs` | checksum, block_dev, superblock |
| **F6** | `volume.rs` | todos acima |
| **F7** | `neural_fs_agent.rs` | volume, fs/mod, vfs |
| **F8** | Testes (in-memory disk) | volume |

**Ordem recomendada:** Seguir a ordem de compilação — de baixo (block_dev, checksum) para cima (volume, agent). Testes com `MemoryDisk` (`Vec<u8>`) em cada fase.

---

## 10. Riscos e Mitigações

| Risco | Mitigação |
|-------|-----------|
| CoW bump pointer estoura espaço metadata | `next_cow_block_address` salvo a cada commit. Se colidir com data area, usar free-extent tree como fallback. |
| xxHash-64 collision em diretório | Linear scan dentro do mesmo hash (BAFS já trata). Probabilidade negligible (<10⁻¹⁸ para 1000 arquivos). |
| Journal overflow em write massivo | Journal de 1% do disco. Para v1, transação única não pode exceder journal. Em v2, journal circular. |
| Deadlock com `Mutex<BafsVolume>` | NeuralFS agent expõe operações atômicas (read/write por path). Uma transação por chamada. |
| Integração com VFS atual (que resolve por agente) | `NeuralFsAgent` implementa `FilesystemAgent` trait existente. Zero mudança no VFS. |
