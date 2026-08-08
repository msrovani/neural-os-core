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

---

## 9. Revisão família ADR-0047 + reconciliação SASOS/CE + ADR-0087 implementada (mesma sessão)

### 9.1 Revisão "uma a uma" das ADRs 0047 (usuário: "vi 3 adr 47, uma hmi")

| ADR | Estado | Veredito |
|-----|--------|----------|
| 0047-Latent | Accepted (MVP parcial) | ✅ nenhuma ação — SGDB já cobre o gap narrativo; MHI agora tem dono (0087) |
| **0047-GPU** | Accepted (MVP parcial) | ⚠️ **§7 SASOS vs ADR-0087 CE**: duas abordagens para o Tier 0 → reconciliados como complementares |
| 0047-HMI | Superseded→0058 | ✅ sem overlap DMA |

**Reconciliação (commit `0b11354`):** SASOS (0047-GPU §7 = VRAM no heap, acesso pontual por ponteiro)
+ CE/SDMA/BCS (0087 = DMA bulk via engine) **não são concorrentes — são complementares**.
SASOS decide ONDE o dado vive (ponteiro); CE decide COMO moves bulk acontecem (engine).
Roadmap 0087 Fase 4 dividida: **4a SASOS** (dá o ponteiro, pré-requisito lógico) antes de
**4b CE** (dá a velocidade). WC vs UC: SASOS mapeia WC p/ gravação de VRAM via CPU, UC p/ leitura.
INDEX + 0047-GPU §7.2 + 0087 §2.0.1 atualizados.

### 9.2 ADR-0087 implementada — Fases 1–5 ✅ (7 commits, ~1.200 LOC, 22 testes host)

| Fase | Entrega | Commit | Verificação |
|------|---------|--------|-------------|
| Pré-req 4a | **Detecção medida de BARs** — `k_nano::pci::read_bar_size` (0xFFFFFFFF) + `detect.rs` seleciona MMIO/VRAM por tamanho real (VRAM = maior BAR ≥64MB; AMD dGPU VRAM→BAR0 ⇒ MMIO=BAR5; APU sem BAR grande → DRAM compartilhada) | `f0e5911` | check 0 erros |
| 1 | **NVMe PRP zero-copy** — `nvme_prp_layout` (PRP1/PRP2/lista 512, regras Linux), `io_nvm` com prp1+prp2 (cdw8/9 antes ficavam 0 = quebra >1 página), `read/write_blocks_direct` | `c222cdc` (fix-1) | 3 testes host |
| 2 | **MHI wiring** — `record_access` com callers reais (disk write `io_scheduler_flush`, disk read `readahead_hint` lba*512; `vram_alloc` registra/`vram_free` unregister/`msched_record` acessa); `hot_hits` + histerese (streak ≥2, LWN 898766); `tier_id`; VRAM na escada | `c222cdc` (fix-2) | 6 testes host |
| 3 | **Intel BCS** — 4 bugs: `BLT_RING_BASE` 0x220000→**0x22000**, TAIL +0x38→**+0x30** (0x38 = RING_START), CTL 4096→**0x3001**, blit header 0x41000000 (XY_COLOR_BLT!)→**0x54F00008** (XY_SRC_COPY_BLT, depth no DW1, DW3 x2/y2) + **MI_FLUSH_DW** 0x4C000001; sem BB_END no ring (engine pararia antes do TAIL); pin GGTT | `c4634be` | check 0 erros |
| 4a | **SASOS real** — `map_page_uc_at`/`map_region_uc_2mb_at` (VA arbitrário) + `init_sasos_vram` mapeia aperture em 0x4020_0000_0000+ UC; `sasos_vram_ptr`/`sasos_phys_to_ptr` (ponteiro CPU unificado); substitui PoC simbólico | `9346cd4` | 1 teste host |
| 4b | **NVIDIA CE Pascal** — channel dedicado (classe 0xc1b5, privileged inst\|0x20, runlist CE, USERD fence), DMA_COPY phys→phys (apertures 0x0260/0x0264 SRC=0x1000/DST=0x2000, 0x0400×8, launch 0x0300), canário 64KB RAM→VRAM→RAM golden; `mhi_tier0_copy()` seam | `2fd3acc` (fix-3) | 3 testes host |
| 5 | **Policy** — `DEMOTION_ORDER` explícita + `demote_to()` + `migration_rate_ok()` (64MB/janela 100 ticks, LWN 898766) no `mhi_tick` | `f6ddc89` | 9 testes host |

