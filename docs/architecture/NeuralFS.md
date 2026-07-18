# NeuralFS ? Plano de Arquitetura Completo

> **Lifecycle (INDEX):** `fazendo` (SESSION_133)  
> **Feito:** RAM I/O; B-tree multi-nivel; USB mount + format opt-in; GPT GUID NeuralFS; boot dados exFAT (+ unified); journal recover no mount.  
> **Residual:** stress power-loss e2e; stress B-tree level≥2 com milhares de keys; interop Windows/Linux da imagem exFAT.  
> ADR-0040 MVP permanece `completa`; este doc cobre o follow-up NeuralFS, nao reabre a ADR.

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


---

## 11. Sprint FS-v2 � Ecossistema de Armazenamento Completo

A Sprint FS-v2 implementou todo o ecossistema de armazenamento em 4 subsprints, ~3.884 LOC, 0 erros.

### FS-a: Funda\u00e7\u00e3o Multi-FS (1.712 LOC)

| Item | Arquivo | Descri\u00e7\u00e3o |
|------|---------|-----------|
| BlockDevice+ write | `block_dev.rs` | `write_sectors()` no trait. ATA PIO (0x30) + AHCI DMA (0x35) com sfence + erro checking |
| exFAT driver | `exfat.rs` | Leitura de pendrives/SDHC >4GB. VBR, cluster bitmap, FAT chain, volume label UTF-16 |
| GPT escrita | `gpt.rs` | Criar tabela GPT com CRC32C, backup GPT, MBR protetiva. `gpt_format_single()` |
| DiskIntelligenceAgent v2 | `disk_agent/mod.rs` | Probe real de exFAT/NTFS/EXT. SMART, hotplug, MHI registration, VFS mount, ARC cache |
| USB-MSC fix | `usb_msc.rs` | `write_sector()` agora passa dados reais (n\u00e3o &mut[]) |

### FS-b: NeuralFS + HDs do Usu\u00e1rio (1.357 LOC)

| Item | Arquivo | Descri\u00e7\u00e3o |
|------|---------|-----------|
| NeuralFS (F1-F8) | `neural_fs/` (12 arquivos) | FS nativo CoW: CRC32C, superblock, B-tree, inode, dir, extent, journal WAL, volume, agente VFS. ~1.119 LOC. 11 DATA_LOSS bugs corrigidos via ponytail audit |
| NVMe TRIM | `disk_agent/nvme.rs` | Dataset Management (Deallocate) para SSD. Range descriptor 8+4+4 bytes |
| ATA TRIM | `ata.rs` | DATA SET MANAGEMENT via PIO. Cache FLUSH ap\u00f3s comando |
| SMART hist\u00f3rico | `disk_agent/mod.rs` | SmartHistoryEntry ring buffer (64). Alerta preditivo se realoca\u00e7\u00e3o acelerar |
| Bad block | `self_heal.rs` | Detec\u00e7\u00e3o via SMART pending/reallocated sectors + read/write retry |
| NTFS leitura | `ntfs_reader.rs` | Parse $MFT, $Volume, FILE records, atributos residentes. Detecta label, mount via FilesystemDriver trait |
| EXT2 leitura | `ext2_reader.rs` | Parse superblock, block group descriptors, inode table. Detecta label, mount via FilesystemDriver trait |

### FS-c: MHI Ativo + Apps (+315 LOC)

| Item | Arquivo | Descri\u00e7\u00e3o |
|------|---------|-----------|
| ARC cache din\u00e2mico | `disk_agent/cache.rs` | Configur\u00e1vel por tier, write-back coalescing (100 tick janela), LFU + rec\u00eancia eviction |
| MHI Ativo | `mhi.rs` | `mhi_tick()` executa 1 migra\u00e7\u00e3o/tick via DMA ring. MegaTrain queue. `arc_suggest_tier()` real |
| FilesystemDriver trait | `fs_driver.rs` | Trait unificado: detect, mount, read, write, list, free_space. Todos os FS implementam |
| I/O Scheduler | `io_scheduler.rs` | Deadline: 4 filas (Critical > Read > Write > Idle). Write coalescing, batch flush, merge requisi\u00e7\u00f5es |
| Storage Manager | `storage_manager.rs` | App: /storage, /smart, /mount, /format. Relat\u00f3rio de MHI, VFS mounts, discos |

