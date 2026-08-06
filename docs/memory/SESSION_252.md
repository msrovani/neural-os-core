# SESSION_252 — ADR-0086 Instalação e Update OTA: processo unificado + execução completa (2026-08-05)

**Escopo:** Consolidar instalação (ADR-0079) + update (ADR-0031 §1 / #308 / ADR-0074) numa ADR
canônica e **implementar todos os gaps** — o AIOS agora instala, baixa o cérebro, se auto-atualiza
com rollback, e reporta telemetria ao dev.
**Status:** ADR-0086 Accepted · 10 gaps fechados · 8 commits · 0 erros (6 warnings Known).

---

## 1. Documentação (ADR-0086 — processo canônico)

- **`docs/architecture/0086-instalacao-e-update-ota.md`** (nova, Accepted, `fazendo`):
  - §0 inventário de fontes (ADR-0079 + plan, ADR-0031 §1, ADR-0074, IDEA_BANK #176/#306–310/#417–423)
  - §2 instalação (AutoInstaller, modos, menu live/install, adaptação ao silício, autobiografia)
  - §3 update OTA (A/B dual-slot, topologias QEMU/note real, loop de telemetria, imagem fixa, HTTP→mesh)
  - §5 gaps U1–U6 / I1–I12 com origem rastreada
- **Deprecações (processo → 0086):** ADR-0079, ADR-0079-plan, ADR-0031 §1; INDEX + headers atualizados.
- **Descoberta:** ADR-0074 **não tem arquivo** (só referência no codemap/`git_thin.rs`) — lacuna
  registrada no INDEX; conteúdo consolidado na §3.3.

## 2. Implementação (10 gaps fechados)

### Update OTA
| Gap | Entregue | Commit |
|-----|----------|--------|
| **U2** trigger imediato | comando shell `update` → `check_for_update()` | 84a421b |
| **U6** update no GPT instalado | filtro `0xEF` (ESP FAT32) nos 4 pontos do self_update + UPDATE.CFG na ESP (build.rs) | 84a421b |
| **U1** elo de boot | `switch_slot()` promove slot inativo → `kernel.elf` (path fixo do Limine) + BOOTCFG | fef1eb7 |
| **U4** rollback automático | `rollback()` promove slot bom com guarda `tries`; BootSelfHeal dispara em PANIC/GPU_HUNG | af4992a |

### Instalação / ciclo de vida
| Gap | Entregue | Commit |
|-----|----------|--------|
| **I9** `boot_media::mode()` | `k_nano/boot_mode.rs`: CONFIG.TXT BOOT_MODE + NeuralFS 0x7F = Installed; cacheado + set_boot_mode | ff8362a |
| **I6** AutoInstallerAgent órfão | registrado no AgentFleet + comando shell `install` (publica SYS_INSTALL) | 4783c50 |
| **I4** ModelProvisioner | `model_provisioner.rs`: baixa slots vazios (HwExpert→RustCoder→Reranker→Active) via URL do UPDATE.CFG + register_bytes; comando `provision` | 486a22c |
| **I5** leitura NeuralFS no boot | `try_hub_slot_neuralfs` (monta 0x7F, lê /models/+/boot/) antes do FAT32; `persist_slot` grava /models/ | 7a89bdc |
| **I10** SELF.STATE | `k_ai/self_state.rs`: `sys/self_state` KV + `record_life_event` (narrativa L3); wiring boot + update | 8f1a4a9 |
| **I11** loop de telemetria | `log_agent.rs` POST /api/logs via `tcp_exchange` + `do_POST` no serve_update.py (grava `target/logs/`) | 194d278 |
| **I7** VRAM real | `bar_size_mb()` técnica PCI (0xFFFFFFFF→bits→restaurar) no BAR0 — fim do 2048 hardcoded | 7f006c9 |
| **I8** self_check real | `resolve_path("boot/kernel.elf")` + CRC32C vs INSTALL.CHK | e03e925 |
| **I12** imagem mini | `build_image.py --mini`: PACK_LLM=none + MODELS_SOURCE=network no CONFIG.TXT (~60MB) | 2f995a5 |
| **I3** agente executa install | `run_install_from_bus()`: SYS_INSTALL → source ATA + target AHCI/NVMe/USB | 69b618d |

### Limpeza
- **Stub hardcoded removido:** `CHANNEL_MANIFEST_URL` (IP QEMU), `UpdateChannel`, `poll_channel`,
  `channel_name` — todos mortos, URL do server vive só no UPDATE.CFG (ff8362a).

## 3. Decisões ponytail

- **U6:** em vez de suportar NeuralFS (centenas de LOC), usar a **ESP que já é FAT32 real (0xEF)** —
  o update fala o GPT instalado com ~15 linhas de diff.
- **U1:** não ensinar o Limine a ler slots — o `switch_slot()` **promove o slot → `kernel.elf`**
  (path fixo que o Limine já carrega). Zero mudança no bootloader.
- **I3:** a ESP copiada crua **é** o bootloader instalado (Limine + kernel.elf + UPDATE.CFG) —
  BootloaderInstaller dedicado desnecessário neste fluxo.
- **U3 (defer):** FNV-1a cobre integridade; Ed25519 = anti-tamper p/ server público — custo real é
  o server assinar (quebraria o fluxo dev). Reabrir quando update for público/mesh.
- **I12:** `PACK_LLM=none` já faz a imagem mini (llmfit_pack_filter respeita); só faltava o flag + MODELS_SOURCE.

## 4. Verificação

- `cargo check --release` — 0 erros (6 warnings = política Known Warnings do projeto).
- `cargo test -p hermes --lib self_update` — 2/2 PASS (manifest field, version ordering).
- `cargo test -p k_ai --lib self_state` — 1/1 PASS (phase names).
- `cargo test -p k-nano --lib boot_mode` — 1/1 PASS (mode codes).
- `serve_update.py` smoke: GET manifest 200, GET KERNEL.BIN 200 (17MB), 404 ok, **POST /api/logs 200**
  + arquivo `target/logs/neural-*.log` gravado íntegro.
- `build_image.py --mini`: imagem 128MB com raiz sem LLMs grandes (HwExpert + experts + LEGO + FW).

## 5. Lições

1. **Hardcoded de ambiente ≠ contrato de layout.** Nomes 8.3 do FAT (KERNEL~1) e path do Limine
   (kernel.elf) são contratos — configurá-los quebra o sistema. IP de server (10.0.2.2) é dado —
   deve viver no config file (UPDATE.CFG). O AIOS não carrega endereço como constante.
2. **"Resolver o gap" pode ser escolher o alvo mais simples**: U6 pedia NeuralFS; a ESP FAT32 (0xEF)
   já resolve — o filtro é 1 linha, o mount+create_file seriam centenas.
3. **Agente que só loga o evento = dívida**: o AutoInstallerAgent escutava SYS_INSTALL mas nunca
   instalava. O wiring (evento → execução real) é o gap que vale; a API já estava pronta.
4. **Stub morto com valor de ambiente = lixo a apagar**, não configurar (YAGNI — se canais um dia
   nascerem, nascem do UPDATE.CFG).

## 6. Estado do ciclo de vida (ADR-0086)

```
onde estou (I9 boot_mode) → quem sou (I10 SELF.STATE) → instalar (I3/I6 → GPT dual)
→ 1º boot: provision (I4) → persiste (I5) → boot lê do disco
→ update diário (U2) → slot (U6) → promove (U1) → rollback em PANIC (U4)
→ telemetria (I11) → opencode analisa target/logs/ → gera fix → ciclo
→ imagem fixa ~60MB (I12): uma imagem, qualquer alvo
```

---

## 7. Continuação (mesma sessão) — fila ADR itens 11 e 5

### Item 11 — Backprop real + router treinado (ADR-0083, fechado)

O `TransformerTrainer` backprop real já existia (SESSION_246); o gap real era o **elo
treino→kernel**: o `train_router.py` exportava ROUTER.BITNET **v6** (ADR-0085 model_type=2,
posicional), mas o `load_router_from_file` (Rust) parseava o **formato v3 antigo** (header 20B +
names/tags + ntensors) → **nunca carregava** → fallback LCG (exatamente o log
`DETERMINISTIC FALLBACK (LCG seed=42)` da SESSION_251).

- `train_router.py`: fix `verify_roundtrip` para o layout v6 (era v3 — assert falhava) +
  fix import `bitnet_writer` (sys.path do repo raiz). Treino: **93.5% (29/31), gate ≥0.80 PASS**,
  exporta `tools/target/ROUTER.BITNET` (25.818 bytes) + report de matriz de confusão.
- `trinity.rs load_router_from_file`: reescrito para o layout v6 posicional (preamble
  magic/version/num_params/model_type + vocab/hidden/n_experts + embed f32 + weight i8).
- Teste host `load_router_v6_roundtrip`: blob v6 em memória → loader parseia (PASS).
- Cadeia completa: treino → export v6 → `find_file` (mkfat32 inclui `tools/target/`) → FAT →
  boot carrega → **fim do fallback LCG**. Commit 71c4eed.

### Item 5 — Market fetch v3 (ADR-0056, fechado)

`search_remote` tinha **hardcoded `http://10.0.2.2:8080`** — o anti-padrão da lição ADR-0086
(IP de ambiente é dado, vive no config file).

- `marketplace.rs`: `remote_base()` deriva a URL do **UPDATE.CFG** (reuso `read_update_cfg`) —
  mesmo server do update OTA; `search_remote` sem hardcoded.
- `serve_update.py`: endpoint `GET /api/search?q=` lista pacotes/modelos de
  `target/models/`, `target/` e `tools/target/` (testado: `?q=ROUTER` → ROUTER.BITNET 25.818B).
- `ALLOWLIST_HOSTS` mantido (é allowlist de hosts permitidos, não URL default).
- Commit 973bde5.

### Smoke QEMU (A2) — refinamento em andamento

`tools/smoke_ota_cycle.ps1` (base `run-qemu-p2p-mesh.ps1`: TCG puro, OVMF, `-no-reboot`):
Ato 1 (install no target.raw) alcançou o **Runtime** (evidência parcial) mas o `sendkey`
via monitor TCP não acionou o shell (pipeline teclado→Hermes). Refinamento pendente: debug do
envio de scancodes (usb-kbd) ou trigger por outro canal.

---

## 8. Revisão profunda do NeuralFS + compatibilidade NeuralFS/MHI/SGDB (mesma sessão)

### 8.1 Correções NeuralFS (oracle F1-F16 + BAFS/LiberFS) — commit `f07834f`

| Fix | Severidade | O que era | Correção |
|-----|-----------|-----------|----------|
| **F1** | CRÍTICO | free-stack LIFO entregava blocos invertidos/não-contíguos ao extent → corrupção silenciosa na re-escrita | `alloc_contiguous()`: popa, ordena, valida contiguidade; fallback bump |
| **F2** | ALTA | reclaimava blocos antigos ANTES dos novos existirem → power-loss/ENOSPC destruía a versão boa | dados novos → cow folha → commit → **só então** reclaim; no erro devolve os novos |
| **F3** | ALTA | `probe_magic` só lia bloco 1; primário corrompido + backup bom = format destruía tudo | probe com fallback ao backup; volume existe mas mount falha → **nunca formata** |
| **F5** | MÉDIA | journal com CRC inválido montava mesmo assim (`dirty` nunca lido) | `recover` falhou → mount retorna None (log CRÍTICO) |
| **F6** | MÉDIA | re-format podia sofrer replay de transação velha | format zera a região do journal |
| **F8** | MÉDIA | `read_file` materializava 792MB na RAM | `read_range(ino, offset, len)` — AirLLM streaming |
| **F10** | MÉDIA | create_file/dir aceitavam `..`, `/`, `\`, NUL | `valid_name()` rejeita todos |
| **F12** | MÉDIA | `extent.rs` + `checksum_tree.rs` dead (sem consumers) | removidos + facades atualizadas |
| **F13** | BAIXA | `Superblock::new` morto e com layout divergente | removido |
| **F14** | BAIXA | smokes level2/power_loss existiam sem caller | wireados no bootstrap_ram |
| **F15** | BAIXA | hack redundante de root update em write_file | removido (cow_leaf_for_key já atualiza) |
| **F16** | BAIXA | `commit_tx` sem flush de dispositivo (só fence CPU) | flush barrier: free_list → sync_cache → journal → sync_cache → sb (padrão LiberFS) |

**Licença corrigida (lib-1):** BAFS é **GPL-3.0** desde v1.2 (não MIT) — TECNOLOGIAS.md corrigido.
BAFS upstream congelado (0 issues/PRs); LiberFS (Unlicense) emerge como referência de
flush-barrier + freeing-deferido; littlefs2-pure como referência de torn-write test.

### 8.2 Compatibilidade NeuralFS/MHI/SGDB (oracle C1-C10) — commit `6a8f379`

| Achado | Severidade | Correção aplicada |
|--------|-----------|-------------------|
| **C1** | CRÍTICA | TickvLite gravava no LBA 2048 (colidia com ESP+NeuralFS do GPT instalado — brick no 1º boot NVMe real) → região movida para o **fim do disco** (antes da backup GPT) |
| **C2** | ALTA | backend=RAM reportado como ok mas volátil (SELF.STATE/episódica evaporam sem aviso) → log CRÍTICO no init_flash |
| **C4** | MÉDIA | EpisodicMemory reescrevia o tail completo a cada record (amplificação O(n) no flash) → fonte única doc-por-episódio |
| **C9** | BAIXA | provision não registrava meta no SGDB → `persist_slot` grava `pkg/model/<file>` (file+bytes+sha256) — ponte NeuralFS↔SGDB |

**Pendências documentadas:** C6 (ArcCache morto — wire como wrapper BlockDevice ou delete),
C5 (MHI hinting-only — wire `record_access` no read_range ou podar), C7 (vocabulário "tiers"
cognitivos vs físicos), C8 (rebuild de índice a cada boot).

### 8.3 Estado real do NeuralFS (para o doc)

- **Inodes/dirents são INLINE** na leaf B-tree (48B items, 84/folha) — o "inode de 128B em
  bloco dedicado" era design-only (removido).
- **Sem checksum de dados**: CRC32C cobre B-tree/superblocos/journal; `checksum_tree_root` fica 0.
- **Journal não-circular** confirmado; `sfence` ≠ flush — flush real via `sync_cache` agora.
- **Mapa canônico**: modelos/firmware = NeuralFS `/models/`; kernel/slots/BOOTCFG = FAT32 ESP;
  memória IA (L2-L7, HANR, audit) = SGDB→TickvLite (partição dedicada C1); SELF.STATE =
  NeuralFS write-through (futuro); índices ART/BQ = RAM-only rebuild do TickvLite.