Fase 6 (AMD SDMA + SGL + P2P) permanece **AWAITING_HW** (ADR-0087 §4). Pesquisa AMD VRAM (lib-1,
amdgpu source): dGPU RDNA VRAM→BAR0/doorbell→BAR2/MMIO→BAR5 (Bonaire+), ReBAR expõe VRAM total
(sem ReBAR aperture ≈256MB), APU = carveout de RAM sem BAR, SDMA ring offsets + packet COPY
4MB/vez + fence via SDMA_OP_WRITE + polling wb.

### 9.3 Lições da sessão (ver AGENTS.md)

- **AMD BAR roles ≠ NVIDIA**: amdgpu (Bonaire+) mapeia VRAM→BAR0, doorbell→BAR2, MMIO→BAR5 —
  o código local assumia VRAM=BAR2/MMIO=BAR0. Como AMD era AWAITING_HW, o bug era invisível.
  Fix de raiz: **medir o tamanho real dos BARs em runtime** e atribuir roles por evidência.
- **Intel BCS**: 0x22000 (não 0x220000); TAIL=+0x30 (0x22038 é RING_START); XY_SRC_COPY_BLT =
  0x54F00008 (0x41 = XY_COLOR_BLT!); MI_FLUSH_DW = 0x4C000001 (0x02000000 é MI_FLUSH pré-gen6).
- **Ring submission ≠ batch**: não usar MI_BATCH_BUFFER_END no ring (engine para nele, HEAD nunca
  alcança TAIL → wait_idle timeout); ring vazio ⟺ HEAD==TAIL.
- **NVMe PRP**: PRP1 só vale se o transfer cabe numa página; >1 página precisa PRP2
  ou lista (512 entradas/página); cdw8/9 nunca eram setados (bug latente multi-página).

---

## 10. Loop QEMU OTA — Jarbas + comunicação com o server python (mesma sessão)

### 10.1 Fechamento do ciclo OTA e2e (ADR-0086 A2)

**Objetivo (usuário):** loop boot→log→correções→kill→restart até o neural subir o Jarbas
**e se comunicar com o server python** (serve_update.py). Além disso: WiFi + SMP para HW real.

**Resultado — comunicação VALIDADA:**
- Jarbas sobe (Hermes-PnP ready, DisplayAgent, 55 agents, Runtime)
- `GET /UPDATE.MANIFEST` → **200** e `GET /KERNEL.BIN` → **200** no serve_update.py
- Download do KERNEL.BIN (17MB) com **tamanho exato** (17415280)

**Bug-fixes descobertos e corrigidos no caminho (10 commits):**

| # | Fix | Commit |
|---|-----|--------|
| 1 | **scancode_to_ascii era stub** (None p/ tudo) — teclado morto em HW real e QEMU (sendkey nunca acionava o shell) | 0c4e661 |
| 2 | **Scheduler rate-limit** matava de fome agents passivos (`Pending >50x → skip 80%` → polled=1 → input/rede paravam) — `set_urgency` isenta interativos | b5d846e |
| 3 | **BudgetManager.reset_all sem callers** — ticks_used acumulava → todos Paused após ~103 polls → polled=0 | 09cb434 |
| 4 | **smoltcp clock**: TIMER_TICKS (PIT 18.2Hz ≈ 55ms) tratado como ms → relógio 55× lento → RST do slirp → download 17MB truncava 1748B | 8f517d9 |
| 5 | **Content-Length não validado** — RST/FIN precoce virava "Done" | 8f517d9 |
| 6 | **Checksum TCP RX ignorado** (só Tx) — payload corrompido aceito | 8f517d9 |
| 7 | **json_field** não tolerava `": "` do json.dumps → manifest=no_version | 272eef9 |
| 8 | **serve_update.py**: `--token ""` não desligava auth (args.token or random) → 401 | 272eef9 |
| 9 | **FAT32 short-write**: arquivo < 512B em cluster spc=2 → PANIC (WIFI.CFG/BOOT.LOG) | c203eb9 |
| 10 | **PIC fallback 0xFA** mascarava IRQ1 (teclado) — 0xF8 corrige | c203eb9 |
| 11 | **ESP GUID**: bytes.fromhex (BE) vs GPT little-endian misto → ESP virava 0xEE → UPDATE.CFG missing | c203eb9 |
| 12 | **Trigger OTA via flag QEMu-loader** (padrão netmode.flag) + scripts ota_launch/ota_loop | d32f301 |

**Residual documentado (ora-1, 2ª investigação):** o download agora tem tamanho EXATO mas o
hash ainda difere (corrupção de conteúdo não-determinística) — **frame allocator não exclui
kernel/heap/page tables** (`init_from_usable_ranges` só marca MEMMAP_USABLE como livre); um
`deallocate_frame` de frame vivo (gguf_mmap.rs:121, dma.rs) devolve ao pool → e1000 aloca como
buffer RX → DMA sobrescreve o heap (conn.buf) **depois** do checksum validar. Fix proposto:
excluir kernel/.bss.heap/page tables no init + auditar deallocs (unmap antes do free).