### FS-d: Rede + Prote\u00e7\u00e3o (+500 LOC)

| Item | Arquivo | Descri\u00e7\u00e3o |
|------|---------|-----------|
| Network mounts | `netfs.rs` + `tools/netfs_bridge.py` | Protocolo serial tunnel (porta 4446). Backends WebDAV/NFS/S3/dummy. READ/WRITE/LIST via TCP |
| Filesystem SelfHeal | `self_heal.rs` | `BAD_BLOCKS` global. `verify_block()` CRC32C. `read_with_retry()` / `write_with_retry()` 3 tentativas |
| Disk Power Mgmt | `disk_power.rs` | ATA IDLE (spin-down config). NVMe PS0-PS5. Pol\u00edtica autom\u00e1tica SSD vs HDD |

### Deferidos (read-only j\u00e1 atende)

| Item | LOC | Motivo |
|------|:---:|--------|
| NTFS escrita | ~800 | Risco de corromper discos Windows. Postecipado |
| EXT3/4 journal | ~600 | Journal replay complexo sem HW real para testar |
| S3 Cloud Storage | ~300 | Parcialmente coberto pelo netfs_bridge.py |
| GPU Direct Storage | ~300 | Requer GPU compute maduro (futuro) |

### 0 erros, 0 warnings em todos os crates.

---

## 12. Ecosystem Namespace (Hermes / Cortex packages)

> **Contrato de pastas** no volume NeuralFS. Orquestração, assinatura, CRUD e HITL:
> **ADR-0051** (`0051-hermes-ecosystem-packages.md`). Este § só define **onde** vive o quê.

Mount canônico: `/mnt/neural` (NeuralFsAgent). Namespace de pacotes:

```
/mnt/neural/ecosystem/
  skills/<name>/
    SKILL.md              # metadata + instruções (assinatura embutida opcional)
    references/           # schemas, exemplos (read)
    scripts/              # Python/Bash efêmero (HITL antes de exec)
    app.wasm              # artefato promovido (SkillOpt / evolve)
  agents/<name>/
    AGENT.md              # manifesto Agency/nativo (schedule, division, skills)
    MANIFEST              # legado WASM (ADR-0032)
    app.wasm              # agente WASM tickável (opcional)
  workflows/<id>/
    WORKFLOW.md           # fluxo declarativo
  plugins/<name>/
    PLUGIN.md             # bundle: skills declaradas + risk
  mcp/<name>/
    MCP.md                # tools/resources → bridge EventBus
  models/                 # .bitnet / pesos (read; update = Confirm)
  firmware/               # FW blobs HW (read; write = Escalate)
```

| Pasta | Propósito | FS default | Consumidor |
|-------|-----------|------------|------------|
| `skills/` | Capacidade repetível (procedimento) | RW via PackageHub | Hermes + Cortex prompt |
| `agents/` | Manifestos Agent + WASM opcional | RW + HITL | Agency / PackageHub / WasmRT |
| `workflows/` | Fluxos declarativos | RW + HITL | Hermes |
| `plugins/` | Bundle de skills | RW + Escalate | PluginHub |
| `mcp/` | Tools externos | RW + Escalate | McpAgent |
| `models/` | Inferência | R (W Confirm) | Cortex |
| `firmware/` | HW bring-up | R (W Escalate) | SelfHeal / GPU FW |

**Bootstrap:** `NeuralFsAgent` cria `ecosystem/{skills,agents,workflows,plugins,mcp,models,firmware}` após mount (RAM/ATA/USB). Nomes de dir entry ≤22 bytes; paths lógicos longos são codificados no VFS.

**Disco de dados ≠ namespace:** `disk_*.raw` (exFAT flat via `mkexfat.py`) carrega modelos/firmware no root. PackageHub **não** lê exFAT — só NeuralFS `/mnt/neural`. Seed de agentes = embutido (`include`/`agency_seed`) + persist opcional no NeuralFS.

**Persistência:** se `/mnt/neural` não montar, PackageHub opera em **RAM** e loga honestamente (`persisted=false`). Não inventar write em disco.

**Segurança no FS:** path traversal (`..`) rejeitado no hub; pacotes unsigned exigem ApprovalGate Escalate antes de Create/Update/Delete (detalhe ADR-0051).
**VFS bridge:** Hermes usa callbacks do bin (`neural-kernel::fs`) após `init_fs_agents` — não o VFS vazio de `k_nano`.