**WiFi/SMP:** código A0-A6 completo e wired (a3_on_bind via generic_wifi; init_smp/
wake_aps_sequential no boot); HW-gated (QEMU sem QCA6174 → AWAITING honesto). SMP funciona em
QEMU TCG (-smp 2); validação de timing de AP em HW real pendente.

## 11. Causa raiz do hash_mismatch — NÃO era frame allocator (SESSION_252 continuação)

A hipótese do ora-1 (§10 residual) era: frame allocator entrega frames do kernel/heap ao e1000 RX → DMA sobrescreve conn.buf. **Investigação provou que está ERRADA.**

### 11.1 Evidência que refuta o frame allocator

Logs de diagnóstico (memmap completo + endereços físicos):

| Região | Endereço físico | Tipo memmap |
|--------|----------------|-------------|
| Kernel (522MB, incl. .bss.heap 512MB) | `0x983bd000..0xb8dc0000` | type=6 (KERNEL_AND_MODULES) |
| RX buffers e1000 (64×4KB) | `0x613c5000..0x61405000` | type=0 (USABLE) |
| conn.buf (heap talc) | dentro do kernel image | type=6 |

- Kernel em `0x98...` (2.4GB), RX buffers em `0x61...` (1.5GB) — **nunca colidem**
- O Limine reporta o kernel como type=6 (KERNEL_AND_MODULES), NÃO USABLE → frame allocator nunca entrega
- `MEMMAP_KERNEL_AND_MODULES` no protocolo Limine = **6** (não 1!) — o código usava 1 (RESERVED), corrigido
- Reserva correta: `kernel_region fallback: reserva 0x983bd000 len=0x20a03000` (522MB)

### 11.2 A causa raiz real: bug no SHA-256 do guest

O download estava **ÍNTEGRO** o tempo todo. O bug era no `k_nano::tpm::sha256`:

```rust
// ANTES (buggy): sha256_pad colocava block[0] = 0x80
fn sha256_pad(total_len: usize) -> ([u8; 64], bool) {
    let mut block = [0u8; 64];
    block[0] = 0x80;  // ← ERRADO: 0x80 no índice 0
    ...
}
// E o montador copiava pad_block[remaining..64] → 0x80 (em [0]) NUNCA entrava
for i in remaining..64 { last[i] = pad_block[i]; }
```

Para qualquer mensagem com `len % 64 != 0` (kernel.elf: `17415976 % 64 = 40`), o byte `0x80` do padding SHA-256 **ficava fora do bloco final** → hash errado **deterministicamente**.

**Prova:** reproduzi a lógica exata do Rust em Python → `rust-buggy = 91e4e6a6...` bate exatamente com o `got` do guest em 3 rodadas consecutivas. O `got` era idêntico entre rodadas com o mesmo arquivo (determinístico) — o que descartava race de DMA (que seria não-determinístico).

### 11.3 Fix

`crates/k_nano/src/tpm.rs` — reescrito o padding inline no `sha256()`:
- `last[remaining] = 0x80` (índice correto, não 0)
- Caso `remaining >= 56` (two_blocks): bloco extra separado com só `bit_len` (sem 0x80)
- Removido `sha256_pad` (substituído pelo padding inline correto)

Adicionados 3 vetores de teste FIPS 180-2 (incl. caso two_blocks):
- `sha256_abc` (len=3, remaining=3) → `ba7816bf...`
- `sha256_empty` (len=0) → `e3b0c442...`
- `sha256_two_blocks` (len=56, 64, 63) → hashes FIPS

### 11.4 Por que o mesh/TLS "funcionavam" com o sha256 bugado

O sha256 bugado produzia hash **deterministicamente errado** mas **consistente** — dois nós usando a mesma implementação chegavam ao mesmo hash. O mesh TOFU (`peer_public_key()` + HMAC-SHA256) e TLS (fingerprints auto-geridos) eram **self-consistent**. Só falharia contra referência externa (ex: pin de certificado gerado por OpenSSL/hashlib).

### 11.5 Verificação

- 3 testes sha256 no k_nano: **PASS**
- Workspace completo (k_nano 86, cortex 46, hermes 20, jarbas 5, k_ai 17): **PASS** (bench `dod_10m_100k` skipado — OOM pré-existente, não relacionado)
- Loop OTA e2e: **`fetch=OK bytes=17415904 sha256=b42a2a...` + `KERNEL~2 written`** — hash bate, slot inativo escrito

**Lição crítica:** corrupção "com tamanho exato + hash determinístico" = bug no hash, não na transmissão. Sempre valide a implementação criptográfica contra vetores FIPS antes de investigar a rede.
