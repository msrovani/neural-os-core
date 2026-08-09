# Changelog â€” neural-os-core v2.0 "Ring Buffer Refactor"

## [Unreleased]

### SESSION_254: Crash ip=0 com loader 4GB + heap lazy AIOS (2026-08-09)

**Fix `8901d97` — 3 arquivos, 0 erros:**

- **Stack do Limine reservada (root cause do #PF ip=0x0):** `StackSizeResponse` ganhou
  o campo `address` (ABI real do Limine: revision + address) e o frame allocator
  agora reserva os 2MB da stack (`reserve_range`). Antes o allocator tratava a
  região da stack (~2.44GB) como livre — com o loader BITNET2B @4GB as alocações
  extras do boot empurravam o watermark até lá, entregando frames da própria
  stack do kernel → return address corrompido → `#PF ip=0x0000000000000000`.
- **Heap lazy AIOS (premissa 4):** removido o `resize_bump_heap` eager (1024MB e
  teto 1536MB no T+0); piso 512MB + `grow_bump_auto` sob demanda (256MB/passo,
  já existia no OOM do LazyBumpAllocator). Menos gordura no T+0 = menor janela
  do crash + o modelo 2B v6 carrega sem reserva eager.
- **Validação QEMU TCG (BITNET2B + HWEXPRT4 + hw_expert_v4):** MVP-C demo OK
  (ponto exato do crash), auto-grow 512→768→1024MB, `LLM LOADED file=577MB
  h=2560 L=30` (antes llm=ABSENT).

### SESSION_252 (cont.): Loop QEMU OTA — Jarbas + server python validados (2026-08-07)

**Objetivo (usuário):** WiFi + SMP para HW real + loop boot→log→correções→kill→restart até o
neural subir o Jarbas e se comunicar com o server python (serve_update.py).

**Comunicação OTA VALIDADA:**
- Jarbas sobe (Hermes-PnP ready, DisplayAgent, 55 agents, Runtime)
- `GET /UPDATE.MANIFEST` → **200** e `GET /KERNEL.BIN` → **200** no serve_update.py
- Download do KERNEL.BIN (17MB) com **tamanho exato** (17415280)

**12 fixes em 10 commits** (bugs reais de HW, muitos latentes):
- **Teclado morto:** `scancode_to_ascii` era stub (None p/ tudo) — sendkey nunca acionava o shell
  (0c4e661). **Scheduler:** rate-limit matava de fome agents passivos → `set_urgency` isenta
  interativos (b5d846e); `BudgetManager.reset_all` sem callers → todos Paused → polled=0
  (09cb434). **Rede/OTA:** smoltcp clock TIMER_TICKS≠ms (PIT 18.2Hz ≈ 55ms → relógio 55× lento →
  RST slirp → download truncava 1748B) + Content-Length não validado + checksum TCP RX ignorado
  (8f517d9); json_field `": "` do json.dumps + serve_update `--token ""` (272eef9); FAT32
  short-write <512B PANIC + PIC fallback 0xFA mascarava IRQ1 teclado + ESP GUID BE vs LE on-disk
  → UPDATE.CFG missing (c203eb9); trigger OTA via flag QEMu-loader + scripts ota_launch/ota_loop
  (d32f301).
- **Residual (ora-1):** hash_mismatch do KERNEL.BIN = frame allocator não exclui kernel/heap/
  page tables (`init_from_usable_ranges`) → dealloc de frame vivo → DMA e1000 sobrescreve o heap.
  Fix proposto documentado (exclusões no init + auditoria de deallocs).
- **WiFi/SMP:** código A0-A6 completo e wired (a3_on_bind via generic_wifi; init_smp/
  wake_aps_sequential); HW-gated (QEMU sem QCA6174 → AWAITING honesto). SMP funciona em QEMU TCG.

**Doc:** AGENTS.md +12 lições; IDEA_BANK OTA e2e ✅; SESSION_252 §10; SESSION_INDEX.


### SESSION_252 (cont.): ADR-0087 implementada — MHI Real DMA Multi-Tier Fases 1–5 ✅ (2026-08-06)

**Revisão ADR-0047 + reconciliação SASOS/CE (0b11354):** 0047-GPU §7 (SASOS = VRAM no heap,
acesso pontual por ponteiro) + ADR-0087 (CE/SDMA/BCS = DMA bulk via engine) reconciliados como
**complementares** — SASOS decide ONDE o dado vive, CE decide COMO moves bulk acontecem.
Roadmap: Fase 4a (SASOS, dá o ponteiro) antes de 4b (CE, dá a velocidade). WC p/ gravação
VRAM via CPU, UC p/ leitura.

**ADR-0087 — MHI Real (7 commits, ~1.200 LOC, 22 testes host, 0 erros):**

- **Pré-req 4a — detecção medida de BARs (f0e5911):** `k_nano::pci::read_bar_size`
  (técnica 0xFFFFFFFF) + `detect.rs` seleciona MMIO/VRAM por tamanho real, não tabela DID.
  🔴 **Bug de raiz AMD invisível:** amdgpu (Bonaire+) mapeia **VRAM→BAR0, doorbell→BAR2,
  MMIO→BAR5** — o código assumia VRAM=BAR2/MMIO=BAR0 (o oposto). AIOS mede o silício.
- **F1 — NVMe PRP zero-copy (c222cdc):** `nvme_prp_layout` (regras Linux: PRP1 só, PRP2 =
  2ª página, lista ≥3 páginas, 512 entradas); cdw8/9 antes ficavam 0 = transfer >1 página
  quebrado; `read/write_blocks_direct(lba, dma_phys, len)` para callers MHI.
- **F2 — MHI wiring (c222cdc):** `record_access` (era ZERO callers) agora chamado de disk
  write `io_scheduler_flush`, disk read `readahead_hint` (lba*512), `vram_alloc`/`vram_free`/
  `msched_record`; histerese `hot_hits` (streak ≥2, LWN 898766), `tier_id`, VRAM na escada.
- **F3 — Intel BCS (c4634be):** 4 bugs de encoding (i915 source): base 0x220000→**0x22000**,
  TAIL +0x38→**+0x30** (0x38 é RING_START), CTL 4096→**0x3001** (RING_CTL_SIZE|VALID), blit
  header 0x41000000 (**XY_COLOR_BLT**!)→**0x54F00008** (XY_SRC_COPY_BLT 0x53, depth no DW1,
  DW3 x2/y2, src_pitch) + **MI_FLUSH_DW** 0x4C000001 (não o MI_FLUSH antigo 0x02000000); sem
  MI_BATCH_BUFFER_END no ring (engine pararia antes do TAIL); pin GGTT.
- **F4a — SASOS real (9346cd4):** `map_page_uc_at`/`map_region_uc_2mb_at` (VA arbitrário) +
  `init_sasos_vram` mapeia aperture em 0x4020_0000_0000+ UC; `sasos_vram_ptr`/
  `sasos_phys_to_ptr` (ponteiro CPU unificado, base p/ `Tensor::location = MemTier::Vram`).
  Substitui o PoC simbólico.
- **F4b — NVIDIA CE Pascal (2fd3acc):** channel dedicado (classe 0xc1b5, privileged
  inst|0x20, runlist CE, USERD fence), DMA_COPY phys→phys (apertures 0x0260/0x0264
  SRC=0x1000/DST=0x2000, 0x0400×8, launch 0x0300), canário 64KB RAM→VRAM→RAM golden;
  `mhi_tier0_copy()` seam p/ MHI tier1→tier0. HW-gated (GTX 1050).
- **F5 — policy (f6ddc89):** `DEMOTION_ORDER` explícita + `demote_to()` (um degrau por vez) +
  `migration_rate_ok()` (64MB/janela de 100 ticks, LWN 898766) no `mhi_tick`.

**Fase 6 (AMD SDMA + SGL + P2P)** = AWAITING_HW (ADR-0087 §4). Pesquisa AMD VRAM (lib-1):
dGPU RDNA VRAM→BAR0/doorbell→BAR2/MMIO→BAR5, ReBAR expõe VRAM total (sem ReBAR aperture
≈256MB), APU = carveout de RAM sem BAR, SDMA ring offsets + packet COPY 4MB + fence
SDMA_OP_WRITE + polling wb. Ver SESSION_252 §9.

**Doc:** ADR-0087 atualizada (Fases 1–5 ✅ + pré-req 4a), INDEX (0047-gpu/0087), SESSION_252 §9.


### SESSION_252 (cont.): Revisão profunda do NeuralFS (F1-F16) + compatibilidade NeuralFS/MHI/SGDB (C1-C10) — 2026-08-05

**NeuralFS — correções (oracle + BAFS/LiberFS):**

- **F1 CRÍTICO** — `alloc_contiguous()`: o free-stack LIFO entregava blocos invertidos/
  não-contíguos ao extent → corrupção silenciosa na re-escrita (reescrever um modelo
  corrompia o arquivo). Agora ordena e valida contiguidade, fallback bump.
- **F2** — ordem CoW correta: dados novos → cow folha → commit → **só então** reclaim dos
  antigos (antes destruía a versão boa em power-loss/ENOSPC). No erro, devolve os novos.
- **F3** — mount seguro: `probe_magic` com fallback ao backup (bloco 2); volume existe mas
  mount falha → **nunca formata** (wipe de /models/ evitado). BAFS/LiberFS: freeing deferido.
- **F5** — journal com CRC inválido → mount recusa (não monta transação parcial).
- **F6** — format zera o journal (sem replay de tx velha de formato anterior).
- **F8** — `read_range(ino, offset, len)`: destrava AirLLM streaming (read_file materializava
  792MB na RAM, estourando o heap antes do model_fit).
- **F10** — `valid_name()`: create_file/dir rejeita `..`, `/`, `\`, NUL, controle.
- **F12** — dead code removido: `extent.rs` + `checksum_tree.rs` (sem consumers; facades
  hermes/bin atualizadas).
- **F13** — `Superblock::new` removido (morto, layout divergente do format).
- **F14** — smokes `level2` + `power_loss_soft` wireados no bootstrap_ram (antes sem caller).
- **F15** — hack redundante de root update removido.
- **F16** — flush barrier no commit_tx: free_list → sync_cache → journal → sync_cache → sb
  (padrão LiberFS; QEMU/RAM mascarava ausência de flush).
- **Licença** — BAFS é **GPL-3.0** desde v1.2 (não MIT) — TECNOLOGIAS.md corrigido. BAFS
  upstream congelado (0 issues/PRs); LiberFS (Unlicense) como referência de flush+defer.

**Compatibilidade NeuralFS/MHI/SGDB (C1-C10):**

- **C1 CRÍTICO** — TickvLite gravava no LBA 2048 (colidia com ESP@2048 + NeuralFS@4096 do
  GPT instalado → brick no 1º boot NVMe real). Região movida para o fim do disco (antes da
  backup GPT).
- **C2** — backend=RAM reportado como ok mas volátil → log CRÍTICO no init_flash.
- **C4** — EpisodicMemory: fonte única doc-por-episódio (removida a reescrita O(n) do tail).
- **C9** — ponte provision↔SGDB: `persist_slot` registra `pkg/model/<file>` (bytes+sha256).
- **Pendências documentadas:** C6 (ArcCache morto), C5 (MHI hinting-only), C7 (tiers
  cognitivos vs físicos), C8 (rebuild de índice a cada boot).

**Doc:** NeuralFS.md §13 (estado real: inodes inline, sem checksum de dados, journal
não-circular, mapa canônico por tipo de dado). Commits f07834f + 6a8f379.
cargo check 0 erros; testes k-nano 62 + k_ai 19 PASS.


### SESSION_252: ADR-0086 Instalação + Update OTA — processo unificado + execução completa (2026-08-05)

ADR canônica consolidando instalação (ADR-0079, deprecada) + update (ADR-0031 §1, deprecado)
+ ADR-0074 (lacuna sem arquivo). **10 gaps fechados em 8 commits, 0 erros.**

- **Update OTA:**
  - `U1` — `switch_slot()` promove o slot inativo → `kernel.elf` (path fixo do Limine) + BOOTCFG;
    zero mudança no bootloader (ponytail: usa o kernel.elf que o Limine já carrega)
  - `U2` — comando shell `update` → `check_for_update()` (teste sem esperar 24h)
  - `U4` — rollback automático: `rollback()` promove o slot bom com guarda `tries` (1=pendente,
    0=limpo, evita loop); BootSelfHeal dispara em PANIC/GPU_HUNG pós-desligamento inesperado
  - `U6` — update no disco GPT instalado: filtro `0xEF` (ESP FAT32) nos 4 pontos do self_update
    + UPDATE.CFG gravado na ESP pelo build (SysInstaller copia a ESP setor-a-setor → herda)
- **Ciclo de vida AIOS:**
  - `I9` — `boot_mode::mode()`: lê CONFIG.TXT (BOOT_MODE) + detecta NeuralFS 0x7F = Installed;
    cacheado + `set_boot_mode` p/ menu live/install
  - `I10` — `SELF.STATE` na SGDB (`sys/self_state`) + `record_life_event` (narrativa episódica L3);
    wiring no boot (fase via boot_mode) e no update — **autobiografia do OS**
  - `I6` — AutoInstallerAgent registrado no AgentFleet + comando shell `install` (publica SYS_INSTALL)
  - `I3` — agente **executa** a instalação: `run_install_from_bus()` (source ATA → target AHCI/NVMe/USB)
  - `I4/I5` — ModelProvisioner baixa slots vazios (URL do UPDATE.CFG) + persiste em `/models/` na
    NeuralFS + boot lê da NeuralFS (imagem fixa: baixa uma vez, carrega do disco)
  - `I11` — telemetria dev↔neural: `log_agent` POST `/api/logs` + `do_POST` no serve_update.py
    (grava `target/logs/neural-*.log`) — opencode analisa a quente
  - `I7` — VRAM real via técnica PCI de tamanho de BAR0 (fim do 2048 hardcoded)
  - `I8` — `verify_install_checksum` real (resolve_path + CRC32C vs INSTALL.CHK)
  - `I12` — `build_image.py --mini`: PACK_LLM=none + MODELS_SOURCE=network (~60MB, alvo baixa o brain)
- **Limpeza:** removido stub morto `CHANNEL_MANIFEST_URL`/`UpdateChannel`/`poll_channel`/
  `channel_name` (IP de QEMU hardcoded) — URL do server vive **só** no UPDATE.CFG
- **Deprecações:** ADR-0079, ADR-0079-plan, ADR-0031 §1 (processo → 0086); ADR-0074 = lacuna
  registrada no INDEX; IDEA_BANK #308a/b/c/#421 apontam para a 0086
- **Decisões ponytail:** U6 = ESP FAT32 em vez de NeuralFS; U1 = promover slot→kernel.elf;
  U3 (Ed25519/TPM) = **defer** (hardening, FNV-1a cobre integridade)
- **Verificação:** `cargo check --release` 0 erros (6 warnings Known); testes hermes 2/2,
  k_ai self_state 1/1, k-nano boot_mode 1/1; serve_update smoke (GET/POST/404);
  `build_image --mini` testado (raiz sem LLMs grandes)


### SESSION_251: Tier 0+1 ADR (0041/0083a/0045/0082) + fix raiz reboot loop IST (2026-08-05)

Implementação da fila ADR por complexidade (itens 1–4 do TODO) + desbloqueio do boot.

- **0041 aceite QEMU** — evidência em `docs/evidence/boot-whpx-20260805.txt` (WHPX):
  `QUEUE_NOTIFY pci` (NotifySent ≥1×), `[VirtIO] [h4] OK: 1/1`, `h5_demo R1=Allow
  R3_no_cap=Deny FE_no_bind=Deny`, `AS restore CR3 OK`, `BOOT: MVP-C` + `P2–P9` OK,
  57 agents + Runtime, sem #GP/#PF fatal.
- **0083a** — warn honesto no fallback LCG (`init_router_weights`); `ROUTER.BITNET`
  confirmado no FAT.
- **0045** — cutover de áudio já estava feito (`e51a48b`); ADR-0045, `jarbas_bridge.rs`,
  TODO e STATE reconciliados (truth=jarbas; residuals: soft-float/VITS, UAC AWAITING_HW,
  dedup HDA pendente).
- **0082 Onda CPU** — `ns::HW` + `populate_hw_namespace` em `store.rs` boot_init:
  `hw/cpu/*`, `hw/cache/*`, `hw/mem/total_mb`; log `Onda CPU: /hw/* populado`.
- 🔴 **Fix raiz do reboot loop** (commit 2662d50 veio com boot quebrado, SESSION_250):
  o GDT do k_nano referenciava `TSS_ARRAY[0]` cru (ISTs zerados) e o lazy_static `TSS`
  que preenche os ISTs nunca era dereferenciado → entrega de #PF/#GP/timer com IST
  fazia push para VA 0 → #DF (CR2=0xfffffffffffffff8) → triple fault. Fix:
  `Descriptor::tss_segment(&*TSS)`. Checks `HUGE_PAGE` e3/e2 adicionados ao
  `map_page_direct` (prescrição SESSION_250 §4).
- **Verificação:** `cargo check --release -p neural-kernel -p k-nano -p k_ai -p cortex` 0 erros.


### SESSION_250: AIOS na veia — RAM física → HMI → auto-adaptação + Boot do 2B (2026-08-05)

Premissa do dono: o AIOS deve **ler a memória física disponível, elencar no HMI
e se auto-adaptar** (heap sob demanda; AirLLM/layer-streaming se necessário).

- **Heap self-adapting:** `heap_initial_mb = clamp(75% RAM detectada, 512..1536)`
  no boot (era `resize_bump_heap(2048)` hardcoded) + **`grow_bump_auto`** — o
  bump cresce sob demanda (+256MB/passo) quando atinge `HEAP_LIMIT`, com
  verificação `heap_pte_present` pós-mapeamento e re-try. Log:
  `[HEAP] [AIOS] - heap auto-alvo=1536MB (RAM detectada=9216MB; grow sob demanda)`.
- **Gate AirLLM:** `model_fit::needs_airllm(params, file_mb)` — modelo + heap
  > 75% RAM ⇒ layer-streaming (residente vs AirLLM). `estimate_heap_mb` clamp
  derivado da RAM (não mais hardcoded 128..2048). Logado no load.
- **HMI:** SysInfoAgent (card 9001) já expõe RAM/heap/frames.
- **2B v6 convertido:** `target1/bitnet_2B.bitnet` (792MB canônico: act=RELU2,
  embed=Q6_K, feat=0x07, theta=500000). Encoder Q6_K **vetorizado** (numpy):
  328M elementos de horas → 0.012s.
- **Scan autodescritivo:** `cortex::model::v6_file_size` deriva o tamanho do
  header (era const v4 604MB que truncava o v6 → parse lixo).
- 🔴 **Wrap 2⁶⁴ no bump heap (diagnóstico oracle):** `HEAP_BUFFER` @ high-half;
  `heap_start + offset` envolve 2⁶⁴ em ~2044MB — o 2B (offset 2158MB) escrevia
  em VA 0 → #PF CR2=0 no memcpy (`rep movsq`). Tentativa HEAP_EXT_BASE revertida
  (map_page_direct sem check HUGE_PAGE em todos os níveis → reboot loop no
  boot-time resize). Known-issue: check HUGE_PAGE em todos os níveis = fix real.
- **Verificação:** `cargo check --release --workspace` 0 erros.


### SESSION_249: Formato canônico `.bitnet v6` (ADR-0085) + Fidelidade 2B4T (ADR-0084) (2026-08-05)

Implementação completa das ADRs 0085 (formato canônico + registro K³CHJ) e 0084
(engine BitNet — fidelidade + kernels CPU). Fases F0–F6b + F1b:

- **F0** — `tools/bitnet_writer.py` (writer canônico numpy-only) + `save_model_v6`
  (cortex.rs) + golden; `v6_writer_parity` byte-exact PASS. Goldens un-ignored no
  .gitignore (eram engolidos por `*.bin` e quebrariam testes em clone fresco).
- **F1+F1b** — 8 conversores → v6: convert_bitnet (2B4T: act_type=RELU2, embed Q6_K),
  train_hw_expert_v4 (model_type=HWEXPERT, sem prefixos), train_router (posicional),
  convert_gguf (feat só bit2, act_type do metadata, D3 tied), convert_falcon3
  (2 normas sintéticas removidas; theta no EOF — bug real corrigido),
  convert_safetensors (container ilegível → body LLM real, fail-loud em schema não-LLaMA),
  prepare_extra_models (scales 1.0, rms_ffn_norm intermediate), train_models_gpu
  (**silu no forward de treino** §10.3 — retreino TinyStories/RustCoder pendente).
- **F2** — `load_model_v6`/`load_llm_v6` estritos: reserved==0, feat bits 3-7 rejeitados,
  tied⇒zero bytes de unembed (D3), `rms_ffn_norm`=intermediate_size (D2), theta só bit2;
  fallback v3/v4 com WARN.
- **F3/F4** — kernels ADR-0084: unpack branchless `(pair&1)-(pair>>1)` (era match/peso),
  consts de tiling, activation-parallel gated `m>=8`.
- **F5** — fidelidade M1–M4: `act_type` nos 4 forwards (relu2/silu via `ffn_act`),
  eps RMSNorm 1e-6→1e-5, embed Q6_K (encoder Python + loader bytes brutos +
  `embed_lookup` row-wise + `unembed_logits` por super-bloco), theta parametrizado,
  `bitnet_fwd_parity.py` fortalecido (magic 0xBE11BE11, default 2B4T, gate logit-level ≤0.5%).
- **F6+F6b** — `cortex::model` (ModelKind/ModelView + dispatcher), `ModelHub::register_bytes`,
  4 sites LLM do main.rs roteados via `load_model_v6` (fallback legado mantido).
- 🔴 **Bug latente crítico:** `f16_to_f32` (gguf.rs) — `sign = (bit>>15 as f32) * -1.0`
  produzia `-0.0` para todo f16 positivo, zerando TODOS os dequants GGUF (Q4_0/Q5_0/Q6_K).
  Fix: `if bit==1 {-1.0} else {1.0}`. Descoberto pelo teste Q6_K cross-check.
- **Verificação:** 18 testes cortex PASS (parity, `q6k_decode_matches_python`,
  `v6_roundtrip_load` host — desrisca o boot sem o 2B); 142+ testes workspace;
  `cargo check --release --workspace` 0 erros.
- **Pendentes por design:** boot QEMU v6 com 2B real (download ~3GB safetensors 2B4T),
  F7 W2A8 (gated WHPX/HW real + gaps de geração), retreino TinyStories/RustCoder (GPU).
- ✅ **Boot QEMU v6 validado (pós-commit):** `synth_v6.bitnet` (108KB, h=128 L=2 feat=0x07)
  injetado via `-device loader,addr=0x100000000` — kernel varreu magic, `load_model_v6`
  parseou (`v6 LLM h=128 L=2 vocab=512 act=0 emb=0 feat=0x07`), `AI_READY`, AgentFleet
  54 agentes + Runtime + NetAgent tick. Falta só a validação com o 2B real.

### SESSION_249b: Fase 7 W2A8 (ADR-0084 F4) + retreino silu (2026-08-05)

- **W2A8 maddubs implementado** (`cortex::bitnet_w2a8`): ativações int8 (si per-token via
  absmax) × pesos ternários i8 via `_mm256_maddubs_epi16` (32 MACs/instrução), acumulação
  i32, epílogo com escala f32 + desconto do viés (u8 = q+128 ⇒ subtrai 128·Σw). Repack
  coluna-major (n,k) i8 p/ maddubs. Self-test de paridade vs referência quantizada PASS.
- **Gated (ADR-0084 §3 F4):** `w2a8_enabled()` = false — exige WHPX/HW real + gaps de
  geração resolvidos (`GENERATION_GAPS_RESOLVED` static, hoje false). Kernel real compila
  só em host/test (`not(target_os="none")`); stub no_std gated (target x86_64-unknown-none
  desabilita -ssse3 → LLVM "split the result" no pmaddubsw 256-bit; SESSION_247: gate por
  target, não cfg(test)). Dispatch conectado atrás do gate — não regride TCG.
- **Retreino silu (#3):** ✅ **concluído na GPU (SESSION_249b)** — descoberta: a GTX 1050
  **funciona** com o torch 2.13+cu126 (`arch list` inclui sm_61; o drop sm_61 era do cu130,
  não do cu126). Sem `CUDA_VISIBLE_DEVICES=0` o torch reporta device_count=0 apesar da GPU
  presente; com ela: GTX 1050 4.3GB, cuda=12.6. `--rustcoder --epochs 50` (1M params) →
  `rust_coder.bitnet` v6 canônico silu (act=0, feat=0x07) exportado. Arquivo pronto para
  FAT/loader. (O script já seta `CUDA_VISIBLE_DEVICES=0` na linha 28.)
- **Verificação:** 20 testes cortex PASS (incl. 2 W2A8), workspace release 0 erros.

### SESSION_248: Veredito de Arquitetura — HW Expert v4 NN não identifica hardware além da tabela (2026-08-04)

12 lanes de medição exaustiva (explorers + fixers + oracle): diagnóstico H2 (artefato degenerado),
retreino validado (Rust-exact forward + STE), sweep QEMU (3/60 = 5%), controle decisivo (fp32
same-arch = 60.58% = majoritário), MLP probes (todas as variantes ≤58.97%, stage-2 sem imbalance
63.27%). **Veredito definitivo:** o transformer ternário colapsa para majoritário e NÃO é a
quantização (controle fp32 idêntico); a arquitetura (atenção truncada q_dim=32 + mean pool) é
a vilã. MLP no alvo vendor (famílias de driver dos nomes pci.ids) placa em 63.27% → teto =
SINAL (vid:did → família de driver ~59-63%, nomes cobrem 54.7%). **NENHUMA arquitetura
atinge o gate de 65%** em família específica de devices nunca vistos. Reivindicação
"260KB NN ≥ DB 40MB" REFUTADA.

Ações no kernel (ea696c3):
- uild_card → tabela (100% conhecidos) + heurística class byte; branch NN removido.
- prediction_to_card preservado com #[allow(dead_code)] + protocolo de re-habilitação.
- predict_all_pci → no-op (predições NN erradas não entram no SGDB).
- cargo check --release: 0 erros.

Infraestrutura entregue: sweep QEMU multi-device (	ools/hw_sweep/), validator Rust-exato
(alidate_hw_expert_v4*.py com gate parse_end==size, nonzero, holdout do ARQUIVO), split
honesto 90/10 por (vid,did) seed 42, controle contínuo (probe_continuous_arch.py), MLP
probes (probe_mlp_vendor*.py), relabel com ground-truth independente (dataset_class_v2.json
12 genérico + dataset_class_v3.json 21 vendor), docs/evidence (holdout benchmark + runtime
sweep + architecture verdict). Protocolo de re-habilitação: restaurar branch em uild_card +
provar ≥65% específico no protocolo honesto (split + sweep).

Commits: 79ac8e5 493fcd cbaf1a5 3f9dc51 5d4f67c ea696c3.
### SESSION_247: HW Expert v4 artefato degenerado â†’ retreino validado + ADR-0084 + CI + testes host (2026-08-04)

O artefato `models/hw_expert/hw_expert_v4.bitnet` era **100% zeros** (embed 2002/2048
nÃ£o-zero; todos os 42 tensores backbone e 5 heads = 0) â€” root cause confirmada
(H2): `export_v4` quantiza com threshold 0.5 e `nn.Linear` inicia Â±1/âˆš128â‰ˆÂ±0.088 â†’
todo peso vira 0; o kernel NÃƒO tinha bug (parse exato; port Python reproduz
family=0). CorreÃ§Ãµes (cargo check --release: 0 erros; cargo test host 139 testes):

- **Retreino validado.** `tools/retrain_hw_expert_v4.py`: split honesto 90/10 por
  (vid,did) Ãºnico seed 42 (holdout nunca visto), early stopping (patience 3),
  threshold de export tunÃ¡vel (0.5/0.25/0.1/0.05 â†’ escolhe o que maximiza acc do
  ARQUIVO com fraÃ§Ã£o nÃ£o-zero â‰¥1%), embed ROW-MAJOR `wt(f, embed.weight)` (o `.T`
  embaralhava). `tools/validate_hw_expert_v4.py`: port Rust-exact (parse_end ==
  file size, header, fraÃ§Ã£o nÃ£o-zero GATE â‰¥1%, prediÃ§Ãµes nÃ£o-constantes, holdout
  do arquivo).
- **Loader v5 real do export_v4.** `cortex.rs`: `num_params` u32 (nÃ£o u64),
  tensores com prefixo `u32 len + u32 scale` (scale vestigial â†’ 1.0 efetiva),
  rope 16 f32/layer, 5 heads prefixed â€” `read_prefixed_ternary`/
  `read_prefixed_f32_vec`.
- **SSE tail clamp.** `bitnet_sse.rs`: n%4â‰ 0 real (heads 17/9/10) â€” clamp do
  Ãºltimo bloco (`lanes = min(4, n-j)`).
- **Tabela curada > ML.** `hw_capability.rs`: `build_card` ordem invertida â€”
  tabela HWID SEMPRE vence, ML cobre o resto, heurÃ­stica por Ãºltimo.
- **cargo test no host.** Lib crates `#![cfg_attr(not(test), no_std)]`; HW-only
  gated com `#[cfg(target_os = "none")]` (NÃƒO `cfg(test)` â€” inerte em dep).
  Fixes: IDT `cfg(not(windows))` (MSVC/COFF align 16), `probe_port` stub host,
  IPI no-op host, p2p_sim gated `feature="p2p-sim"` (stale prÃ©-7a97556), ProofGate
  verify com `now` explÃ­cito, `mutation_hash_for` espelha ruvix-vecgraph (senÃ£o
  ProofRejected), transport L2 frame slice + src_mac, telemetry ring
  newest-disappear (0..4095), datetime fix (1783929600 = 2026-07-13), NVMe layout
  72B pinado (spec 64B â€” AWAITING_HW), rabin determinÃ­stico, FedYogi honesto
  (pesos i8 ficam 0 com lr=0.01 â€” limitaÃ§Ã£o conhecida), quarantine lowercase
  (`[inst]` â€” `[INST]` era dead code), skill_manifest interop asserts.
- **CI.** `.github/workflows/ci.yml`: check + test host + build boot image + disco
  + QEMU boot smoke (UEFI TCG -NoDisk, grep "Phase 6" + "tick=").
- **ADR-0084 (Proposed).** Engine BitNet: fidelidade 2B4T (M1 relu2 vs silu, M2
  SubNorms, M3 theta 500000, M4 embed Q6_K) antes de velocidade; F1 decode
  branchless â†’ F2 activation-parallel gated por m â†’ F3 fidelity+Q6_K (bump de
  versÃ£o, +190MB RAM) â†’ F4 W2A8 gated â†’ F5 tiling; receita 1-bit p/ prÃ³ximo treino
  (tanh 30Ã—, LR constante+cooldown, LRs separados, QAT suave Hestia). Sem retreino
  de modelos existentes.
- **Scrub docs.** README reescrito (sem superlativos, status honesto Working/gated,
  badge CI real); CONTRIBUTING: cessÃ£o de IP â†’ DCO + AGPL inbound=outbound;
  AGENTS.md toolchain cross-platform + seÃ§Ã£o cargo test.
- **Sweep runtime.** `tools/hw_sweep/` (3 boots QEMU ~15-20 devices PCI, modelo
  pinado @0x179000000 â€” LLAMA8B cobre a janela de auto-placement).

### SESSION_246: Auditoria TÃ©cnica 7.x â€” Gap da Camada de IA (2026-08-03, ADR-0083)

Auditoria (seÃ§Ã£o 7): infra de inferÃªncia real; a inteligÃªncia que ela deveria servir
nÃ£o existia â€” roteador MoE era ruÃ­do LCG, treino era regressÃ£o de 64 pesos sem
backprop, saudaÃ§Ã£o demo era pool canado. CorreÃ§Ãµes (cargo check --release: 0 erros):

- **7.2 â€” Roteador MoE carregÃ¡vel de arquivo.** `load_router_from_file()` lÃª
  `ROUTER.BITNET` (.bitnet v3+: `router_embed` 99Ã—64 f32 + `router_weight` 64Ã—N i8);
  boot tenta NVMeâ†’AHCIâ†’ATAâ†’USB-MSC. `load_router(embed, weight, trained)` loga
  **honestamente**: "loaded (trained)" vs warn "DETERMINISTIC FALLBACK (LCG seed=42,
  UNTRAINED)".
- **7.3 â€” Backprop real.** `TransformerTrainer` (k_ai::cognitive): train_forward
  (attention full causal) + backward analÃ­tico (CEâ†’unembedâ†’rmsâ†’camadas GQA/RoPE/
  FFN groupedâ†’embed) + update STE ternÃ¡rio. `self_test()` PASS no boot QEMU:
  CE 2.7018â†’1.7487 (20 steps, modelo sintÃ©tico hidden=16).
- **7.5 â€” SaudaÃ§Ã£o sem pool canado.** Removidos `argmax_row_greeting_only` +
  `GREETING_BIAS_IDS`/`greeting_*` (bpe.rs); saudaÃ§Ã£o usa argmax real do modelo.
  Clima mantÃ©m constrained decode (saÃ­da estruturada).
- **Router treinado.** `tools/train_router.py`: 111 utterances curated PT/EN +
  ~1863 templates; holdout 31 curated, acurÃ¡cia **93.5%** (gate â‰¥80% PASS);
  artefato `models/ROUTER.BITNET` (27.780B) + matriz de confusÃ£o em
  `docs/evidence/router-confusion-matrix-20260804.md`.
- **Assets opcionais.** `mkfat32.py`/`mkexfat.py` incluem `ROUTER.BITNET` se existir.
- **Fixes prÃ©-existentes revelados por `cargo clean`** (xhci): `% ISOC_SLOTS` type
  mismatch, match incompatÃ­vel, use-after-move do ring UVC (`configure_uvc_endpoint`
  â†’ `Option<()>`).
- ADR: `docs/architecture/0083-ai-layer-gap-auditoria.md` (Accepted) + INDEX.

### SESSION_245: Auditoria SeguranÃ§a 6.1â€“6.4 â€” modelo de confianÃ§a unificado (2026-08-03)

Auditoria tÃ©cnica encontrou 4 lacunas; todas corrigidas (cargo check --release: 0 erros).

- **6.1 â€” portÃ£o Ãºnico ADR-0052 para skills.** `verify_skill_md` (auto-evoluÃ§Ã£o)
  agora **delega** para `verify_artifact_md(PackageKind::Skill, â€¦)` â€” mesmo contrato:
  schema 1, kind, name sanitizado, goal/contexto/acionaveis/tokens/provenance/
  sandbox_status, 7 seÃ§Ãµes `## `, content_hash, assinatura Ed25519. `verify_and_register`
  reordenado para **sign-first** (assinatura Ã© parte do contrato verificado; fail-closed
  se assinar falhar). Generators (`skill_gen`, `skill_observer`, `matrix_learn`,
  `llm_skill_prompt`) passam a emitir o contrato completo. Seeds embedded carregam via
  novo `register_trusted_skill` (trusted-by-compilation, precedente SESSION_230).
  Callers extras corrigidos: `/learn` (bin + hermes) e caminho LLM no bin.
- **6.2 â€” documentaÃ§Ã£o honesta dos anÃ©is.** AGENTS.md explicita que R0â€“R3 sÃ£o
  organizaÃ§Ã£o de cÃ³digo (camada lÃ³gica), NÃƒO fronteira de seguranÃ§a do processador â€”
  tudo executa CPL=0; isolamento real = wasmi (A) + Ring3 gated (ADR-0077).
- **6.3 â€” token de capacidade fail-closed.** `CapabilityToken::Ed25519(_)` deixou de
  retornar `true` cego; agora `false` (payload sem mensagem vinculada + crate leaf
  sem crypto = nÃ£o verificÃ¡vel = invÃ¡lido). Nada construÃ­a Ed25519 â€” sem regressÃ£o.
- **6.4 â€” entropia real.** `mix_session_seed` usa `k_nano::hw_rng` (RDRAND, fallback
  ChaCha20) quando `probe_done() && rdrand` (gate ADR-0082); RDTSC/ticks viram stir
  secundÃ¡rio.

### SESSION_244: NeuralFS fonte Ãºnica (consolidaÃ§Ã£o triplicata) (2026-08-03)

NeuralFS existia em triplicata (k_nano/hermes/bin) violando a liÃ§Ã£o SESSION_237.
ConsolidaÃ§Ã£o para fonte Ãºnica na crate base:

- `k_nano::neural_fs` agora Ã© canÃ´nico: agent avanÃ§ado (USB opt-in, exFAT write,
  usb_trust, ecosystem tree ADR-0051, GPT virgin) movido do bin, prefixos adaptados
  para `crate::`, `impl FilesystemAgent` convertido em mÃ©todos `pub` inerentes
  (trait Ã© ring-local; k_nano nÃ£o pode depender dos rings).
- `hermes/src/neural_fs/` â†’ facade `pub use k_nano::neural_fs::*` (12 cÃ³pias deletadas).
- `neural-kernel/src/neural_fs/` â†’ facade idÃªntica (3 arquivos locais deletados).
- Adapter `impl FilesystemAgent for k_nano::...::NeuralFsAgent` nos dois `fs/mod.rs`
  (orphan rule OK: trait local + tipo canÃ´nico).
- **Novo guarda `tools/check_duplication.py`**: exit 1 se o mesmo `.rs` (nÃ£o-facade)
  existe em â‰¥2 crates â€” "nada avisa" resolvido. Primeira execuÃ§Ã£o lista dÃ­vida
  prÃ©-existente (camada fs/, camada net, espelhos cortex/k_ai) como follow-up.
- `cargo clean -p neural-kernel && cargo check --release`: 0 erros.

### SESSION_243: Isolamento Ring3 de ProduÃ§Ã£o â€” ADR-0082 Fases 1â€“4 (2026-08-03) âœ…

**ADR-0082** depreca ADR-0041 Â§P9+ para escopo Ring3 (docs em `docs/architecture/0082-*.md`).

**Fase 1 â€” FundaÃ§Ã£o:**
- `address_space::create_sandbox_as()` from-scratch (kernel supervisor-only P4[â‰¥256], sem PTs compartilhadas) + `frame_for_virt()`.
- `interrupts::TSS_ARRAY[8]` per-process (RSP0/IST dedicados) + `switch_to_proc_tss(pid)` no run_process.
- `demo_ring3` usa `create_sandbox_as` (fix bug higher-half `clone_current`).
- SYSCALL/SYSRET fast path: `init_syscall_fast_path()` (wrmsr LSTAR/STAR/FMASK) + naked entry + `dispatch_syscall`.

**Fase 2 â€” ELF Loader + Sandbox:**
- `elf_loader.rs`: merge com ADR-0076 (preservou API) â€” create_sandbox_as, flags RX/RW por segmento (PF_X), relocations `R_X86_64_RELATIVE` (PIE base=0), `elf_boot_self_test()`.
- `run_elf()` no user_mode; `ring3_run_native()` implementado (isolation_ring); `host_send_tcp_payload()` real (udp_exchange).

**Fase 3 â€” W^X Arena USER + WASM B/C:**
- `set_user_leaf_flags()` (flip RWâ†’RX preserva USER) + `jit_write_exec_user(aspace, code)` + `user_arena_self_test()`.
- `ring3_run_native()` dual path: ELF64 | blob nativo (Cranelift B/C).
- app_factory B/C jÃ¡ gated por `isolation_ring_available()` = `native_ring_registered()`.

**Fase 4 â€” ValidaÃ§Ã£o (3 bugs corrigidos):**
- SYSCALL/SYSRET gated por hypervisor real: `probe_done() && hv âˆˆ {None, Kvm}` â€” WHPX rejeita `wrmsr` dos MSRs â†’ #GP no boot (TCG mascarava). Fallback `int 0x90`.
- `jit_write_exec_user`: escrita via HHDM no frame (VA do sandbox nÃ£o existe no CR3 kernel â†’ #PF).
- `user_arena_self_test`: valida folha+bytes, nÃ£o executa em Ring 0; `elf_boot_self_test`: offsets ELF64 corrigidos (e_phentsize@54/e_phnum@56).

**VerificaÃ§Ã£o (TCG 2 cores 8G sem disk):** boot completo 8 fases + Runtime + tick â€” P6 Ring3 OK (marker=3352494e470001), ELF selftest PASS, USER arena PASS, P7/P8/P9 OK, AgentFleet 54 agents, WASMI add(2,3)=5, ISO-RING gated (TCG=UNSAFE, wasmi A). `cargo check --release` = 0 erros.
**Commits:** `8d3eb90` (F1+2) Â· `1450108` (F3) Â· `6b073bf` (fix WHPX) Â· `4c7a2e9` (fix F4).
**Pendente:** validaÃ§Ã£o HW real / WHPX estÃ¡vel (canÃ¡rio `ring3_is_safe` = KVM).

---
### SESSION_242: Mesh P2P Reliability â€” ADR-0081 Phase 2 Complete (2026-08-02) âœ…

**Short-term (Critical):**
- **ACK seletivo por fragmento** â€” `FRAG\0` â†’ `FRACK\0` stop-and-wait (3 retries, 50 tick timeout); elimina retransmissÃ£o de payload inteiro quando 1 fragmento perde.
- **Exponential backoff no probe_node** â€” timeout dobra a cada falha: 50â†’100â†’200â†’400â†’800â†’1600â†’3200 ticks (cap 3200); reduz carga na rede durante falhas prolongadas.
- **Health TTL automÃ¡tico** â€” cleanup a cada 500 ticks remove entradas sem atividade > 60s (6000 ticks); evita vazamento de memÃ³ria em tabelas estÃ¡ticas.
- **MÃ©tricas latÃªncia** â€” `avg_rtt_ticks` (EWMA Î±=1/8), `p99_rtt` via buffer circular 32 amostras + insertion sort; `peer_p99_rtt(node_id)` exposto.

**Medium-term:**
- **ARP cache / MAC resolution** â€” `PEER_MAC_CACHE` (16 slots), populado via heartbeat RX; `peer_mac()`/`peer_set_mac()` para unicast futuro.
- **Capacity scoring dinÃ¢mico** â€” `MeshExpertDistributor` usa `peer_health().reachable`, `avg_rtt`, `p99_rtt` para ajustar capacidade base; nÃ³s unreachable â†’ capacidade 0.
- **Rate limiting broadcast** â€” token bucket global (1 token/tick, burst 20); heartbeat custa 1, ROLE custa 2, dados custam 3.

**Dashboard JSON:**
- `PeerHealth::to_json(node_id)` â†’ `{"node_id":N,"reachable":bool,"avg_rtt":N,"p99_rtt":N,"tx":N,"ack":N,"fail":N,"probe_to":N}`
- `publish_mesh_health()` emite JSON array `[{"node_id":1,...},...]` no tÃ³pico `MESH_HEALTH` (EventBus).
- `mesh_health_json::parse()` no_std no Jarbas â†’ `Vec<PeerHealthJson>`.
- Lazy subscribe `MESH_HEALTH` no `DisplayAgent::tick()`.
- Cards coloridos (verde/vermelho) com RTT, p99, TX/ACK, failures, probe timeout.

**VerificaÃ§Ã£o:** `cargo check --release -p k-nano/cortex/jarbas` â€” 0 erros.

---

### SESSION_241 (cont.): Mesh AEAD Tier F + anti-replay dados + calibraÃ§Ã£o ed25519 â€” ADR-0081 (2026-08-02) ðŸŽ¯
- **AEAD Tier F implementado** â€” primeira dep cripto simÃ©trica do workspace: `chacha20poly1305 0.11` (`default-features = false, features = ["alloc"]`); X25519 via feature `x25519` do prÃ³prio `ed25519-compact` (sem `x25519-dalek`, sem handshake novo no wire).
- **Wire:** `header NoProto 36B â€– ciphertext â€– tag16`; nonce 12B = `source_id` u32 BE â€– `clock` u64 BE (derivado do header, NÃƒO vai no wire â€” anti-replay garante nÃ£o-repetiÃ§Ã£o, NIST SP 800-38D contador); AAD = header; KDF = `sha256(DH(X25519_local_sk, peer_pk))` via `from_ed25519`.
- **RX order:** len-check â†’ TOFU â†’ anti-replay CHECK â†’ decrypt â†’ clock UPDATE (update sÃ³ apÃ³s auth â€” previne forged-high-clock DoS).
- **Escopo:** MR\0/EDR\0 (unicast request/response) encriptados; broadcasts (MW/ED/FD/FM/CRDT/SKILL/PROMOTE/offer/sync) permanecem assinados â€” sem chave Ãºnica de recipiente (documentado). Fail-closed: sem chave/peer desconhecido = Full Ed25519, zero regressÃ£o.
- **Anti-replay dados Tier L:** `next_data_clock()` estrito-monotÃ´nico via `GLOBAL_LOGICAL_CLOCK.tick()` nos 12 sites `AiosTaskPacket::new` (MW/MR, ED/EDR, FD/FM, CRDT, SKILL, PROMOTE, MEM/CHK, ROLE); RX `clock <= last` â†’ DROP + `SEC_DROPPED_REPLAY++`. **Corrige falso drop cross-type** (heartbeat usava `TIMER_TICKS` ~10000 vs dados `clock=0`).
- **Build cfg:** `.cargo/config.toml` `--cfg chacha20_backend="soft"` + `--cfg poly1305_backend="soft"` â€” LLVM crash `STATUS_ILLEGAL_INSTRUCTION` com backend SIMD sob soft-float (mesmo padrÃ£o `polyval_force_soft`/`aes_force_soft`).
- **CalibraÃ§Ã£o ed25519-compact 2.3.1** (source confirmado: SEM SIMD, portable/scalar): verify 68.9Âµs/69.8Âµs/114.0Âµs e sign 65.5Âµs/68.3Âµs/162.3Âµs @ 300B/1200B/17.5KB (~14.3k ops/s) â€” faixa eBACS 26-46Âµs do ADR era otimista demais; corrigida.
- `cargo check --release` 0 erros; `cargo build --release` (boot image) OK. Tag v1.9.9-s241.
- **Nota:** `cargo nk` direto (O3 + `-Z threads=16`) crasha LLVM no codegen dos kernels AVX512 prÃ©-existentes do k_ai (`arch/x86_64.rs`) â€” prÃ©-existente, pipeline canÃ´nico (boot) nÃ£o afetado.

### SESSION_241: TLS Bridge Fix â€” hermesâ†’kernel wiring (2026-08-02) âœ…
- **Bug:** `hermes::tls` era dead code â€” `register_https_get()` nunca chamado no boot,
  consumers usavam HTTP-only (`net_bridge::resolve_and_http_get_safe`), fallback
  construÃ­a `http://host:443/path` (HTTP na porta TLS).
- **Fix (hermes/tls.rs):** Reescrito com `fetch_url(url)` dispatcher Ãºnico:
  `https://` â†’ kernel TLS via bridge, `http://` â†’ net_bridge HTTP. Fallback
  HTTP na porta 443 removido.
- **Wire (main.rs):** `hermes_crate::tls::register_https_get(crate::net::https_get)`
  no Phase 7. Bridge function pointer conectada.
- **Consumers (11 arquivos):** browser_agent, marketplace (3 calls), self_update (2),
  agents (/fetch, /scrape, model download), rss_agent, search_agent, git_thin,
  async_io â€” todos roteados para `crate::tls::fetch_url`.
- **lib.rs:** `pub mod tls;` adicionado.
- **Resultado:** TLS 1.3 (embedded-tls 0.19, HybridProvider, ECDSA+RSA-PSS) agora
  acessÃ­vel via `hermes::tls::fetch_url()` para todos os agents. `cargo check --release` 0 erros.

### SESSION_240: Tier cripto Relativizado (HMAC) vs Full (Ed25519) â€” ADR-0081 Fase B (2026-08-02) âœ…
- **DecisÃ£o (maintainer)**: mesmo range/subnet (datacenter) â†’ cripto "relativizada"
  em troca de velocidade (DADOS com HMAC-SHA256 + chave de segmento); mesh externo
  â†’ protocolo completo (Ed25519; AEAD na evoluÃ§Ã£o). Controle/TOFU SEMPRE Ed25519.
- **AnÃ¡lise de custo (eBACS/lib25519/dalek/OpenSSL, Zen 4-class)**: Ed25519 verify
  ~26-46Âµs/pacote (custo fixo) vs HMAC ~1.3Âµs @1.2KB â€” ~30x; verify limita
  throughput a ~0.3 Gbps/core vs ~8 Gbps (HMAC); em datacenter (RTT 0.1-0.5ms)
  a cripto Ã© +8-40% do RTT (visÃ­vel), em WAN Ã© invisÃ­vel â€” onde dÃ¡ pra relativizar
  o custo Ã© alto, onde nÃ£o dÃ¡ a rede engole. ImplementaÃ§Ã£o importa 3-4x (OpenSSL
  EVP ~100Âµs vs lib25519 ~32Âµs; usamos ed25519-compact sem SIMD).
- **feat(k_nano): `crypto.rs` (novo)** â€” `hmac_sha256` (RFC 2104/4231, reusa
  `tpm::sha256`, sem dep nova), `ct_eq` (constant-time), `hmac_self_test`
  (RFC 4231 caso 1, roda no boot).
- **feat(mesh): gate L/F** â€” `SEGMENT_KEY` + `crypto_tier()` + seam
  `set_segment_key(Option<[u8;32]>)` (`mesh.rs`); TX dados tiered
  (`sign_packet` â†’ HMAC 32B em Relativized / Ed25519 em Full;
  `sign_packet_authentic` para heartbeat/ROLE) e RX fail-closed tiered
  (controle sempre Ed25519; dados de peer conhecido â†’ tiered; falha â†’ DROP)
  em `udp_broadcast.rs`; Worker MR usa `verify_packet_tiered` (`compute.rs`).
  Fail-closed: sem chave = Full = comportamento atual (zero regressÃ£o).
- **docs(adr-0081)**: Fase B atualizada com tiers + tabela de custo + evoluÃ§Ã£o
  AEAD (X25519+ChaCha20) p/ Tier F externo. Anti-replay de dados em Tier L
  (clock=0 nos senders) = follow-up.
- **cargo check --release**: 0 erros.

### SESSION_239: Fase C ADR-0081 â€” experts + DSD + NodeTier + FL + CRDT (2026-08-01) âœ…
- **feat(mesh): experts distribuÃ­dos (C2)** â€” `mesh_distrib.rs`: Workerâ†’Master `ED\0`
  (lista de experts assinada+fragmentÃ¡vel), Masterâ†’Worker `EDR\0` (assign ponderado
  por capacidade, greedy). `capacity_weighted_assign`, `remote_experts`, `my_assignment`.
  Wire: bei_tick `poll_expert_requests` + `broadcast_local_experts` 1x no Worker.
- **feat(mesh): DSD SpeculativeDecoder** â€” `cortex/speculative.rs` (novo): draft_verify
  local (self-test accepted=8), stats, mesh_tick. VerificaÃ§Ã£o distribuÃ­da real = futuro
  (verifier MLP).
- **feat(mesh): NodeTier SKYNET (#315.27)** â€” `NodeTier L0-L4` + `score_bonus`
  (1.0-3.0) no capacity_score; `NodeCapabilities::new_tiered` (new delega L1);
  `set_local_caps`/`local_tier`. Heartbeat assinado Fase A inalterado (ponytail).
- **feat(mesh): FL federado (C5, #312f)** â€” `fl_trainer.rs`: Worker envia `FD\0`
  gradiente (packing 2-bit LSB-first) a cada ~200 ticks; Master agrega FedYogi +
  broadcast `FM\0` modelo global; Worker aplica (LWW global_round).
  `mesh_tick_global`/`fl_stats_global` wired no bei_tick.
- **feat(mesh): CRDT sync (C4, #315.26)** â€” `sgdb/crdt_sync.rs`: `CRDT\0` version
  sync real â€” Master publica v, Worker LWW merge, peer_versions. Merge de conteÃºdo
  ART/BQ = ponytail (hoje sync de versÃ£o).
- **PadrÃ£o**: TaskType::Inference (1) p/ FD/FM/CRDT/ED/EDR â€” evita colisÃ£o com
  skill_sync (3) e marketplace (4). Fase A preservada (ingress fail-closed).
- **VALIDADO QEMU dual**: CRDT publish bilateral sent=true (A=Master, B=Worker) +
  FL stats + matmul 64Ã—64 fragmentado round-trip. Commit `866e0e6`. 0 erros.

### SESSION_238: SeguranÃ§a Fase A + FragmentaÃ§Ã£o MTU (2026-08-01) âœ…
- **feat(mesh): Fase A seguranÃ§a (MITM fechado)** â€” RX fail-closed (pacote sem
  assinatura â†’ DROP; assinatura invÃ¡lida vs pk vinculada â†’ DROP; antes verificava
  com chave LOCAL contra assinatura do PEER â†’ sempre falhava â†’ fail-open).
  TOFU via `PK\0`+pk no heartbeat (`PEER_KEYS[(node_id, pk, clock); 16]`, seam
  SKYNET: `peer_public_key()` prÃ©-preenchÃ­vel por TEE attestation). Anti-replay
  (heartbeats `clock <= last` â†’ DROP). Todos os TX assinam (heartbeat, ROLE,
  skill push/promote, offer, MW/MR). Contadores `sec: unsigned/badsig/replay`.
  **Validado QEMU dual: sec=0/0/0** (zero drops legÃ­timos). Commit `e56e5d4`.
- **docs(adr-0081): veredicto BitTorrent** â€” NÃƒO implementar (ora-1+lib-1):
  camada = utilitÃ¡rio content-addressing na Transport R0 (sÃ³ modelos/Fase C +
  ADR-0046); ajuda 1 (merkle/infohash integridade) atrapalha 2 (DHT sybil, MSE
  sem auth); sem crate no_std completo (sÃ³ `bendy`); arXiv GenTorrent/KDN/
  BasedAI/Petals; BEPs public domain mas **uTP patenteado atÃ© 19/11/2027**.
  Subconjunto: merkle piece verification (~150 LOC, reusa `k_ai::merkle_audit`).
  Commit `e0fe270`.
- **feat(mesh): fragmentaÃ§Ã£o MTU + reassembly** â€” gate `>1200B â†’ fallback local`
  removido (limitava matmul grande/FL). `send_fragmented` (â‰¤1200B direto;
  >1200B â†’ chunks â‰¤1000B, header `FRAG\0` 21B: id/total/idx/len u32 LE) +
  `recv_fragmented` (reassembly 2 slots, fora-de-ordem OK, duplicatas via bitmask,
  timeout 500 ticks). FragmentaÃ§Ã£o apÃ³s `sign_packet`, reassembly antes de
  `verify_packet` (integridade preservada, Fase A intacta). `compute.rs` MW/MR
  via fragmentado; self-test 64Ã—64. **Validado QEMU dual**: matmul 64Ã—64
  ~17.5KB round-trip (18 frags TX/RX) `shape=(64,64) primeiro=2016.0`.
  Commit `916d155`.
- **cargo check --release**: 0 erros

### SESSION_237: Jcode-inspired memory integration (2026-08-01) âœ…
- **feat(memory): 4-tier consolidation (IDEA #218)** â€” `k_ai::tiers::consolidate_tiers(tick)`
  promove Workingâ†’Episodicâ†’Semanticâ†’Procedural (SGDB L1â†’L5): tÃ³picos top por frequÃªncia de
  palavras (stopword-filtered) â†’ docs L3 `topic/<name>` + L4 `sem/<tick>/<name>` (â‰¥2 ciclos
  estÃ¡veis) + snapshot L5 `proc/skills`; publica transiÃ§Ãµes `MEMORY_TIER` no EventBus; chamado
  no SleepCycleAgent CONSOLIDATE (`hermes/agents.rs`).
- **fix(memory): BGE statics single-source** â€” `neural-kernel/src/memory_systems.rs` era cÃ³pia
  duplicada de `k_ai::memory_systems`; o boot carregava BGE nos statics do bin e `k_ai` nunca via
  o modelo (recall rodava silenciosamente em pseudo-hashes 64d). Agora o bin Ã© sÃ³
  `pub use k_ai::memory_systems::*;` â†’ recall usa BGE 384d real pÃ³s-load.
- **feat(recall): gate de seguranÃ§a** â€” `gated_rag_context` filtra o recall antes da injeÃ§Ã£o no
  prompt: skip `"empty"` + blacklist de 10 padrÃµes de injection + cap 3 hits
  (`hermes/cognitive_bridge.rs`).
- **feat(skills): hint por embedding (jcode-style)** â€” skills indexadas como `skill:<name>` via
  `k_ai::memory_systems::index_embedding`; `find_skill_hint(intent)` faz semantic_search
  (sim â‰¥ 0.4) e anexa `[SKILL-HINT] <name>` ao system prompt; `invalidate_skill_index()` reseta
  (`hermes/skill_loader.rs`).
- **feat(swarm): CHANGE_NOTIFY** â€” `TOPIC_CHANGE = "CHANGE_NOTIFY"` + `publish_change(what, name)`
  nos pontos de mutaÃ§Ã£o de skill (evolve hot_swap/rollback, self_evolve verify_and_register,
  skill_sync mesh apply, wasmi_rt register_wasm_skill, bin `/learn`); SelfEvolveAgent drena e
  invalida o Ã­ndice de skills.
- **feat(ADR-0059): F5 promote wired** â€” `promote_ephemeral_to_wasm` (era log-only) agora gera
  wasm via `wasmi_rt::generate_wasm_module()` e promove via `EVOLVE_LEDGER.hot_swap` (sandboxed,
  rollback on failure). Native self-dev Ring3 segue gated por ADR-0060 (`TRY_ENTER_RING3=false`).
- **cargo check --release**: 0 erros.

### SESSION_236: Codemap â€” index completo do repositÃ³rio (2026-08-01) ðŸ—ºï¸
- **docs(codemap): atlas + 66 mapas hierÃ¡rquicos** â€” skill `codemap` rodado na base
  inteira: 8 fixers paralelos (1 por crate/tree, escopos disjuntos) geraram
  `codemap.md` por crate/submÃ³dulo (Responsibility / Design Patterns / Data &
  Control Flow / Integration Points), todos com sÃ­mbolos verificados por grep
  contra o cÃ³digo real (zero placeholders, zero mapas vazios).
- **docs(codemap): atlas raiz** â€” `codemap.md` na raiz: responsabilidade do
  projeto, entry points (Limine `_start` â†’ kernel_boot), tabela agregada de 13
  diretÃ³rios com links, cadeia de anÃ©is R0â†’R3 e comandos de refresh incremental.
- **docs: `## Repository Map` no AGENTS.md** â€” seÃ§Ã£o idempotente para agentes
  auto-carregarem o atlas a cada sessÃ£o.
- **infra: `.slim/codemap.json`** â€” estado de change-detection (739 files);
  refresh incremental via `codemap.mjs changes|update --root ./`.
- **Drifts docs-vs-cÃ³digo encontrados** (registrados no SESSION_236): 
  `probe_uefi_framebuffer` removido (â†’ `probe_raw_framebuffer`, SESSION_232);
  jarbas/audio agora Ã© a fonte Ãºnica (nota ADR-0045 stale); `neural-kernel/src/{fs,vfs,neural_fs}`
  sÃ£o espelhos legados NÃƒO compilados; `update_tecnologias.py` nÃ£o existe;
  `migrate_k2chj.py` arquivado; claim "bios.img" stale (Limine â†’ sÃ³ `uefi.img`).
- **cargo check --release -p neural-kernel**: 0 erros.

### SESSION_235 (item 4): Compute distribuÃ­do Workerâ†’Master via P2P real (2026-07-31) âœ…
- **feat(mesh): matmul ternÃ¡rio distribuÃ­do** â€” cortex feature `p2p` nova; o bloco
  `#[cfg(feature="p2p")]` do `dispatch_ternary` (que existia mas nunca compilava) agora
  funciona: Worker serializa w+x (`MW\0` + shapes u32 LE + packed_data 2-bit + x f32 LE,
  gate MTU 1200B), envia via udp_broadcast (TaskType::Inference), espera sÃ­ncrona
  (~200 TIMER_TICKS) a resposta `MR\0` (filtro dest_id); timeout â†’ fallback local.
- **feat(mesh): Master responde requests** â€” `poll_mesh_requests()` drena EventBus
  P2P_PACKET, computa com `ternary_matmul_adaptive` e responde. Gate "sÃ³ Master"
  removido (responde mesmo Undecided â€” sob TCG o Master pode ainda nÃ£o ter eleito).
- **feat(mesh): self-test distribuÃ­do** â€” `mesh_matmul_self_test()` 16Ã—16 (1107B â‰¤ MTU)
  + retry 5x no bei_tick (DIAG do boot roda antes da eleiÃ§Ã£o â€” nunca pegava o P2P).
- **VALIDADO QEMU dual**: `[B] matmul request node=3 size=1107 sent=true` â†’
  `[A] matmul resposta node=3 sent=true` â†’ `[B] matmul resposta node=3 ok
  shape=(16,16) primeiro=120.0 (mesh dispatch)`. Commit `b6ab13b`. 0 erros.

### SESSION_235: Mesh P2P aplicaÃ§Ãµes reais â€” Marketplace + PROMOTE + PapÃ©is (2026-07-31) âœ…
- **feat(marketplace): broadcast real** â€” `activate_global` popula `local_skills` do
  SKILL_REGISTRY canÃ´nico (14 skills, dedupe); antes nunca era chamado â†’ nada enviado.
  Throttle por TIMER_TICKS (scheduler rate-limited: 200 CALLS demoravam minutos sob TCG).
- **feat(promote): PROMOTE_SKILL real** â€” Worker envia `PROMOTE\0name\0desc` (NoProto Sync
  via k_nano); Master detecta prefixo e registra `DynamicSkill("promoted from mesh worker")`.
- **feat(roles): propagaÃ§Ã£o de papÃ©is real** â€” `assign_roles` envia `ROLE\0target\0role_u8`
  (broadcast, throttle 110 ticks); receptor filtra por `node_id()` e aplica via `set_role`
  (era ponytail "send role-assignment"). Primeiro uso de destino no mesh.
- **fix(eleiÃ§Ã£o): todos-Worker** â€” lazy-init do MESH_ENGINE usava MAC completo vs peers
  `[source_id,0,..]` â†’ comparaÃ§Ã£o lexicogrÃ¡fica sempre favorecia o peer. Fix: local usa
  `[node_id(),0,0,0,0,0]` (mesmo formato).
- **VALIDADO (2 QEMUs)**: A=Master node=2 (15 skills push + 14 offers broadcast sent=true),
  B=Worker node=3 (RX type=4 ModelUpdate + `role aplicado node=3 role=Memory`). 0 erros.
- Commits: `50bdf6b` (1+2+3), `e4917c1` (fix .data), `9239ac9` (node_id+tie-break).

### SESSION_234: P2P Mesh real entre 2 QEMUs + migraÃ§Ã£o transporteâ†’k_nano (2026-07-31) ðŸ†
- **feat(mesh): descoberta P2P funcionando de verdade** â€” duas instÃ¢ncias QEMU
  (10.0.3.2/10.0.3.3) trocam heartbeats via broadcast UDP 42069 na NIC real
  (e1000), com RX cruzado (A recebeu `clock=4796` enviado por B) e eleiÃ§Ã£o.
  Commits `f240fa4` (Fase A) + `0eec18f` (migraÃ§Ã£o).
- **feat(skillsync): Master push â†’ Worker apply** â€” 15 skills empurradas do
  Master (`broadcast=true`) e processadas no Worker via `poll_p2p` (EventBus
  topic `P2P_PACKET`). skill_sync (R3 hermes) e marketplace (R3 hermes+jarbas)
  consomem sem inversÃ£o de dependÃªncia.
- **refactor(mesh): transporte+serviÃ§o movidos do bin â†’ k_nano (R0)** â€”
  decisÃ£o do oracle (a intuiÃ§Ã£o do maintainer estava certa: mesh Ã© camada
  baixa de sistema). `udp_broadcast::{build_frame,send,recv}` + `mesh::p2p_tick`
  agora vivem em k_nano (que jÃ¡ tinha smoltcp+e1000+nic_globals). O bin
  `net.rs` re-exporta os statics NIC de k_nano (transporte R0 usa o MESMO NIC);
  non-heartbeat publicado no EVENT_BUS; `net_bridge` P2P removido (HTTP/TCP/DNS
  permanecem). `set_nic_config(mac,ip)` sÃ³ pÃ³s-configuraÃ§Ã£o (set_static_ip/DHCP).
- **fix(script): run-qemu-p2p-mesh.ps1** â€” ASCII puro (PS 5.1 lÃª sem BOM como
  ANSI), `$Root = $PSScriptRoot`, OVMF via caminho 8.3 (`C:\PROGRA~1\...`),
  `-m 8G` + `-smp 2` (MTTCG), switch `-NoDisk` (teste P2P Ã© rede pura â€” a
  leitura FAT32 dos modelos via ATA PIO sob TCG travava o boot).
- **cargo check --release**: 0 erros
- **Known**: `nodes=1` na eleiÃ§Ã£o (node_id = `local_role()` colide entre nÃ³s) â€”
  next: derivar node_id do MAC/IP real (10.0.3.2â†’2, .3â†’3).

### SESSION_233b: Ring3 triple-fault resolvido â€” boot QEMU 100% (2026-07-30)
- **fix(ring3): RSP=0 no `jump_back_to_kernel`** â€” `"xor ax, ax"` para zerar
  ds/es/ss clobberava o registro RAX que o compilador escolheu para o operando
  `{rsp}` â†’ `mov rsp, rax` com RAX=0 â†’ ret para RIP=0 â†’ #PF storm (CR2=rodata).
  Em long mode zerar segmentos era desnecessÃ¡rio (SS.RPL=0 vem do TSS no int 0x90).
- **fix(ring3): callee-saved restore** â€” handler `extern "x86-interrupt"` salva
  rbx/rbp/r12-r15 na stack RSP0 e o `jmp` pulava o epilogue â†’ restaurar em
  `jump_back_to_kernel` (CPL=0 + kernel CR3, statics acessÃ­veis).
- **RESULTADO**: `P6 SUCCESS iretq+CPL3 marker=3352494e470001 Cap::ENTER_USER`
  + `BOOT: P6 Ring3 OK` + scheduler vivo (tick=1 agents=53 polled=32).
  Boot QEMU 8GB + OVMF + janela completa sem reboot loop. âœ…
- **fix(mem): statics .bss corrompidas pelo bump heap** â€” `resize_bump_heap(2048)`
  entregava endereÃ§os alÃ©m do HEAP_BUFFER (512MB) â†’ sobrescrevia
  GLOBAL_ALLOCATOR/PHYS_MEM_OFFSET/TOTAL_RAM_MB â†’ `total_frames=0` â†’ falsa
  exaustÃ£o de frames ("sem frame CoW"). Fix: statics â†’ `.data` +
  HEAP_BUFFER â†’ seÃ§Ã£o `.bss.heap` no fim da imagem (limine.ld).
- **fix(neuralfs): nunca formatar disco com partiÃ§Ãµes** â€” `try_format_gpt_virgin`
  nÃ£o bloqueava 0xEE (protective GPT do ESP Limine) â†’ kernel formatava o
  uefi.img como NeuralFS â†’ OVMF "Not Found" no boot seguinte.
- **fix(boot/build.rs): rerun-if-changed** â€” sem isso uefi.img ficava stale
  (corrompido por boot anterior).
- **cargo check --release**: 0 erros

### SESSION_233: Ring3 Isolation (ADR-0077) â€” 6 fases (2026-07-30)
- **Phase 0 (fix)**: CR3 switch BEFORE iretq asm (Moros pattern). Moveu `mov cr3` do inline asm para `address_space::restore_cr3()` em Rust, eliminando triple-fault apÃ³s switch de page table.
- **Phase 1 (feature)**: `user_mode::run_process(pid)` â€” conecta ELF loader + ProcessManager + `enter_user_mode()`. Comando shell `run <pid>`.
- **Phase 2 (feature)**: TSS mutÃ¡vel via `TssCell` (wrapper Sync) + `set_rsp0()`. Per-process kernel stack para traps CPL=3â†’0.
- **Phase 3 (feature)**: Syscall ABI por registrador (RAX=nr, RDI=arg, RDX=caps) + fallback atomics. Handler lÃª registradores quando `stage_syscall()` nÃ£o foi chamado.
- **Phase 4 (feature)**: `address_space::create_sandbox_as()` â€” AS do zero que sÃ³ copia entries P4â‰¥256 (kernel+HHDM). Sem tabelas L3/L2/L1 compartilhadas com kernel.
- **Phase 5 (feature)**: Hypervisor-aware gating em `isolation_ring.rs`. `ring3_is_safe()` = true sÃ³ em KVM; TCG/WHPX/HW real = gated. `init_connectors()` registra native ring via `register_native_ring()` quando seguro.
- **fix: TssCell wrapper Sync** â€” substitui `UnsafeCell` por `TssCell(TaskStateSegment)` com `unsafe impl Sync` (single-threaded durante Ring3).
- **cargo check --release**: 0 erros

### SESSION_232: Bootloader 0.11 cleanup â€” Limine path Ãºnico (2026-07-30)
- **clean: vendor/bootloader/** â€” crate do image builder 0.11 removida (~1.8MB, 65+ arquivos)
- **clean: bootloader_api dep** â€” removida de k_nano, neural-kernel, jarbas Cargo.tomls
- **clean: limine-boot feature** â€” removida; Limine Ã© agora unconditional (sem feature gates)
- **clean: bootloader 0.11 entry point** â€” `kernel_main(boot_info)`, `BootloaderConfig`, `entry_point!` removidos
- **clean: BootloaderHandoff** â€” `neural-kernel/src/boot_handoff.rs` deletado (wrapper `bootloader_api::BootInfo`)
- **clean: probe_uefi_framebuffer** â€” `jarbas/src/display/fb.rs` â€” sÃ³ chamada no entry 0.11
- **clean: raw_boot_info()** â€” removido do trait `BootHandoff` em `k_nano/src/boot_handoff.rs`
- **clean: BitmapFrameAllocator::init()** â€” mÃ©todo morto (sÃ³ `init_from_usable_ranges` usado)
- **clean: ramdisk bootloader path** â€” cÃ³digo que destrinchava `bootloader_api::Optional` (nunca dispara no Limine)
- **clean: LEGACY/build-tools/mk_uefi/** + **build_usb_bios.py** â€” builders 0.11 deletados
- **clean: [patch.crates-io] bootloader** â€” patch morto removido do workspace Cargo.toml
- **cargo check --release**: 0 erros

### SESSION_231: HW Expert v4 + ADR-0082 â€” HardwareInfo Registry (2026-07-30)
- **ADR-0082** â€” criada e implementada: HardwareInfo Registry, 489 linhas, Anexo A pesquisa de mercado
- **feat: HardwareInfo struct** â€” `platform_probe.rs`: registro pÃºblico de HW unificado. `hw_info()` accessor. `avx2_ready()`, `avx512_ready()`.
- **feat: HW Expert v4 multi-head** â€” 5 heads (family, fw, agent, caps, next_action). 59.905 amostras de treino. 260KB. v5 .bitnet format.
- **feat: Rust v5 loader** â€” `cortex.rs`: `HwExpertV4Model`, `load_hwexpert_v5()`, `predict_hw_v4()`, `hwexpert_v4_predict()` API pÃºblica.
- **feat: build_card() integrado com ML** â€” `hw_capability.rs`: tenta HW Expert v4 â†’ tabela â†’ heurÃ­stica.
- **feat: Boot loading HWEXPRT4.BIN** â€” QEMU loader scan + FAT32 fallback.
- **feat: SGDB /hw/pci/** â€” `predict_all_pci()` escreve prediÃ§Ãµes do HW Expert v4 no SGDB por device PCI.
- **fix: xsave gate AVX2** â€” WHPX filtra CPUID xsave. `allow_avx2` agora sÃ³ depende de `isa.avx2 && isa.avx && !tcg`.
- **fix: find_child_byte16_sse runtime dispatch** â€” ART: `art_ok=false` com `art_len==n_art` por SSE2 mal compilado em soft-float. Agora usa `#[target_feature(enable = "sse2")]` + runtime check.
- **feat: Windows DriverStore extractor** â€” `tools/extract_wdm_hwids.py`: 478 HWIDs extraÃ­dos.
- **feat: Q-jump per-step logging** â€” `mod.rs`: cada passo Q1-Q7 loga PASS/FAIL individualmente.
- **feat: ART benchmark monitorado** â€” `bench.rs`: `art_len=` no output mostra quantas chaves realmente inseridas.
- **tools**: `train_hw_expert_v4.py` (multi-head training), `unify_hwids_v4.py` (59.905 amostras), `extract_wdm_hwids.py`
- **models**: `hw_expert_v4.bitnet` (260KB), `dataset.json` (59.905 amostras), `vocab.json`
- **docs**: ADR-0082 completa com anexo de mercado, mapa fornecedores/consumidores, ring isolation.
- **cargo check --release**: 0 erros

### SESSION_230: Boot acelerado â€” skip Ed25519 + VFS I/O para seed agents (2026-07-30)
- **perf: seed_agent()** â€” pula `sign_artifact_md()` (Ed25519, ~50-100ms/agent) e
  `read_vfs`+`write_vfs` (NeuralFS I/O) quando `tier == "native"`. Seeds sÃ£o
  trusted-by-compilation, nÃ£o precisam de assinatura runtime nem persistÃªncia VFS
  (jÃ¡ estÃ£o no binÃ¡rio). Economia: ~8.5s de boot (T+810â†’T+9386 â†’ T+810â†’~T+900).
- **ponytail comment** â€” marcado com `// ponytail: ...` no cÃ³digo.
- O fix estÃ¡ em `crates/hermes/src/package_hub.rs` `seed_agent()`.

### SESSION_229: Turing Test â€” JARBAS Plenitude + LLM 8 slots + BEI (2026-07-30)
- **feat: JARBAS Rung 4** â€” Ring3 (TRY_ENTER_RING3=true), TTF Latin-1 (Ã  Ã¡ Ã¢ Ã£ Ã© Ãª Ã­ Ã³ Ã´ Ãµ Ãº Ã§), alpha blending real
- **feat: Sprint 80** â€” fail-closed safety (ConsentGate deny por padrÃ£o), emotion classifier 16-feature
- **feat: Streaming TTS** â€” primeiro Ã¡udio em ~50ms via StreamingTtsState, PLAYBACK_RING streaming
- **feat: HWâ†’Persona** â€” 4 perfis (StandardUmaâ†’Tool, AsymmetricCcdâ†’Coach, IntelHybridâ†’Tutor, MultiDomainNumaâ†’Auto)
- **feat: AutoSkillGenâ†’AppFactory** â€” gera WASM real no 3Âº matching
- **feat: Matrix learning #311f** â€” OnDemandLearning + MatrixLearningAgent (454 LOC)
- **feat: 8 modelos no ModelHub** â€” BITNET2B, VISION, LLAMA8B, RERANKER, RUSTCDR3, HWEXPRT, LEARNER, AGENT
- **feat: dispatch_expert** â€” RUSTCODER_MODEL + HWEXPERT_MODEL + Agent slot roteados para modelos dedicados
- **feat: MoE router neural** â€” load_router() no boot substitui keyword matching
- **feat: Fine-tuning #312b** â€” FineTuningPipeline (DataCollectorâ†’TrainingAgentâ†’BitNetTrainer)
- **feat: Self-Learning OS #313** â€” SelfLearningAgent (PollEvery 5000, DataCollectorâ†’Hub)
- **feat: SleepCycle #314a-f** â€” 6/6 itens (EWC, ring buffer 1000, Dream sintÃ©tico, Pruning, Confidence, Ciclo)
- **feat: Structured Decoding #412** â€” OutputGrammar, mask_logits, generate_structured, 10 self-tests
- **feat: BudgetManagerâ†’scheduler** â€” watchdog Normalâ†’Warningâ†’Pausedâ†’Crashed
- **fix: Model loading honesty** â€” ModelStatus, NO_MODEL_MSG, CortexAgent nÃ£o cria toy model no boot
- **fix: wasmi unwrapâ†’Trap** â€” 6 call sites convertidos de panic para Trap seguro
- **refactor: LEGACY migrations** â€” hardware/, adaptation/, p2p/, brain_mesh, core_pair, budget, hooks, wasm* (7.845 LOC)
- **restore: LEGACY gems** â€” hardware topology, adaptation engine, MPMC queue, budget watchdog, dedup, HAL trait (3.500 LOC)
- **docs: ADR INDEX** â€” ADR-0057 completa, ADR-0075 completa(parcial)

### SESSION_228: Hardware Boot + SysInfo Debug Card + Mouse Fix (2026-07-28)
- **hardware boot**: pendrive unified (GPT/ESP) bootou Limine UEFI atÃ© Jarbas em notebook real
- **fix(compositor)**: `render_app_content` agora renderiza `WindowContent::Card` via
  `card::render_card()`. Cards existiam como cÃ³digo mas nunca apareciam na tela.
- **feat: SysInfoAgent** â€” agente `PollEvery(50)` que coleta CPU/cores, memÃ³ria/RAM/heap,
  agentes totais, uptime, rede e storage de fontes lock-free e exibe como card Jarbas.
  Card ID=9001, atualizado in-place a cada ~2.7s.
- **feat: status bar** â€” linha de status agora mostra mais info (implÃ­cito no SysInfoAgent)
- **fix(mouse): `ps2_check_exists()`** â€” detecta controlador 8042 antes de init PS/2
- **fix(boot): `mk_esp_fat.py` GPT** â€” migrado de MBR-only para GPT completo
- **Hardware boot em notebook real**: pendrive unified (GPT/ESP + dados FAT32) bootou
  Limine UEFI atÃ© interface Jarbas. Primeiro boot HW real da histÃ³ria do projeto.
- **fix(mouse): `ps2_check_exists()`** â€” detecta controlador 8042 antes de init PS/2.
  Em notebook moderno sem 8042, `enable_ps2_mouse()` fazia 100K-loop timeouts em
  cada operaÃ§Ã£o de porta 0x64/0x60, tornando o sistema lentÃ­ssimo e sem mouse.
  Agora: self-test 0xAAâ†’0x55 com timeout curto 5K loops. Fallback para USB HID.
- **fix(boot): `mk_esp_fat.py` GPT** â€” migrado de MBR-only para GPT completo
  (protective MBR 0xEE + EFI PART header + partition entries + backup GPT).
  Limine bootloader exige GPT/ESP padrÃ£o UEFI. `build_usb_unified.py` depende
  de GPT no `uefi.img` para criar pendrive bootÃ¡vel unificado (`usb_hw.img`).
- **Decks (diÃ¡rio de bordo)**: identificados challenges do bare-metal real:
  sem 8042, sem ATA PIO (USB boot), trackpad I2C-HID sem driver, xHCI depende
  de enumeraÃ§Ã£o bem-sucedida. SMP com retry 3Ã—250ms adequado.

### SESSION_227: ADR-0079 Neural AutoInstaller â€” M0 a M4 (2026-07-27)
- **ADR-0079 Neural AutoInstaller**: Instalador inteligente pendriveâ†’HD/SSD/NVMe com IA.
  Detecta HW alvo, copia sÃ³ o que a mÃ¡quina precisa, cria GPT dual (ESP+NeuralFS).
  Nenhum projeto AIOS no_std pesquisado (ClaudioOS, FYY, Wetware, WeftOS, Oreulius,
  WAeasi, coconutOS, ArceOS) tem self-installer â€” territÃ³rio inÃ©dito.
- **M0 â€” SysInstaller reativado**: `pub mod sys_installer` linkado na lib.rs.
  `install(target, kernel_elf)` aceita `&mut dyn BlockDevice` genÃ©rico em vez de ATA hardcoded.
  Cria GPT via `gpt_format_single()`, copia kernel.elf setor a setor, verifica MBR+GPT.
  demo() com MemoryDisk testa o ciclo completo.
- **M0.1 â€” gpt_format_multi()**: Cria GPT com N partiÃ§Ãµes (128 entradas, header+backup, CRC32C).
  `GptPartitionDef` struct para definir type_guid, start, end, label.
- **M1 â€” Dual partition + ESP copy**: `install(source, target, kernel_elf)` â€” cria GPT com
  ESP (FAT32, 512MB) + NeuralFS (restante). Copia ESP setor a setor do source (pendrive).
  Formata NeuralFS, copia kernel.elf via `NeuralVolume::create_file()+write_file()`.
  Verifica MBR + GPT header + FAT32 BPB.
- **M2 â€” AutoInstallerAgent + HwProfiler + Jarbas card**: `HwProfile` com PCI scan + RAM detect +
  GPU/NIC/WiFi flags. `AutoInstallerAgent` EventDriven que orquestra instalaÃ§Ã£o completa,
  copia catÃ¡logo de skills `/skills/CATALOG.MD` e perfil HW `/config/hw_profile.txt` para o target.
  `install_progress_card()` no Jarbas com gauge + step + botÃ£o Reboot.
  Hermes shell comando `install` exibe perfil HW.
- **M3 â€” AI-Native installation**: `Cortex::install_adviser` gera recomendaÃ§Ã£o via ModelHub slots
  (GeneratorProâ†’Activeâ†’fallback). `self_check` salva/verifica CRC32C dos arquivos instalados.
  `rollback` com 3 tentativas + fallback pendrive.
- **M4 â€” HW Swap + Recovery**: `hw_change` detecta troca de GPU/NIC/WiFi comparando perfil salvo.
  `self_heal_disk` escaneia StorageBus, escolhe maior disco alternativo, propÃµe migraÃ§Ã£o.
  `net_fallback` busca firmware ausente via NetFsâ†’GitHubâ†’HuggingFace aios-k2chj.
- **detect_ram_mb() real**: `TOTAL_RAM_MB` atomic populado pelo frame allocator no boot.
  Substitui hardcoded 512MB.
- **format_fat32_esp()**: Cria FAT32 vÃ¡lido do zero (BPB, FSInfo, FATs, root dir).
  PartiÃ§Ã£o â‰¥ 65525 clusters (~32MB mÃ­nimo). Sem depender de source ESP.
- **Ajuste hub multi-LLM**: InstallAdviser roteia via `model_hub::generate_from_slot()`.
  `N_SLOTS=8` para comportar `ModelSlot::Agent`.
- **LiÃ§Ãµes:** `scan_pci()` Ã© unsafe (precisa `unsafe {}`). `PciDevice` tem `bar0..bar5` individuais,
  nÃ£o `bars[]`. `NeuralVolume::write_file()` precisa de `dev + ino + data`. `list_skills()` retorna
  `Vec<(String, ToolPolicy)>`, nÃ£o String. `StorageBus.entries()` devolve `&[StorageEntry]` com
  campos nomeados, nÃ£o tuplas. Sempre verificar `N_SLOTS` ao adicionar `ModelSlot`.
- **cargo check --release: 0 erros**.

### SESSION_226: Onyx ChatWindow + StreamPacket Protocol + Render Registry + COSMIC UI (2026-07-27)
- **StreamPacket protocol** (`hermes/src/stream_packet.rs`): 14 typed packet types (ReasoningStart/Delta/Done, ToolStart/Delta/Done, MessageStart/Delta, Stop, etc.) com encode/decode compacto para EventBus.
- **ChatSession tree** (`hermes/src/chat_tree.rs`): Ãrvore de conversa com branching (parent/children), ChatNode, display_nodes().
- **ChatWindow** (`jarbas/src/display/chat_window.rs`): UI Onyx-style com timeline de tools expansÃ­vel/colapsÃ¡vel, mensagens streaming, histÃ³rico, input bar, botÃ£o mic toggle `[MIC]`/`[REC]`.
- **Ãudio integrado**: `MIC_ACTIVE` flag â†’ VoiceAgent escuta sem wake word â†’ STT transcreve â†’ texto no input buffer. TTS automÃ¡tico na resposta.
- **FocusMode** (`compositor.rs`): Chat (clique no chat â†’ teclado vai pro input) vs Ambient (fundo â†’ wake-word "Jarvis").
- **COSMIC visual refinements**: `decorations.rs` com `draw_rounded_rect(r=4)`, gaps entre tiles (4px), painel Hermes translÃºcido (bg_alt/2 + r=8), barra de status estilo COSMIC.
- **Render Registry** (`jarbas/src/display/render_registry.rs`): `RENDER_REGISTER` / `RENDER_WINDOW` topics. Agentes registram `RenderFn` e publicam janelas dinÃ¢micas sem modificar compositor.
- **Cleanup**: NeuralConsole removido (~287 LOC), F-keys legados, Settings/Power/Ide/Camera/AudioViz AppIds descartados. render_app_content sÃ³ trata HermesChat.
- **cargo check --release: 0 erros**.

### SESSION_225: Limine Migration + Higher-Half Fixes + Desktop Jarbas na Tela + Soft Power Off (2026-07-27)
- **Limine boot (uefi.img):** MigraÃ§Ã£o bootloader 0.11 â†’ Limine 6.x. Kernel higher-half 0xffffffff80000000+. Framebuffer @0xffff8000c0000000 via HHDM.
- **PHYS_MEM_OFFSET early store:** main.rs:1268 â€” setado ANTES de qualquer driver. e1000/HDA/NetAgent enxergam offset correto em vez de 0.
- **P6 raw_vec capacity overflow fix:** TRY_ENTER_RING3=false. SubtraÃ§Ã£o entre VA higher-half e user-space (0x7000..) estourava isize::MAX â†’ Vec::with_capacity overflow.
- **e1000 RX #PF loop fix:** ponytail guard `if pmoff == 0 { return None }` em recv()/any_rx_dd(). Buffer overflow corrompe o static PHYS_MEM_OFFSET.
- **BPE scan bound fix:** cortex/src/bpe.rs:485 â€” 0x200000000â†’0x180000000 (RAM 6GB, nÃ£o 8GB).
- **Desktop Jarbas na tela do QEMU:** Compositor, 3 apps (HermesChat + Settings + Power), 55 agentes, scheduler rodando.
- **Soft power off OK:** BotÃ£o Power â†’ confirmaÃ§Ã£o â†’ ACPI PM1a_CNT (0xb004) â†’ shutdown.
- **Cleanup:** build_esp.ps1 removido (build.rs gera ESP). limine.cfg removido. .gitignore em tools/limine/esp/.
- **WHPX:** "Ignoring request for interrupt vector 0" â€” pendente investigaÃ§Ã£o ACIP/IDT.

### SESSION_224: ADR-0076 ImplementaÃ§Ã£o Pesada â€” 23 entregas (2026-07-27)
- **Skill Manifest FYY canÃ´nico (Onda 1):** struct expandida com RemoteConfig, Pricing, QualityIndicators, Interop (a2a/clawhub/skillnet), parser from_slice/from_json_str, 12 testes. 25 manifests agentes nativos A-001 a A-025.
- **WASM Runtime expansion (Onda 2):** host functions 1â†’6 (aios, aios_net, aios_fs), 11 cap constants + check_cap() com cap check. WASI Preview 1 stubs conectados ao linker. WAT test suite com 18 testes. Telemetry ring lock-free SPSC 4096 slots + shell trace cmd.
- **SeguranÃ§a (Onda 3):** Membrane two-layer gate (bitmask + Membrane::check). Permission Gate com HITL (RiskLevel, Approve/Deny spin-wait). Quarantine Gate sanitization (pattern/length/repetition/structural + 8 testes). WIT-typed ABI (aios.wit).
- **Live capsule lifecycle (Onda 3.6):** PKG_CHANGED events no EventBus para upgrade sem reboot.
- **Cascading capability revoke (Onda 4.2):** CapRegistry com create/delegate/revoke em cascata.
- **Goal-aware scheduler (Onda 4.3):** goal_urgency + novelty_score + coherence_partner. Sort por goal_urgencyÃ—2 + novelty_score. Rate-limiting exclui agentes com urgÃªncia >0. Novelty decay 1/tick.
- **Intent Bus canÃ´nico (Onda 4.4):** Intent enum com 33 variantes, 10 categorias, describe().
- **Glass Box inspect (Onda 4.5):** inspect command mostra estado vivo dos 25 agentes.
- **Syscalls consolidados 13â†’9 (Onda 4.4.1-3):** removeu SEND_TCP + VRING_SETUP, unificou WRITE_RING+READ_RINGâ†’RING_OP. 6 arquivos atualizados.
- **GEMM benchmark golden checksum (Onda 8.1):** ternÃ¡rio 64Ã—64 FNV-1a (Folkering pattern).
- **SYS_MAP_FB real:** page table walk no syscall dispatch, mapeia BAR fÃ­sicas no AS atual.
- **Proof-gated mutations (Item 3):** ruvix-proof crate integrado (ProofGate, 3-tier proof, 6 testes).
- **Kernel HNSW (Item 4):** ruvix-vecgraph crate integrado (KernelHnsw, HNSW slab-allocated, patches no_std).
- **Ring-3 Userspace (Item 1):** ELF loader (elf_loader.rs), ProcessManager (process.rs), SYS_DEMAND_PAGE real, TRY_ENTER_RING3=true, shell `run` cmd.
- **LiÃ§Ãµes:** Fixers paralelos sobrescrevem lib.rs â€” verificar mÃ³dulos apÃ³s cada fork. ruvix-vecgraph precisa de patches no_std (f32::sqrt). Merge conflicts em rust-toolchain.toml e wasm_build.rs. `AgentTickResult::Continue` nÃ£o existe â€” usar Pending.

### SESSION_223: Cross-OS Ecosystem + BEI + P01 Drift + TLS + ADR-0040 (2026-07-26)
- **ADR-0076 Cross-OS Ecosystem (7 fases):** Skill Manifest (skill_manifest.rs: RiskLevel, SkillType, Permissions, SkillManifest, validate, to_json, office_spreadsheet factory). Membrane + CapGate (membrane.rs: Membrane struct, Operation, Capability, Verdict Allow/Deny/Escalate, for_legacy/for_wasm, glob FS, net allowlist, demo self-test). JAIL sandbox (jail.rs: Jail struct, Membrane::check, Merkle audit trail, check_file_read/write/net/capability, report, demo). WASI Preview 2 (wasi_host.rs: 15 wasi_snapshot_preview1 stubs â€” fd_write, fd_close, fd_seek, fd_prestat_get, environ_*, args_*, proc_exit, random_get, clock_time_get, path_open). MCP bridge (mcp_client.rs: search_marketplace/search_fyy/search_weftos â†’ mcp_server.rs: SearchFyy/WeftOS/Skills MCP methods + search_skills/search_fyy_skills/search_weftos_skills). Ciclo aprendizado (cross_os/agent.rs: LearningState Learnâ†’Proposeâ†’Auto, WorkflowLearner pattern_registered integration).
- **ADR-0060 BEI (7 ondas completas):** Onda 3 Dynamic MoE (cortex::moe: try_birth/try_merge/try_split + stale_indices/high_entropy_indices + self_test). Onda 4 Memory L0-L7 (hermes::memory: MemoryLevel 8 tiers, MemoryTier trait, InMemoryTier, MemoryStore read/write/promote/tick_advance). Onda 7 Soul Mirror (jarbas::display: SoulMirrorState/SoulMirrorRenderer, Avatar8State 8 estados Idle/Listening/Processing/Speaking/Thinking/Dreaming/Alert/Updating). Lifecycle INDEX.md atualizado Accepted/completa.
- **ADR-0040 Residuals:** SysInstaller #421 (k_nano::sys_installer: scan_disks, install ATA copy, verify, EventBus SYS_INSTALL publish). Storage UI #419 (jarbas::cards::storage_card: gauge, disk list, format button via UiDeclaration/UiRenderer).
- **TLS:** embedded-tls integration (hermes::tls: TlsStatus, https_get/https_get_fallback bridges, feature-gated `tls`). Cargo.toml feature + kernel bridge registration.
- **P01 Type-drift fix:** NIC globals unificados (k_nano::nic_globals â†’ pub use no bin). BSP_PCPU unificado (BspPcpu wrapper). wasmi Error type fix (wasmi::core::Trap â†’ wasmi::Error).
- **P08 SELF_HEAL/TRUST_CACHE:** Movidos para k_ai como singletons. Bin agora pub use.
- **Drift massivo (14 mÃ³dulos):** boot_logger (FAT persistence binâ†’k_nano), serial (pub use), allocator (LazyBumpAllocator binâ†’k_nano), vfs (append/exists/mkdir binâ†’k_nano), smp/trampoline (pub use), smp/work_stealing (pub use), fs/ata_agent (pub use), disk_power (pub use), usb_trust (pub use), NeuralFS (9 arquivos idÃªnticos deletados do bin), hnsw/multi_user (cÃ³pias em k_nano + cortex/k_ai).
- **RTC driver:** k_nano::rtc (CMOS MC146818: cmos_read, bcd_to_bin, read_rtc, RtcDateTime, format_rtc, demo).
- **BGE alignment:** static mut BGE_WEIGHTSâ†’spin::Mutex, BGE_VOCAB/HIDDENâ†’AtomicUsize, f32 alignment chunks_exact(4)+from_le_bytes safe copy.
- **HwRegistry detect_all:** PCI class_name() helper + slog_kai! log por device no boot.
- **restore_checkpoint:** save_count field, best-effort doc table, v3 serialization format. Boot log FAT32 validation log.
- **Ring 1 ownership:** safety/security/optimizer/SleepCycle/AutoLearn documentados como permanÃªncia em hermes.
- **Toys no-op:** CandleSidecar/TaskSpawner/ReActLoop ponytail comments (k_ai::cognitive).
- **Trust validation:** check_or_cache wired em 3 execute_skill diretos + audit 17/17 paths.
- **debug_rl! deprecation** em favor de slog_bin!.
- **TECNOLOGIAS.md:** 4 novas entradas (RTC, SysInstaller, BEI, Cross-OS).
- **59+ arquivos modificados, 8 deletados, 5 novos.** cargo check --release = 0 erros.

### Power Management completo â€” P-state, C-state, S3 Suspend/Resume (2026-07-26) â€” SESSION_222
- **cpufreq.rs (novo):** MSR IA32_PERF_CTL (0x199) + IA32_PERF_STATUS (0x198) + IA32_ENERGY_PERF_BIAS (0x1B0). Governor Performance/Powersave/Ondemand. CPUID leaf 0x16 + probe MSR write-take-effect. APERF/MPERF actual_ratio() via MSR 0xE8/0xE7 para frequÃªncia real.
- **MWAIT real:** AP idle loop usa `monitor`/`mwait` quando CPU suporta (CPUID.1:ECX[3]), fallback `hlt`. MONITOR_FLAG (AtomicU8, cache-line aligned) escrito no enqueue() para wake sem IPI. `set_mwait_hint(cstate)` para C1â€“C6.
- **S3 suspend:** ACPI _S3 DSDT parser + FACS waking vector. `suspend()` salva CR3/RSP, set FACS wake vector â†’ trampoline 64-bit em 0x7000, park APs, set powersave, write SLP_TYP=3+SLP_EN via PM1a_CNT.
- **S3 resume trampoline:** Blob de 64 bytes na posiÃ§Ã£o fÃ­sica 0x7000: restaura CR3 + RSP, jump para `s3_resume_entry()`. Handler re-inicializa APIC, PIT, EPB. Save/restore e1000 (16 regs + MTA 128 entradas).
- **Scheduler integration:** Ondemand tick no closure `halt` do `registry.run()` â€” chama `cpufreq::ondemand_tick(ap_work::has_pending())`.
- **10 arquivos modificados:** cpufreq.rs (novo), suspend_resume.rs (novo), acpi.rs (+_S3 parse +FACS), ap_work.rs (+MWAIT), platform_probe.rs (+mwait), apic.rs (+send_ipi_reschedule_to), e1000.rs (regs pub), core_pair.rs (send_wake_ipi real), lib.rs (+2 mod), hardware/probe.rs (+cpufreq init). 0 erros.
- Refs: wasmi, AAGT, GBNF/Outlines/XGrammar, arXiv SelfEvolve/ARISE/Tool-Making/MCP-SandboxScan

### Generative Card Desktop (UI/Desktop Jarbas) â€” ADR-0058 (2026-07-21) â€” S1â€“S4 âœ…
- Planejamento unificado do UI/desktop: fundaÃ§Ã£o **embedded-graphics** (`DrawTarget` sobre `DoubleBuffer`) + toolkit no_std (matrix-gui/embedded-gui/kolibri, MIT/Apache) + camada declarativa **`UiDeclaration`/`UiRenderer`** (cards)
- Cards gerados como **dados** por Hermes/Trinity/Cortex (constrangidos pelo structured decoding ADR-0057 #412) ou por **skill WASM** (RustCoder/Codex, ADR-0052) + repetiÃ§Ã£o Cron. Ex.: "clima de amanhÃ£" â†’ WeatherCard
- WM stacking mantido (Ã¡rvore de janelas retida; aposenta enum `AppId` hardcoded)
- **Supersede parcial** ADR-0047-HMI (H1/H2/H4/H5 absorvidos; H3 âŒ); ADR-0036 persona inalterada
- **S1â€“S4 âœ… implementados** (QEMU: 3 cards + orb responsivo + barra de relÃ³gios/HUD preservados; self-tests S1/S2 PASS; clique fecha card; `cargo check` 0 erros). S5 (widgets ricos/tema/TTF) + A/V real (mic/alto-falante/vÃ­deo via HDA/UVC) = residual. Cards demo: Sistema, Clima ("clima de amanhÃ£"), Chamada de VÃ­deo (Atender/Microfone/Alto-falante/Encerrar)

### Compute Dispatch SMP+GPU+NPU â€” ADR-0057 (2026-07-20)
- **WS-A wake multi-AP:** SIPI direcionado sequencial por LAPIC ID + stack/PerCpu por-AP + retry INIT-SIPI-SIPI 3x. QEMU `-smp 4` â†’ **APs acordados: 3**, `CorePools r0=1 r1=2 r2=1` (antes: mÃ¡x 1 AP; â‰¥2 â†’ 0). Contador `AP_ENTRY_COUNTER` unificado; `neural-kernel::smp` emagrecido (delega a `k_nano::smp`)
- **WS-B:** `parallel_ternary_matmul` (particiona colunas; decode `m=1` escala) + `Tensor::matmul` f32 nos APs â€” **gated por `ap_pollable`** (deadlock-proof: BSP faz o matmul enquanto APs em `hlt`)
- **WS-C:** `cortex::compute` â€” dispatcher Ãºnico (`NPUâ†’GPUâ†’CPU-SMPâ†’AVX2â†’scalar`) nos choke points; backends via fn-pointer
- **WS-D:** `k_hal::gpu::compute_dispatch` registra GPU sÃ³ se `BackendState::Ready` (canÃ¡rio silÃ­cio); kernel W2A8 = Layer S/HW
- **WS-E:** `k_hal::npu` â€” detecÃ§Ã£o PCI XDNA/Intel + `[NPU-HW] VERDICT=SOFTWARE` honesto + fallback software (Ring0 MLP CPU). Driver/firmware = Layer S/sponsor
- **WS-F:** wake robusto (retry) + `hlt` idle + gate `ap_pollable` + seam `install_wake_fn`/`wake_aps`. On-demand AP-worker (IDT+reschedule-IPI) = residual HW
- **WS-G #412:** `cortex::decode` structured decoding (mÃ¡scara de tokens antes do argmax); default no-op; self-test de boot **PASS**. Medusa/FlashAttention/PagedAttention/huge-pages/burn-flex/codebook = residual (validaÃ§Ã£o com modelo)

### Rebrand KÂ³CHJ (2026-07-18)
- Nome canÃ´nico **KÂ³CHJ** = `k_nano` + `k_hal` + `k_ai` + Cortex + Hermes + Jarbas
- HistÃ³rico **KÂ²CHJ** = 5 crates (sem `k_hal` na marca); paths ADR `*k2chj*` inalterados
- GlossÃ¡rio: ADR-0042 Â§0; INDEX â€œNome do produtoâ€

## [1.9.5] â€” 2026-07-19 â€” Emagrecer neural-kernel cutover (TEST)

**VersÃ£o:** v1.9.5 TEST / NÃƒO ESTÃVEL â€” **nÃ£o** v2.0.0.

### Emagrecer neural-kernel (SESSION_163 / IDEA #467)
- Cutover seguro binâ†’crates KÂ³CHJ (ondas 0â€“6): stubs `pub use` + promote truth do bin
- Gate: `tools/diff_bin_crate.py` + `docs/memory/BIN_CRATE_DIFF.md`
- Unificados: `ATA_DRIVER`, `TIMER_TICKS`/`MOUSE_ABS_*`, `global_arena` pending_route
- Promovidos a k_nano: `fat32`/`ata`/`e1000` (probe exFAT, prove_rx)
- Residuals honestos no bin: `cortex.rs`/`bpe`/`agents`/`net*`/`audio/*`/`boot_logger`/`smp`
- `cargo nk` = 0 erros; ~âˆ’7k LOC no monÃ³lito (sem perda de lÃ³gica)

## [1.9.1] â€” 2026-07-19 â€” BitNet 850 generate + BPE SP32 + TLS/WiFi/LEGO (TEST)

**VersÃ£o:** v1.9.1 TEST / NÃƒO ESTÃVEL â€” **nÃ£o** v2.0.0.

### BitNet ladder 850 (SESSION_162)
- **#PF fix:** AVX2 ternary matmul â€” desactivar bitwise OOB (`n%4`); cauda scalar `n%8` (`bitnet_avx2.rs`)
- **Loader:** size = blob chat FAT; hub skip PIO se Active QEMU-loader; copy+`Box::leak`
- **Layout v4:** `has_basic_rms=true` (evita rms=0)
- **BPE SP32:** `export_bpe_bin.py --sp32` â†’ BPB1+**MRG1** (61249 merges); load **antes** LLM-TEST
- **LLM-TEST:** `ola` â†’ encode HF `[1,288,433]`; resposta BPE (coerÃªncia semÃ¢ntica residual)
- **Harness:** `tools/llm_ladder_bench.py`; FAT default **3072 MB**; `PACK_LLM=850|13|2b|3b|all`

### TLS / WiFi / Device LEGO (SESSION_154â€“161)
- TLS #123: embedded-tls soft-float; https_get; smoke google; PKI pins+TOFU
- WiFi ath10k QCA6174 A0â€“A3 BMIâ†’fw_ready (Note AWAITING)
- ADR-0056 DeviceRecipe / UnlockDAG H1 + specs `docs/specs/device-lego/`

## [1.9.0] â€” 2026-07-18 â€” PÃ³s-LAN B-01 + Residuals 0â€“7 (TEST)

**VersÃ£o:** v1.9.0 TEST / NÃƒO ESTÃVEL â€” base v1.8.6; **nÃ£o** v2.0.0.

### Plano Residuals 0â€“7 FECHADO (SESSION_142â€“151)
- **PreFlight:** `tools/preflight_wave.py` + cache `.preflight_cache/` + `pass_marker` anti-contaminaÃ§Ã£o + anti-fake Ready
- **Ondas 0â€“6:** docs/IDEA; NeuralFS smokes; exFAT write `#417`; USB Trust/UAC-HW; GPU/MHI AWAITING; AirLLM ATA + AIRLLM-DMA; soft-float defer (Trilha R)
- **Onda 7 LAN:** e1000 TX canÃ´nico `0x3800/0x3818` (aliases QEMU no-op); L3.5 ARP/RX; DNS raw + HTTP 301 smoke WHPX
- **Tags:** `depends_on: lan` liberado; WiFi AWAITING; TLS BLOCKED; #418 peer PASS
- **PolÃ­tica:** `â–¶ï¸ AWAITING_HW` â€” sem fake Ready

### PÃ³s-LAN B-01 unlock (SESSION_152)
- **net_bridge** Hermesâ†’bin NETSTACK; `resolve_and_http_get` + Host header; HTTPS deny atÃ© TLS
- **Agents:** `/fetch`, Browser, Search, RSS, Market, AutoLearn sem stub B-01; Email SMTP residual honesto
- **AirLLM Net:** DNS+hostname + Range/stream; `tools/serve_tiny_gguf.py`
- **#418 NetFs:** TCP `gateway:4446` + `tools/netfs_peer.py` + `[NETFS] VERDICT=PASS`
- **#308 SelfUpdate:** `fetch_update` HTTP + FNV + slot A/B
- **#123 TLS:** `[TLS] VERDICT=BLOCKED reason=softfloat_or_crate` (sem fake HTTPS)
- **Fix:** deadlock NETSTACK â€” NetFs smoke fora de `NETSTACK.lock()` pÃ³s-L5
- **Hygiene:** TODO BLOQUEADORES / STATE / IDEA / INDEX alinhados SESSION_152

## [1.8.6] â€” 2026-07-18 â€” ADR-0041 H4+/H5+/AS + HalOffer Cap (TEST)

### ADR-0041 restante (SESSION_140)
- **H4+ QUEUE_NOTIFY:** `k_hal::virtio` map UC VirtIO-PCI + `try_pci_queue_notify` â†’ `NotifySent` / `NotifySkipped` honesto
- **Residual MMIO:** hermes/jarbas FE (HalOffer); VGACNTRL â†’ `k_hal::gpu::backend::disable_intel_vga_plane`; `virtio_gpu` / `link_watcher` sem BAR
- **H5+ Cap:** `grant_fe` no `offer::bind`; ports `fe_*` + `check_fe_bound`; demo R3 Deny / Bound Allow
- **AS shallow:** `address_space::demo_as_r1_r3_shallow` (CR3 + touch BAR + restore; â‰  isolamento produÃ§Ã£o)
- **HalOffer:** API R3 genÃ©rica; VirtIO = transporte BE; slog canÃ´nico `[T+n] [Rn] [k-xxx]`
- **VersÃ£o:** v1.8.6 TEST â€” **nÃ£o** v2.0.0; ADR lifecycle `fazendo`

### HW USB boot diagnostics (SESSION_139)
- **Console FB legÃ­vel:** `console_clear` / `console_print` em `jarbas/display/fb.rs`; `boot_ckpt`/`boot_splash` e `vga_buffer::fb_print` usam o mesmo cursor (limpa faixa â€” sem TRACE/ghost).
- **BOOT.LOG:** `fat-boot-log` no artifact boot; overwrite 8.3 + `heap_ready`; `init_after_usb` + ckpts K0â€“K17.
- **USB Windows mount:** MBR dados FAT32 `0x0C` + ESP `0xEF`; `mkfat32` BPB/seed; `inspect_usb_layout.py`.
- **Bootloader BltOnly:** `[patch.crates-io]` â†’ `vendor/bootloader` (SetMode Rgb/Bgr; sem panic em HD 620).

### ADR-0048 / 0049 / 0050 â€” GPU Multivendor Unlock (SESSION_138)
- FundaÃ§Ã£o: `compute_abi`, detect `has_compute=false` atÃ© canÃ¡rio, `display_coex` dirige `init_backend_with_plan`
- KernelPack NKP1 (`kernel_pack.rs`) + packers host NVIDIA/AMD/Intel + `tools/gpu_kernels` (CPU golden)
- Bring-up stubs: LegacyAcr vs Gsp; Gen9 vs Arc; AMD KiQ/Mes; canÃ¡rio FailDispatch â†’ CPU (display intacto)
- Hardening: ACR sÃ³ Pascal + BAR2+pmoff; AMD doorbell noop; gate ADR-0047 sÃ³ `Ready`
- **NÃ£o alegado:** QMD/walker/PM4 golden em silicon; NKP ainda unsigned (sig zeros)

### ADR-0051 / Agency data-driven (SESSION_134)
- **255 AGENT.md** em `ecosystem/agents/` (214 Agency + 41 nativos) via `tools/export_agent_packages.py`
- Seed embutido `k_ai::{agency_seed,native_agent_seed}`; `Agency::from_specs` + registro via PackageHub
- Kinds `Agent` + `Workflow`; alias legado `/agents/*.wasm`
- VFS bridge Hermes â†’ `neural-kernel::fs`; `NeuralFsAgent` cria Ã¡rvore `ecosystem/`
- Disco exFAT **nÃ£o** recebe nested ecosystem (residual honesto)

### NeuralFS / storage (SESSION_133)
- USB format lock (opt-in: `NEURALFS_USB_FORMAT=1` / `debug_assertions` / `allow_usb_format`)
- GPT dedicada NeuralFS (`GPT_TYPE_NEURALFS` + virgin `gpt_format_single`)
- `build_usb_unified.py`: partiÃ§Ã£o de dados exFAT default (`--fat32` legado)
- `mkexfat.py`: boot Microsoft checksum + backup; bitmap/upcase; root 0x81/0x82/0x83

### NeuralFS / storage (SESSION_132)
- B-tree multi-nivel (split interno + path CoW); USB-MSC format/mount `0x7F` para pendrive de teste
- Volume de dados de boot: `mkexfat.py` default; `read_file_from_dev` prefere exFAT (fallback FAT32)
- Fix exFAT `VolumeLength` @ offset 72 (spec Microsoft)

### Fixed
- **hermes `wasm_rt::SkillMarket::top`:** replace `partial_cmp(...).unwrap()` ranking with `f32::total_cmp` (total order, NaN-safe), aligned with `skill_market::SkillMarket::top_skills`. Truth path is hermes only (monolith `wasm_rt` mirror removed at N4.6); LEGACY snapshot untouched.
- **Release build warning cleanup:** remove three unused imports, two unreachable match arms and the informational `cargo:warning` emitted by `boot/build.rs`; clean `cargo check --release` now completes with 0 errors and 0 warnings.
- **Framebuffer bpp dinÃ¢mico:** `GpuDevice::from_probe` / `resolve_bytes_per_pixel` usam `info.bytes_per_pixel` do GOP como fonte Ãºnica; `DoubleBuffer::from_gpu` e consumidores (DisplayAgent, splash, console, avatar, P4) atuam sobre o valor coletado â€” sem hardcode Bgrâ†’3.
- **HW PnP / HwCapabilityCard:** remove free-text `generate_via_hwexpert("identifiqueâ€¦")` (lixo `OA5USâ€¦`); card tipado `k_ai::hw_capability` (family/fw/agent/caps/next_action) publicado em `HW_CAPABILITY` + `HW_PNP_ACTION`; hooks honestos (NEED_FWâ†’HEALTH_ISSUE, wifiâ†’NET_IFACE_AVAILABLE). Seed treino `tools/train_hw_expert_v4.py` no mesmo schema.
- **Hermes agentico PnP:** `hermes::hw_pnp` â€” card â†’ `observe_intent` + skill efÃªmera (SkillOpt) â†’ com â‰¥3 usos/70% promove WASM via `evolve::promote_ephemeral_to_wasm`; Cortex sÃ³ em `bind_wifi_scan`/`bind_gpu_compute` (hint â‰  ordem). Detect deixa de dump free-text da Ã¡rvore em `LLM_REQUEST`.
- **ADR-0051 Package Hub:** namespace NeuralFS Â§12 `/mnt/neural/ecosystem/{skills,agents,plugins,mcp,models,firmware}`; `package_hub` CRUD+HITL+assinatura embutida; `/pkg *`; catalog no system prompt Cortex; seed `skills/*/SKILL.md`.

## [1.8.5] â€” 2026-07-16 â€” ConsolidaÃ§Ã£o pÃ³s-v1.8.0 (teste / nÃ£o estÃ¡vel)

> Canal de integraÃ§Ã£o e testes. Os MVPs abaixo nÃ£o constituem validaÃ§Ã£o
> production-grade nem liberam o gate de `v2.0.0`.

### Agentes e voz
- **Sprint 108:** Self-Evolve `observeâ†’generateâ†’verifyâ†’improveâ†’reflect`, verificaÃ§Ã£o de skills, SIL e reflexÃ£o no SleepCycle
- **Sprint Sound:** pipeline Micâ†’Wakeâ†’STTâ†’LLMâ†’TTS, STT PCMâ†’MFCC, UAC descriptor parse, VAD/SER e Piper neural-lite
- Residuais honestos: soft-float/VITS, CTC WER, UAC isÃ³crono e cutover `jarbas::audio`

### Filesystems e modelos
- **NeuralFS:** I/O RAM, B-tree leaf com reclaim/split, ATA MBR opcional e agente `/mnt/neural`
- **ADR-0040:** exFAT read-MVP + MHI soft-migrate; writes e DMA fÃ­sico permanecem `por_fazer`
- **ADR-0046:** AirLLM GGUF layer-wise, prefetch soft e hot-swap ATA/Netâ†’FATâ†’`set_model`
- **Cortex:** N-gram speculative decoding com benchmark empÃ­rico e rollback de KV

### Latent/GPU/HMI
- **ADR-0047:** LatentBus, Evolve hot-swap/Genesis, NeuOS Probe, GPU work queue/SASOS/H2O/G5 e HMI embedding/splats
- **ADRs 0048â€“0050:** propostas multigeraÃ§Ã£o NVIDIA/AMD/Intel registradas como `por_fazer`

### Estado
- VersÃ£o **v1.8.5 de teste, nÃ£o estÃ¡vel**
- `v2.0.0` continua bloqueada por review formal, demandas `por_fazer` e aprovaÃ§Ã£o explÃ­cita do maintainer
- MemÃ³ria consolidada em `docs/memory/SESSION_121.md`â€“`SESSION_129.md`

## [1.8.0] â€” 2026-07-16 â€” Marco KÂ³CHJ: ADR-0042 adequaÃ§Ã£o + wire crates completo

### Marco
- **ADR-0042 N1â€“N5 âœ…** â€” cadeia funcional `k-nano â†’ k-ai â†’ cortex â†’ hermes â†’ jarbas` verificada em QEMU
- **Wire crates N2.5â†’N5.7 âœ…** â€” monÃ³lito boot linka os 5 crates KÂ³CHJ (commits `8740bfd`â€¦`95f8967`)
- **Sprint 107 Voice âœ…** â€” PASS parcial forte+ (`'O tempo esta'` + Piper neural-lite + EventBus skinny)
- **Pista ativa pÃ³s-1.8.0:** Sprint Sound (voz production-grade) + review gate `v2.0.0` (nÃ£o declarar automaticamente)

### Wire summary (N2.5â†’N5.7)
| Fase | VersÃ£o | Crate | Espelhos removidos |
|------|--------|-------|-------------------|
| N2.5 | v1.7.8 | `k_ai` + `k_nano` | `trust.rs`, `self_heal.rs` |
| N3.5 | v1.7.9 | `cortex` | 9 (tensor, trinity, arena, r3, â€¦) |
| N4.6 | v1.7.10 | `hermes` | ~37 (cron, wasm*, wifi*, apps/, â€¦) |
| N5.7 | v1.7.11 | `jarbas` | 29 (display/*, gpu/*, jarvis, â€¦) |

### PadrÃ£o de migraÃ§Ã£o (liÃ§Ã£o)
- Alias dep `*-crate` evita conflito com `mod` re-exportado
- `k_nano` sem feature `global-alloc` â†’ Ãºnico `#[global_allocator]` no bin
- Bridge `memory` + `EVENT_BUS` â†’ `k_nano::globals`
- Residual monÃ³lito = integraÃ§Ã£o bin-only (`cortex.rs`, `audio/*`, `agents.rs`, `net*`, `fs/*`, `jarbas_fb.rs`)

### Residual (nÃ£o bloqueia 1.8.0)
- `audio/*` â€” ADR-0045 truth path (Sprint Sound)
- `cortex.rs` / `bpe.rs` â€” generate path + weather-e2e loader
- `agents.rs` / `net*` / `fs/*` â€” fleet + NETSTACK singleton
- Qualidade voz: STT CTC fraco, Micâ†’Wake runtime, Piper VITS pleno, soft-float latency

### HW real
- `target/usb_hw.img` unificado (ESP + FAT dados; Rufus DD)
- BITNET2B + PIPER + HWEXPRT + RUSTCDR + BGE + STT + 116 firmware blobs

### Build
- `cargo clean -p neural-kernel && cargo nk` â€” **0 erros** (2026-07-16)

### Docs
- STATE v1.8.0, SESSION_120, SESSION_INDEX, TODO, AGENTS, ADR-0042 policy, IDEA_BANK #439â€“#442

## [1.7.11] â€” 2026-07-16 â€” ADR-0042 N5.7 (jarbas wired no bin)

### Wired
- **N5.7** `neural-kernel` â†’ `jarbas-crate` (dep direta `package = "jarbas"`)
- `pub use jarbas_crate::{display, gpu, jarvis, virtio_gpu, uvc_driver, vision_agent}`
- Removidos 29 espelhos monÃ³lito (display/*, gpu/*, jarvis, virtio_gpu, uvc_driver, vision_agent)
- Feature `jarbas-bridge` removida â€” wire always-on (`k_nano` sem `global-alloc` resolve conflito allocator)
- `jarbas_bridge.rs` compara TOPIC_* via `jarbas_crate::audio` (audio truth permanece monÃ³lito)
- Gate `[N5-JARBAS] full_wire=OK(jarbas-crate)`
- `paint_tts_response` / `boot_splash` portados para `jarbas/src/display/fb.rs`

### Residual monÃ³lito (integraÃ§Ã£o bin)
- `audio/*` â€” ADR-0045 truth path + Sprint107 wakeword (`voice.rs` diverge do espelho jarbas)
- `jarbas_fb.rs` â€” CapGate P4 FB demo (bin-only)
- `jarbas_bridge.rs` â€” cross-check TOPIC_* monÃ³lito vs jarbas-crate

### Marco
- **Wire crates N2.5â†’N5.7 âœ…** â€” cadeia KÂ³CHJ linkada no bin; qualidade voz â†’ Sprint Sound

## [1.7.10] â€” 2026-07-16 â€” ADR-0042 N4.6 (hermes wired no bin)

### Wired
- **N4.6** `neural-kernel` â†’ `hermes-crate` (dep direta `package = "hermes"`)
- `pub use hermes_crate::{actor_registry, apps, cron, hermes, safety, security, wasm*, wifi*, â€¦}`
- Removidos 37 espelhos monÃ³lito (hermes, cron, wasm_rt, skill_*, wifi_*, apps/, â€¦)
- Alias `hermes-crate` evita conflito com mÃ³dulos re-exportados
- Gate `[N4-HERMES] full_wire=OK(hermes-crate)`

### Residual monÃ³lito (integraÃ§Ã£o bin)
- `agents.rs` â€” fleet nativo + HermesAgent; globals em `main.rs`
- `cognitive.rs` â€” engine Sprint 95 (nÃ£o no crate)
- `net*` + `rtl8139`/`e1000`/`virtio_net` â€” NETSTACK singleton + virtio init
- `fs/*` â€” VFS monÃ³lito (`inference_fs_agent`, `mhi_scheduler`)
- `aios_api.rs` â€” CapGate P3
- `micropython_wasm.rs` â€” loader via `crate::fs`

### PrÃ³ximo
- Sprint Sound â€” qualidade voz (STT/Piper/soft-float); Micâ†’Wake runtime

## [1.7.9] â€” 2026-07-16 â€” ADR-0042 N3.5 (cortex wired no bin)

### Wired
- **N3.5** `neural-kernel` â†’ `cortex-crate` (dep direta `package = "cortex"`)
- `pub use cortex_crate::{arena, bitnet_avx2, burn_flex, delta, nn, r3, tensor, trinity, tv_dsl}`
- Removidos 9 espelhos monÃ³lito (tensor, trinity, arena, r3, â€¦)
- Alias `cortex-crate` evita conflito com `mod cortex` (integraÃ§Ã£o LLM/EventBus/load_status)
- Trinity crate sync: Sprint 107 generator-first + `moe_router_loaded` / `has_generator`

### Residual monÃ³lito (integraÃ§Ã£o bin)
- `cortex.rs` â€” generate path, EVENT_BUS, demo_flags, allocator resize
- `bpe.rs` â€” BPB1 + weather-e2e lexicon + FAT/QEMU loader
- `global_arena.rs` â€” pending route Hermesâ†’Cortex
- `cortex_mmap.rs` â€” ADR-0041 P5/P7 (nÃ£o no crate)

### PrÃ³ximo
- **N4.6** wire `hermes` crate

## [1.7.8] â€” 2026-07-16 â€” ADR-0042 N2.5 (k_ai wired no bin)

### Wired
- **N2.5** `neural-kernel` â†’ `k_ai` + `k-nano` (sem feature `global-alloc`)
- Removidos espelhos `trust.rs` / `self_heal.rs`; `pub use k_ai::{trust, self_heal}`
- Bridge `memory` â†’ `k_nano::memory` (GLOBAL_ALLOCATOR Ãºnico para boot + SelfHeal)
- Bridge `EVENT_BUS` â†’ `k_nano::globals::EVENT_BUS` (HEALTH_ISSUE no mesmo bus)
- `k_nano` feature `global-alloc` (default OFF) gateia `#[global_allocator]` no crate lib

### PrÃ³ximo
- **N3.5** wire `cortex` â€” remover espelhos `cortex.rs`, `bpe.rs`, `tensor.rs`, `trinity.rs`, â€¦

## [1.7.7] â€” 2026-07-16 â€” ADR-0042 N5 CLOSED (jarbas ego/UI)

### Closed
- **N5.1â€“N5.6** funcionais: DisplayAgent compositor + GPU FB + P4 jarbas_fb; JarvisAgent persona 16-stage; voice agents (`jarvis_voice`/`wakeword`/`audio_mixer`) via Hermes only; `paint_tts_response` FB; voice e2e (GATED boot default + prior Sprint107 TTS+FB); IPCâ†hermes topics mirror honesto
- Serial gate `[N5-JARBAS] â€¦ criteria=MET` â€” evidÃªncia `logs/boot_n5_20260716_145943.txt`
- **N5.7** link crate `jarbas` no bin = deferred (espelho monÃ³lito; padrÃ£o N2.5/N3.5/N4.6)
- STT/Piper/soft-float quality â†’ Sprint Sound (nÃ£o bloqueia N5)

### Marco
- **N1â€“N5 funcionais âœ…** â€” gate `v2.0.0` pode ser **discutido**; wire crates N2.5â€“N5.7 e qualidade voz permanecem deferred

### NÃ£o Ã©
- Crate `jarbas` wired no bin, voz production-grade, ou declaraÃ§Ã£o automÃ¡tica de `v2.0.0` sem review de qualidade ADR

## [1.7.6] â€” 2026-07-16 â€” ADR-0042 N4 CLOSED (hermes orquestra)

### Closed
- **N4.1â€“N4.5** funcionais: HermesAgent intent routing (`USER_INTENT`/`HERMES_RESPONSE`), ReAct 7 fases + `SKILL_REGISTRY` + WASM SFI hub, `global_arena`â†’`generate_via_model`, EventBus intent e2e (GATED boot default + prior weather-e2e L5), IPCâ†’jarbas topics mirror honesto
- Serial gate `[N4-HERMES] â€¦ criteria=MET` â€” evidÃªncia `logs/boot_n4_20260716_144651.txt`
- **N4.6** link crate `hermes` no bin = deferred (espelho monÃ³lito; padrÃ£o N2.5/N3.5)
- Voz/STT quality â†’ Sprint Sound (nÃ£o bloqueia N4)

### NÃ£o Ã©
- Crate `hermes` wired no bin, jarbas ego pleno, ou `v2.0.0` (falta N5)

## [1.7.5] â€” 2026-07-16 â€” ADR-0042 N3 CLOSED (cortex cÃ©rebro)

### Closed
- **N3.1â€“N3.4** funcionais: BitNet 2B `llm=LOADED`, Cap MAP_WEIGHTS (P5), Trinity 6 experts + HWEXPERT/RustCoder, generate path (GATED soft-float no boot default + prior weather-e2e HIT)
- Serial gate `[N3-CORTEX] â€¦ criteria=MET` â€” evidÃªncia `logs/boot_n3_20260716_132753.txt`
- **N3.5** link crate `cortex` no bin = deferred (espelho monÃ³lito; padrÃ£o N2.5)
- Soft-float fluency / TTS quality â†’ Sprint Sound (nÃ£o bloqueia N3)

### NÃ£o Ã©
- Chat fluente 24/7, float/AVX path pleno, ou `v2.0.0` (falta N4â€“N5)

## [1.7.4] â€” 2026-07-16 â€” ADR-0042 N2 CLOSED (SelfHeal VID-gated + Trust)

CritÃ©rios funcionais N2 âœ…. Package Cargo permanece `1.0.0` (tag-only). **NÃ£o** Ã© v2.0.0.

### N2 (k-ai HW-AI / SelfHeal)
- Boot path: Trust `(token,agent,skill)` â†’ inventÃ¡rio PCI â†’ `run_vid_gated_scan` com heal/noop + HEALTH_ISSUE
- Fine-gate: Intel net `8086` class 02/0D **exclui** Ethernet nativo (`subclass==0x00`, ex. e1000) â€” alinhado Ã  polÃ­tica NVIDIA (sem falso positivo)
- Honest noop quando `fw_gated=0`; hermes residual gate commitado (usa `k_ai` real)
- **N2.5:** link crate `k_ai` no bin ainda bloqueado por `#[global_allocator]` â€” comportamento via espelho `neural-kernel` atÃ© entÃ£o

### EvidÃªncia
- `logs/boot_n2_20260716_131837.txt` â€” `[TRUST] allow` + `[N2-SELFHEAL]` inventory/honest noop/gate complete
- `cargo nk` = 0 erros

### Docs
- ADR-0042 checklist N2 âœ…; STATE; SESSION_112; IDEA #435 âœ…

## [1.7.3] â€” 2026-07-16 â€” Docs: handoff voz 107 â†’ Sprint Sound + pista ADR-0042

Docs-only. Sem mudanÃ§a de runtime. Package Cargo permanece `1.0.0` (hÃ¡bito tag-only).

### Docs
- Sprint **107 Voice** marcada **FECHADA** (PASS parcial forte+) â€” entregas permanecem; gaps de voz **nÃ£o** sÃ£o mais 107
- Backlog voz migrado para **Sprint Sound (reaberta)**: STT retrain, Micâ†’Wake runtime, Piper VITS pleno, soft-float latency, UAC, jarbas wire, VAD/SER/Wake polish
- **Pista ativa** = ADR-0042 N2â†’N5 (voz nÃ£o bloqueia)
- STATE / TODO / ROADMAP / AGENTS / IDEA_BANK / TECNOLOGIAS Â§5 / ADR-0045 / SESSION_111 alinhados

## [1.7.2] â€” 2026-07-16 â€” Sprint 107 loops 1â€“5 (clima PASS parcial forte)

Marco funcional pÃ³s-ADR-0045. Package Cargo permanece `1.0.0` (hÃ¡bito tag-only).

### Clima e2e (Loop 5 â€” `logs/boot_whpx_20260716_033322.txt`)
- GEN: `decoded_len=12 text='O tempo esta'` â€” frase PT climÃ¡tica (logits + mÃ¡scara; nÃ£o canned)
- TTS: Piper neural-lite (`emb.weight`) Â· `pcm_samples=15428` + FB paint
- WakeWordAgent registrado no AgentFleet
- STT CTC LOADED (path real) mas `ctc=''` â†’ seed prompt
- Experts: RUSTCODER/STT/BGE OK; HWEXPERT parse FAILED (header vocab u16)

### Code (loops 1â€“5)
- cortex/bpe generate constrained weather + chat frame Llama
- Piper neural-lite + convert_piper; STT path hardening
- WakeWord register; QEMU loaders BPE/HW/RustCoder/STT; weather e2e scripts

### Known gaps (â†’ Sprint Sound reaberta / v1.7.3 docs)
- Soft-float tkn/s; STT CTC retrain; Micâ†’WakeWordâ†’STTâ†’LLMâ†’TTS runtime e2e; Piper VITS pleno; jarbas/audio wiring; UAC

## [1.7.1] â€” 2026-07-16 â€” ADR-0045 Sound Voice Stack (docs)

DocumentaÃ§Ã£o do stack de voz **real** no boot. Sem mudanÃ§a de runtime. Package Cargo permanece `1.0.0` (hÃ¡bito tag-only).

### Docs / ADR
- **ADR-0045** `docs/architecture/0045-sound-voice-stack.md` â€” truth = `neural-kernel/src/audio/*`; `jarbas/audio` = espelho nÃ£o wired
- Stack canÃ´nico: HDA + Piper VITS (+ formant fallback) + STT CTC + VAD + mixer + FB TTS paint
- Supersede como primÃ¡rio: sherpa-onnx, Pocket TTS, Kokoro-82M, Vosk, Wyoming, Rustpotter
- IDEA_BANK: #75/#83 âœ…; #84 UAC ðŸŸ¡ futuro; #315.21â€“25 / #315.N+1 / #360 âŒ supersedido; B-01 voz desbloqueado (SLIP #415)
- STATE / SESSION_109 / SESSION_INDEX / TECNOLOGIAS Â§5 / TODO Sprint 107 alinhados
- Gaps Sprint 107 documentados (WakeWord nÃ£o registrado; Piper neural fraco; loop TTSâ†”STTâ†”LLM aberto) â€” **superseded by 1.7.2** (WakeWord registrado; GEN+TTS neural-lite)

## [1.7.0] â€” 2026-07-15 â€” N1 âœ… + BitNet 2B LOADED (N3 parcial)

Marco QEMU alÃ©m de â€œsÃ³ N1â€. Linha **1.6.0-dev absorvida/superseded** (sem tag `v1.6.0` vazia). Package Cargo permanece `1.0.0` (hÃ¡bito tag-only, como 1.5.7). **NÃ£o** Ã© `v2.0.0` (ADR-0042 gate = N1â€“N5).

### N1 â€” k-nano legÃ­vel âœ…
- **N1.1** `load_status::{LoadStatus,AssetKind}` + banner `[STATUS] llm=â€¦ bge=â€¦ piper=â€¦ fw_gpu=â€¦`
- Removido log falso `modelo 2B carregado da FAT32` sem prova â†’ `LLM ABSENT` / LOADED coerente com `[LLM-TEST]`
- **N1.2** Probe NVIDIA FW (`test_load_firmware` / ACR) **sÃ³** se `GpuVendor::Nvidia`; QEMU 1234:1111 â†’ skip; CapGate bootstrap documentado (DENY demos esperados)
- **N1.3** Hook `agent_core::set_sched_metrics_hook` â†’ log periÃ³dico `[SCHED] tick= agents= polled=`

### N3 parcial â€” BitNet 2B LOADED de verdade
- EvidÃªncia `logs/boot_whpx_20260715_112049.txt`: QEMU-loader @0x100000000, **~590MB**, ver=4 h=2560 **L=30**, `LLM LOADED file=BITNET2B`
- Path STT-sim â†’ Hermes â†’ **FWD layers 0â€¦29/30**
- Export/convert v4 (`tools/convert_bitnet.py`) alinhado a `load_model` dims

### QEMU / ops / FAT
- Soft-float + multicore: `.cargo/config.toml` (`jobs`/`-Z threads`, `-sse*`, alias `cargo nk`)
- Disco/FAT: free-cluster scan por setor; `mkfat32` / `BITNET2B.BIN`; scripts `-RamGB 6 -Smp 4`
- Disco slim: `tools/mkfat32_slim_qemu.py`; full via `build_image.py`

### Known issue (e2e clima PARCIAL)
- `[JARBAS-TTS] FAILED empty generate` â€” generate/TTS ainda aberto; LOADED+FWD â‰  resposta falada

## [1.5.7] â€” 2026-07-14 â€” Boot A/B + ADR-0041 Capability Ladder P0â€“P9

PoC capability no monÃ³lito `neural-kernel` (commits `9bb1382`â€¦`49c4301`). Package Cargo permanece `1.0.0` (hÃ¡bito do repo); release via CHANGELOG + tag git.

### Pacote A â€” Boot endurecido
- STI/PIC, stack heap â‰¥2MB, `init_phase` round-robin, `BOOT_PHASE` + consumer, DiagnosticSkill, logs/docs de heap

### Pacote B â€” Ordem de bring-up
- `init_platform_sync` (PCI+ACPI+APIC+SMP) **antes** dos probes de driver
- PlatformAgent / NetDriverAgent idempotentes se sync jÃ¡ rodou
- Agency SpecialistAgent: Continuous â†’ EventDriven

### MVP C â€” ADR-0041 Capability Rings (PoC)
- `AddressSpace` + CR3 switch Aâ†’Bâ†’kernel (IRQ mascaradas na janela)
- `SharedSpscRing` em pÃ¡gina compartilhada; Cap bitflags + trap `int 0x90`
- Demo pÃ³s-DriverInit non-fatal (WARN + boot continua)
- Arquivos: `address_space.rs`, `syscall.rs`, `ipc/*`

### Docs
- ADR-0041, STATE/IDEA_BANK/SESSION_107, TECNOLOGIAS 2.10

### P3 â€” Hermes CapabilityGate (ADR-0041)
- `capability_gate.rs`: gate `aios_send_tcp` / `aios_write_ring` por `Cap::{SEND_TCP,WRITE_RING}`
- Hermes skills net/* + `wasm_rt::host_call_gated`; demo boot non-fatal
- Deny sem Cap â†’ log serial `[CapGate] DENY`

### P4 â€” JARBAS FB MMIO + double-buffer (ADR-0041)
- `jarbas_fb.rs`: contrato `FbContract` (bootloader FB), map AS JARBAS (`JARBAS_FB_VA`), Cap `MAP_FB`/`WRITE_FB`
- Double-buffer heap + `present` + stub vsync (`TIMER_TICKS`/`sfence`); demo boot non-fatal pÃ³s-P3
- Sem FB fÃ­sico â†’ Cap-only path SUCCESS; falha â†’ WARN, boot continua

### P5 â€” K-IA DMA pin + Cortex weight mmap (ADR-0041)
- `k_ia_dma.rs`: pin frames nÃ£o-reclaimÃ¡veis + map AS (`K_IA_DMA_VA`), Cap `PIN_DMA`/`MAP_DMA`; VirtIO phys stub
- `cortex_mmap.rs`: mmap N pÃ¡ginas peso simuladas em `CORTEX_WEIGHT_VA` (eager), Cap `MAP_WEIGHTS`; demand-paging/GGUF TODO
- Demo boot non-fatal pÃ³s-P4; falha frame alloc â†’ Cap-only / WARN, sem panic

### P6 â€” Ring3 user-mode real (ADR-0041)
- GDT user code/data + TSS RSP0; IDT `int 0x90` DPL=3
- `user_mode.rs`: `enter_user_mode` via `iretq`, stub USER (marker + EXIT), return jmp kernel; Cap `ENTER_USER`
- `map_user_page` com USER em toda a cadeia PT; demo boot non-fatal pÃ³s-P5; #GP/#PF abort durante demo
- Flag `TRY_ENTER_RING3` para disable se necessÃ¡rio

### P7 â€” Demand-paging via #PF (ADR-0041)
- `demand_page.rs`: registry lazy (frames prÃ©-alocados); `#PF` instala leaf PRESENT e retorna (retry)
- `cortex_mmap::mmap_weights_lazy` + Cap `DEMAND_PAGE` / `SYS_DEMAND_PAGE`; `AddressSpace::reserve_page`
- Demo boot non-fatal pÃ³s-P6: first-touch R/W curado; deny sem Cap; falha â†’ WARN

### P8 â€” VirtIO vring + DMA pin (ADR-0041)
- `virtio_vring.rs`: Virtqueue layout-compatible (desc+avail+used) sobre `k_ia_dma::pin_frames`; Cap `VRING_SETUP`
- `Desc.addr` aponta para pÃ¡gina pinnada (zero-copy); path paralelo â€” NIC live observe-only
- Sem VirtIO device â†’ layout-only SUCCESS; demo boot non-fatal pÃ³s-P7

### P9 â€” GGUF/FAT file-backed mmap (ADR-0041)
- `gguf_mmap.rs`: prÃ©-lÃª 1â€“4 pÃ¡ginas de `BITNET.BIN`/`HWEXPRT.BIN`/â€¦ via FAT `read_file_range`; Cap `MAP_FILE`
- Frames prÃ©-preenchidos + `demand_page::register_lazy` (`FILE_WEIGHT_VA`); #PF sÃ³ PRESENT (sem I/O no fault)
- Fallback stub `NFIL` se arquivo ausente; demo boot non-fatal pÃ³s-P8 (deny â†’ mmap â†’ touch magic â†’ restore)

## v2.0.0 â€” 2026-07-13 â€” Sprint 106: Ecossistema de AnÃ©is LÃ³gicos

### Sprint 106-11: CorreÃ§Ã£o de boot em HW real
- **Heap address:** Alterado de `0x4444_4444_0000` para `0x4000_0000_0000` (1TB) â€” endereÃ§o mais seguro para hardware real
- **AHCI/SATA:** Verificado suporte AHCI jÃ¡ implementado em `ahci.rs` â€” sistema suporta tanto ISA ATA quanto SATA AHCI
- **Display/Framebuffer:** Sistema requer UEFI GOP ativo para framebuffer grÃ¡fico. Sem GOP, fallback para VGA text mode (80x25)
- **DiagnÃ³stico vÃ­deo:** Logs mostram "Sem framebuffer UEFI â€” VGA text mode" em QEMU. Bootloader 0.11 nÃ£o expÃµe configuraÃ§Ã£o de framebuffer via API. GOP depende do firmware UEFI/OVMF.
- **SoluÃ§Ã£o:** Para HW real, garantir UEFI GOP ativo no firmware. Para QEMU, usar OVMF com `-vga std` para framebuffer grÃ¡fico.
- **ValidaÃ§Ã£o:** `cargo check --release` com 0 erros (2 warnings menores em ata.rs, nÃ£o crÃ­ticos)
- **Motivo:** EndereÃ§o de heap muito alto (0x4444_4444_0000) pode causar problemas de mapeamento de memÃ³ria em hardware real

### ADR v2.0 â€” RefatoraÃ§Ã£o para Workspace Estrito
- **workspace Cargo:** 11 membros (ticket-lock, neural-kernel, agent-core, skill-registry, event-bus, boot, k_nano, k_ai, cortex, hermes, jarbas) com `resolver = "2"`
- **Rename:** k_ia â†’ k_ai (Ring 1 LÃ³gico), jarvis â†’ jarbas (Ring 2 HCI)
- **Backups:** Pastas antigas preservadas (LEGACY/k_ia, LEGACY/jarvis)

### Sprint 106-1: Estruturar Cargo workspace estrito
- **Cargo.toml raiz:** `members = ["crates/k_nano", "crates/k_ai", "crates/cortex", "crates/hermes", "crates/jarbas"]` + dependÃªncias auxiliares
- **Isolamento:** DependÃªncias nÃ£o vazam entre camadas lÃ³gicas
- **Cargo.lock:** Regenerado com resolver = "2" para dependÃªncias transativas
- **ValidaÃ§Ã£o:** `cargo check --release` com 0 erros

### Sprint 106-2: Cargo clean + Workspace Sanity Check
- **cargo clean -p neural-kernel:** Build artifacts removidos (target/), cÃ³digo fonte preservado
- **cargo check --release:** Validado 0 erros (2 warnings menores em ata.rs, nÃ£o crÃ­ticos)
- **PreservaÃ§Ã£o:** Nenhum arquivo fonte deletado â€” apenas build cache

### Sprint 106-3: Corrigir SOUL.md parser (dependÃªncia ring2â†’ring0)
- **Cargo.toml jarbas:** adicionado `neural-kernel = { path = "../neural-kernel" }`
- **jarvis.rs:** `load_from_fat32()` usa `neural_kernel::fs::read_vfs("/SOUL.MD")` em vez de `k_nano::ATA_DRIVER.lock()` + `crate::fat32::Fat32Reader`
- **Isolamento:** jarbas (ring2) nÃ£o acessa mais k_nano (ring0) diretamente para hardware â€” apenas serviÃ§os comuns (serial_println, EVENT_BUS, AUDIT_TRAIL)
- **ValidaÃ§Ã£o:** `cargo check --release` com 0 erros

### Sprint 106-4: Corrigir Trinity MoE Router
- **InvestigaÃ§Ã£o:** `trinity.rs` usa apenas `k_nano::serial_println!()` para logging (aceitÃ¡vel)
- **ExpertKind enum:** Simples, sem dependÃªncias externas
- **Trinity Router:** Classifica intents via ML/keyword matching â€” **nÃ£o roteia para hardware especÃ­fico**
- **ValidaÃ§Ã£o:** Build com 0 erros, nenhuma dependÃªncia circular detectada

### Sprint 106-5: RustPython no_std (Rota Nativa)
- **Viabilidade investigada:** RustPython **NÃƒO Ã© no_std nativo** â€” depende de `std` para alocaÃ§Ã£o dinÃ¢mica
- **Rota principal WASM (106-6):** Compilar RustPython para .wasm via `cargo build --target wasm32-wasip1`
- **Alternativa documentada:** Bridge C via `abi_x86_interrupt` exigiria portar RustPython para no_std (trabalho enorme)

### Sprint 106-6: MicroPython via WASM (Rota Sandbox)
- **CompilaÃ§Ã£o:** MicroPython para .wasm
- **Sandbox:** Hermes executor com isolamento

### Sprint 106-7: Corrigir page faults (ordem de inicializaÃ§Ã£o)
- **Ordem correta:** allocator â†’ events â†’ agents
- **lazy_init!():** Macro para agentes dependentes de heap
- **Validado:** `cargo run --release` sem page faults

### Sprint 106-8: AIOS API para Python (RAG + System Prompt)
- **Bibliotecas:** aios_net, aios_fs
- **InjeÃ§Ã£o:** Via RAG/System Prompt no RustPython

### Sprint 106-9: Escalonamento Evolutivo de CÃ³digo (JIT Cognitivo)
- **SkillOpt + Knowledge Graph:** Python efÃªmero â†’ WASM cravado em pedra
- **EvoluÃ§Ã£o:** CÃ³digo evolve de JIT para JIT Cognitivo

### Sprint 106-10: SkillOpt - TraduÃ§Ã£o Pythonâ†’Rust no_std
- **GeraÃ§Ã£o:** Rust no_std a partir de Python via Cortex LLM
- **Automatizado:** Pipeline de traduÃ§Ã£o integrado

### Refactor
- **`RingBufStore` extraÃ­do** em `fs/mod.rs` â€” tipo genÃ©rico com evicÃ§Ã£o FIFO por quota
- **`ram_fs_agent.rs`** delegado para `RingBufStore::new(1MB)` â€” ~40 LOC eliminados
- **`log_fs_agent.rs`** delegado para `RingBufStore::new(256KB)` â€” ~50 LOC eliminados
- **`hermes/src/fs/`** tambÃ©m atualizado com `RingBufStore` (consistency com monÃ³lito)

### Safety
- **`LEGACY/v1.5-neural-kernel-src/`** â€” snapshot de todo `crates/neural-kernel/src/` antes da migraÃ§Ã£o v2.0
- Nada foi deletado â€” refactor puro por extraÃ§Ã£o

# Changelog â€” neural-os-core v1.5.2 "Ring Buffer Refactor"

## v1.5.2 â€” 2026-07-13 â€” Ring Buffer Refactor

### Refactor
- **`RingBufStore` extraÃ­do** em `fs/mod.rs` â€” tipo genÃ©rico com evicÃ§Ã£o FIFO por quota
- **`ram_fs_agent.rs`** delegado para `RingBufStore::new(1MB)` â€” ~40 LOC eliminados
- **`log_fs_agent.rs`** delegado para `RingBufStore::new(256KB)` â€” ~50 LOC eliminados
- **`hermes/src/fs/`** tambÃ©m atualizado com `RingBufStore` (consistency com monÃ³lito)

### Safety
- **`LEGACY/v1.5-neural-kernel-src/`** â€” snapshot de todo `crates/neural-kernel/src/` antes da migraÃ§Ã£o v2.0
- Nada foi deletado â€” refactor puro por extraÃ§Ã£o

# Changelog â€” neural-os-core v1.5.1 "Ponytail Audit"

## v1.5.1 â€” 2026-07-13 â€” Ponytail Audit

### Cleanup (600 LOC removidos, 11 dep entries)
- **Deps removidas:** `pic8259` de 4 Cargo.tomls; `ed25519-compact`, `linked_list_allocator`, `bootloader_api` de crates que nÃ£o usam
- **smoltcp features podadas:** `socket-dns`, `proto-dns` removidas (nunca usadas â€” DNS via UDP raw)
- **6 arquivos deletados:** `cfs.rs`, `hal.rs`, `time_utils.rs`, `wifi_aer.rs`, `wifi_dma.rs`, `wifi_apic.rs` â€” todos `#[allow(dead_code)]` sem chamadores
- **3 funÃ§Ãµes mortas removidas:** `ram_used_bytes()`, `agent_for_mount()` (stub), `scheduler_stats()` (stub)
- **14 branches `#[cfg(not(target_arch = "x86_64"))]` removidos** de `tensor.rs`, `simd.rs`, `bitnet_avx2.rs` â€” portabilidade especulativa para arquiteturas inexistentes
- **Trait `Architecture` + static `ARCH` removidos** de `hal.rs` â€” marcado `@dead` pelo autor, zero chamadores
- **`PICS` lazy_static + `init_pics()` removidos** de `interrupts.rs` â€” kernel sÃ³ usa APIC

### KÂ³CHJ Workspace Migration (v1.5.0)
- MonÃ³lito `neural-kernel` â†’ 5 crates (k_nano, cortex, hermes, k_ia, jarvis)
- `tools/migrate_k2chj.py`: 193 arquivos mapeados, 79 refs cross-crate corrigidas
- k_nano compila independentemente (0 erros)
- neural-kernel intacto como bin crate (build 0 erros)

## v1.2.0 â€” 2026-07-12 â€” ATA Liberation

### ATA PIO Bug Fix (crÃ­tico â€” afeta v0.1 atÃ© v1.1.5)

- **Root cause:** `read_sectors()` e `identify()` usavam `in al, dx` + `in al, dx+1` para ler palavras de 16 bits do disco ATA. O port `io_base+1` nÃ£o contÃ©m o segundo byte do dado â€” Ã© o registrador FEATURES/ERROR. CorreÃ§Ã£o: usar `in ax, dx` para ler a palavra completa do registrador de dados.
- **Impacto:** TODO acesso a disco desde o inÃ­cio do projeto (v0.1, 2026-05) era lixo. MBR, FAT32, modelos .bitnet, firmwares, credenciais WiFi â€” nada era lido corretamente. Apenas discos detectados como "presentes" mas dados corrompidos.
- **Probe ATA:** Agora prefere disco com partiÃ§Ã£o FAT32 (type 0x0B/0x0C) sobre GPT (type 0xEE). Antes escolhia o primeiro com MBR â€” que era o bootloader (uefi.img), nunca o disco de dados.
- **Log QEMU confirmado:** `[ATA] ISA 496: slave FAT32! (type=0xc)` + `[FAT32] BPB: bps=512 spc=1`

## v1.1.5 â€” 2026-07-12 â€” Silicon Afterlife

### Sprints v1.1.x: GPU Compute + WiFi + Visual 3-Camadas + SelfHealing

- **v1.1.1 â€” GPU + Firmware + HW Expert v3** (1.200 LOC)
  - Firmware ACR loading: pipeline WPR implementado, blobs NVIDIA GP108 em firmware/
  - HW Expert v3 treinado: 61.453 VID/DID Ãºnicos (SDIO + pci.ids + usb.ids + kernel)
  - Modelo 128h/6L/8heads, 1M params, 259KB, loss 0.389
  - 171.003 HWIDs SDIO de 65 DriverPacks, 20.054 .inf
  - 48.346 registros oficiais pci-ids + usb-ids + kernel PCI tables
  - Firmware metadata: WHENCE (998) + headers + AMD ucode (64 patches)
  - regulatory.db: 174 paÃ­ses WiFi

- **v1.1.2 â€” SelfHealing + HWID Datasets** (800 LOC)
  - SelfHeal I3: firmware ausente â†’ HEALTH_ISSUE
  - SelfHeal I4: skill ausente â†’ HEALTH_ISSUE  
  - firmware.rs: hot_load_firmware(vid, did, class) universal
  - HermesAgent: inscrito em HEALTH_ISSUE â†’ LLM diagnostica
  - mkfat32.py: firmware incluso como FW_* no FAT32

- **v1.1.3 â€” 3 Camadas Visuais + Audio + Rede** (600 LOC)
  - Z-order real: Layer enum (OrbBackground < HermesOverlay < AppWindows < DockBar)
  - FPS control a 60Hz (LAST_FRAME_TICK)
  - Hermes CLI overlay semi-transparente sempre visÃ­vel
  - FFT audio (Goertzel 16 bins) â†’ animaÃ§Ã£o do Orbe
  - Mouse PS/2 integrado: dock bar, close, drag
  - HDA playback (SD1): TTS finalmente chega ao auto-falante
  - BrowserAgent real: HTTP GET via smoltcp TCP com DNS resolve
  - DHCP starvation detection (SecurityAgent)

- **v1.1.4 â€” WiFi Intel AX200** (260 LOC)
  - iwlwifi CSR/HBUS/SRAM registers (0x000-0x29C)
  - ucode loading pipeline: wake â†’ reset â†’ seÃ§Ãµes â†’ alive
  - Command/response via SRAM + doorbell NMI
  - Scan via comando 0x34
  - 5 firmware blobs: cc-a0 (AX200), Qu (AX101), so-a0-gf/hr (AX201/210), ty-a0-gf (AX211)
  - ~7.5MB firmware Intel WiFi em firmware/intel/iwlwifi/

- **v1.1.5 â€” IntegraÃ§Ã£o + DocumentaÃ§Ã£o** (50 LOC)
  - Sprint plan atualizado com progresso real
  - AGENTS.md expandido com liÃ§Ãµes v1.1.x
  - Build release: 0 erros, ~26.000 LOC, 180+ arquivos

## v1.0.0 â€” 2026-07-11 â€” A Era do SilÃ­cio

### Sprints 92-100: FundaÃ§Ã£o EstÃ¡vel â†’ Code Freeze

- **Sprint 92** â€” FundaÃ§Ã£o EstÃ¡vel: VirtIO-MMIO, AHCI probe, Zero-Trust Syscall, WHPX+AVX2 fix
- **Sprint 93** â€” WASM Runtime + IDE: WasmExec VM, PluginHub, SkillMarket, BitNet IDE
- **Sprint 94** â€” GPU Polish: MSched Belady, Observability, Human-in-the-Loop, Actor Registry
- **Sprint 95** â€” Memory + VFS: HNSW index, MHI+FS Bridge, Inference/Hermes/RamFs agents
- **Sprint 96** â€” GGUF + Model Loading: GGUF parser, RoPE, .bitnet v3/v4, /model swap
- **Sprint 97** â€” Rede + AIOS: http_get real, SearchAgent, SelfUpdate A/B slots, ContextWindow
- **Sprint 98** â€” Training: TrainingAgent, DataCollector, WakeWordML, Intel compute dispatch
- **Sprint 99** â€” SkillOpt + Structured Decoding: Compressed FSM, 6 decode modes, SkillOptimizer
- **Sprint 92b** â€” Code Cleanup + ZT Syscall: 94 warningsâ†’0, check_syscall() wireado, serial bridge watchdog
- **Sprint 93b** â€” WASM refinements: parse_description refatorado, auto-rollback via snapshot
- **Sprint 94b** â€” LLM Icons + Human-in-the-Loop: generate_llm_icon() integrado, /approve/deny/pending
- **Sprint 96b** â€” GGUF Streaming + FAT32 chunked: load_gguf_header_from_disk(), read_file_range()
- **Sprint 97b** â€” RssAgent + EmailAgent: RSS/Atom parse + SMTP via http_get_raw()
- **Sprint 98b** â€” HW Expert GPU Training: 43.339 PCI+USB devices, loss 0.097, 95.4% accuracy
- **Sprint 99b** â€” Ponytail Audit: removed 19 dead files (~500 LOC), 3 dead deps (edge-dhcp, embedded-graphics, buddy-alloc), ~32 transitive crate nodes cleaned
- **Sprint 100** â€” **Code Freeze & Release v1.0.0**
- **Sprint 101** â€” **v2.0 CogniÃ§Ã£o**: Piper TTS VITS multilÃ­ngue, STT CTC engine, HDA audio DMA, NVIDIA PUSH_BUFFER GPU, ATA slave, RustCoder treinado

### Funcionalidades Principais

- **Bare-metal Rust kernel**: bootloader 0.11.15, IDT, GDT, TSS, SMP, APIC, ACPI, PCI
- **GPU**: VirtIO-GPU, Intel ring buffer, NVIDIA/AMD probe, VRAM buddy, display coexistence
- **LLM**: BitNet ternary (~850M params), 4-layer transformer, Medusa speculative decoding
- **Trinity MoE**: 5 experts + router (hw_identify, rust_coder, disk_diag, security, generator)
- **Rede**: smoltcp TCP/IP, VirtIO-net, RTL8139, E1000, serial tunnel SLIP, DNS, HTTP
- **WASM Runtime**: Custom VM with fuel, sandbox, 9 built-in skills, PluginHub
- **Filesystem**: FAT32, VFS, 7 agents (ATA, dev, proc, inference, hermes, ramfs, logfs)
- **HNSW**: Hierarchical Navigable Small World for approximate nearest neighbor search
- **Ãudio**: HDA driver, pocket TTS (neural), formant synth, VAD, wake word, mixer
- **SeguranÃ§a**: Ed25519 signing, TPM 2.0, TrustCache, Zero-Trust syscall, Audit Trail

### MudanÃ§as desde v0.109.x

- 165+ arquivos Rust, ~19.000 LOC, ~50 agentes nativos, 0 erros de compilaÃ§Ã£o
- 461 commits desde o primeiro boot

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/),
and this project adheres to [Semantic Versioning](https://semver.org/)
with [Conventional Commits](https://www.conventionalcommits.org/).

## [0.109.3-b01-morto] â€” 2026-07-09 â€” ðŸ†ðŸ”¥ B-01 MORTO! Serial tunnel TCP bridge

### O bloqueador de 18 sprints finalmente caiu â€” B-01
**O kernel recebeu dados reais pela primeira vez:**
```
[BRIDGE] RX #1: 304 bytes â† KERNEL RESPONDEU!
[BRIDGE] RX #2: 42 bytes
[BRIDGE] RX #3: 42 bytes
[BRIDGE] RX #4: 42 bytes
```
ComunicaÃ§Ã£o bidirecional entre kernel bare-metal e host Windows via serial tunnel.

### Causa raiz do B-01
NÃ£o era bug no kernel. Era incompatibilidade entre:
- **Windows 11**: firewall loopback bloqueia TCP inbound, named pipes tÃªm chicken-and-egg
- **QEMU TCG**: emulaÃ§Ã£o de NIC (RTL8139/E1000) RX nÃ£o injeta DMA de forma confiÃ¡vel
- **Kernel**: cÃ³digo Rust correto desde o inÃ­cio â€” TX funcionava, RX=0 por isolamento fÃ­sico

### SoluÃ§Ã£o: bypass serial TCP (inversÃ£o de topologia)
- `slip.rs` (82 LOC): driver serial COM2 com framing length-prefix, non-blocking
- `serial_bridge.py`: bridge TCP **servidor** (Python escuta, QEMU conecta como cliente)
- `-serial tcp:127.0.0.1:4444`: QEMU como cliente TCP, nÃ£o servidor
- `nic_send/nic_recv`: serial tunnel como fallback universal no pipeline

### Arquitetura final do pipeline de rede
```
Browser/curl â†’ WiFi Windows â†’ localhost:4444 â†’ QEMU TCP â†’ COM2 serial
  â†’ slip::recv() â†’ nic_recv() â†’ NetPhy::receive() â†’ smoltcp â†’ socket TCP
```

### SystemEnv â€” kernel sabe onde estÃ¡ rodando
- `env.rs`: SystemEnv enum (QemuSandbox/VBoxSandbox/HwReal/Offline)
- Detectado no boot por CPUID hypervisor + presenÃ§a de NIC
- Serial tunnel sÃ³ ativo em sandbox (QEMU/VBox) sem NIC
- Cortex, Hermes, JARVIS consultam via `crate::env::get()`

## [0.109.2-rtl8139-rx-fix] â€” 2026-07-08 â€” ðŸ›ðŸ”§ RTL8139 RE bit + AHCI BlockDevice

### Raiz do B-01 (RX=0) encontrada â€” RTL8139 CR_RE bit ausente
- **Bug**: `const CR_RE: u8 = 0x01` (Receiver Enable) nunca era escrito no registrador CR (offset 0x37). O MAC da Realtek ficava desligado â€” pacotes descartados na borda do chip antes do DMA.
- **Log confirmou**: `cr=0x0c` (sÃ³ RXE+TXE), bit 0 (RE) = 0.
- **CorreÃ§Ã£o**: todas as 3 escritas do CR agora usam `CR_RE | CR_RXE | CR_TXE` (0x0D).
- **E1000** funciona porque tem registradores diferentes â€” nÃ£o depende desse bit.
- **Aprendizado**: dumps brutos de registradores na telemetria salvam dias de debug.

### scan_pci_cb() â€” Scanner PCI zero-allocation com callback
- `scan_pci_cb(cb)`: varre 256 buses Ã— 32 slots com Header Type optimization, executa callback `(bus,slot,func,vid,did) â†’ bool`, zero alocaÃ§Ã£o.
- `find_device_by_class(class, subclass)`: busca early-return por class/subclass.
- AHCI probe em `main.rs` refatorado de `scan_pci()` (Vec heap) para `scan_pci_cb()` (zero alloc).

### AHCI + BlockDevice trait â€” IntegraÃ§Ã£o com pipeline FAT32
- `block_dev.rs`: trait `BlockDevice` com `read_sectors(lba, buf)`, implementada para `AtaDriver` e `AhciDriver`.
- `AHCI_DRIVER` global: armazena driver AHCI encontrado.
- Model loading tenta AHCI primeiro, fallback ATA legado.
- QEMU sem disco SATA anexado â†’ `[BOOT] No storage device found` (esperado).

### SkillOpt viability analysis
- Paper Microsoft Research analisado: SkillOpt como optimizer de skills em espaÃ§o textual.
- Viabilidade confirmada para neural-os-core (~145 LOC, sem dependÃªncias externas).
- Recomendado para Sprint 99.

### SGLang Structured Decoding viability analysis
- Paper Stanford/Berkeley analisado: FSM comprimido para geraÃ§Ã£o constraint.
- RadixAttention inviÃ¡vel (memÃ³ria), PrefixCache parcial viÃ¡vel (~80 LOC).
- **Compressed FSM**: viÃ¡vel e alto impacto (~120 LOC) â€” mÃ¡scara logits no BitNet decoder para JSON/SKILL.md/shell.
- Recomendado para Sprint 99 (junto com SkillOpt).

### vLLM PagedAttention viability analysis
- Paper UC Berkeley (SOSP 2023): KV cache paginado com COW entre prefixos.
- Conceito implementÃ¡vel com frame allocator + page table existentes (~100 LOC).
- Ganho marginal para single-user (sem batch de LLM).
- Recomendado para Sprint 100+, apÃ³s SkillOpt + FSM.

### FlashAttention viability analysis
- Paper Stanford (NeurIPS 2022): IO-aware exact attention com tiling no cache L1.
- Aplica-se ao BitNet CPU: blocos de 16 tokens cabem no L1 (32 KB).
- ~3-5Ã— speedup para sequÃªncias >256 tokens.
- Recomendado para Sprint 100+, ~100 LOC em cortex.rs.

## [0.109.1-compilation-fix] â€” 2026-07-08 â€” âœ… 32 erros de compilaÃ§Ã£o eliminados

### CorreÃ§Ã£o em massa â€” cache incremental mascarava 32 erros
- `cargo clean -p neural-kernel` revelou 32 erros que o build incremental escondia por meses
- **Causa raiz**: mÃºltiplos imports faltando (`alloc::vec`, `Vec`, `String`, `ToString`), APIs trocadas (slab, VFS, jarvis), format string nÃ£o escapada
- **shell.rs**: commas faltando, VFS methods inexistentes â†’ `lookup`/`list_dir`, `current_dir` removido
- **cortex.rs**: `{` nÃ£o escapado no `format!`; `Event` â†’ `crate::Event`
- **agents.rs**: `}` extra pÃ³s-match arm; `train_step` esperava `&mut [i8]` nÃ£o `&[i8]`
- **alloc_adapter.rs**: `SlabAllocator::new()` â†’ `::empty()`, `allocate` â†’ `slab_alloc` (retorna `*mut u8`)
- **burn_flex.rs**: `matmul_hybrid` (TernaryTensor) â†’ `matmul` (Tensor); imports faltando
- **trinity.rs / memory_systems.rs**: `.sqrt()` â†’ `libm::sqrtf()` (sem trait F32Ext em no_std)
- **jarvis.rs**: 4 APIs erradas (dream, ego, heartbeat, babel)
- **main.rs**: `AuditTrail::new()` nÃ£o-const â†’ `const fn` c/ `Vec::new()`; AHCI com PCI scan
- **Aprendizado chave**: `cargo clean -p neural-kernel` antes de `cargo check --release` Ã© obrigatÃ³rio quando erros somem misteriosamente

## [0.109.0-sprint91-sound] â€” 2026-07-08 â€” ðŸŽµ Sprint 91 + Sound completos

### Sprint 91 â€” Ecosystem + Polimento ðŸ
- **burn-flex backend**: `FlexBackend::gemm/quantize/pack` com testes unitÃ¡rios
- **MSched VRAM eviction**: Predictor Belady/OPT para working set de VRAM
- **GPU Display Co-existence**: iGPU display + dGPU compute assignment planner
- **SkillManifest macro**: `skill_manifest!()` para declarar manifests estaticamente

### Sprint Sound â€” Ãudio completo ðŸŽ¤ (16 mÃ³dulos, ~2.000 LOC)
- **Intel HDA**: Driver real com PCI probe, GCTL reset, BAR0 mapping
- **USB Audio (UAC)**: Probe de dispositivos UAC via xHCI
- **Pocket TTS 100M**: Engine neural com GPU offload, FAT32/QEMU loader
- **Formant TTS**: Klatt-style sintetizador completo (36 fonemas, IIR resonators)
- **VAD**: Voice Activity Detection com RMS+ZCR e hysteresis
- **SER**: Speech Emotion Recognition (8 emoÃ§Ãµes) com Skill exposure
- **Wake Word**: Detector "jarvis" por energia, cooldown 100 ticks
- **Audio Ring Buffer**: SPSC lock-free PCM (16384 samples)
- **Audio Mixer**: Volume scaling agent com `AUDIO_VOLUME` atomic
- **Audio Context**: Construtor de contexto emocional para LLM injection

## [0.108.0-sprint89] â€” 2026-07-08 â€” ðŸ§  Sprint 89: SleepCycle + MemÃ³ria + BGE

### Sprints 86-89 â€” JARVIS completo
- **Sprint 86** (JARVIS Persona): SOUL.md FAT32, 4 compressÃµes, Notification 4 urgÃªncias, SlabBuddy
- **Sprint 87** (JARVIS Security+AHCI): I1-I4 invariantes, AUDIT_TRAIL global, AHCI instanciado
- **Sprint 88** (JARVIS Emotion+Cache): ADE real, Persona Pipeline 16 stages, edge-dhcp
- **Sprint 89** (SleepCycle+Memory): SleepCycle 5 fases, KG bitemporal, BGE semantic_search

### Pendentes resolvidos
- **#314 SleepCycleAgent**: REPLAYâ†’DREAMâ†’CONSOLIDATEâ†’PRUNEâ†’REFLECT com BitNetTrainer
- **#225 KG Bitemporal**: valid_from/valid_to, tx_from/tx_to, as_of(tick)
- **#359 BGE semantic_search**: `index_embedding()` + cosine similarity
- **#333 burn-flex**: stub com gemm/quantize/pack + testes

## [0.102.0-trinity-learn] â€” 2026-07-08 â€” ðŸ¤– Trinity AutoLearn + SmileyOS Nativo

### Trinity AutoLearn
- **AutoLearnAgent**: Detecta intent nÃ£o classificado 3x â†’ gera necessidade de aprendizado
- **Ciclo completo**: necessidade â†’ FAT32/CVE.BIN â†’ BitNetTrainer â†’ expert registrado
- **`generate_via_model()`**: Reporta intents "generator" via EventBus para AutoLearnAgent

### SmileyOS PadrÃµes Nativos (IDEA #279, sem repo original)
- **Shell 55+ comandos**: +17 comandos (touch, mkdir, top, dmesg, netstat, fetch, etc)
- **Compositor drag/resize/close**: `drag_window()`, `resize_window()`, [X] close button, dock bar
- **WASM Executor**: VM stack-based com 20+ opcodes (Push, Add, Sub, Call, Br, Print, Halt)
- **LLM Icons**: GeraÃ§Ã£o de bitmap 8x8 via HWEXPERT_MODEL com fallback hash

## [0.100.0-ai-regmap] â€” 2026-07-08 â€” ðŸ§© Saltos: RegMap IA + MoE Router + Boot Agent

### Salto 1 â€” HardwareRegisterMap via IA
- **`generate_register_map(vid, did)`**: 3 nÃ­veis de inferÃªncia
  - NÃ­vel 1: mapa direto por HWID (40+ dispositivos conhecidos)
  - NÃ­vel 2: IA classifica famÃ­lia â†’ aplica mapa correspondente
  - NÃ­vel 3: heurÃ­stica por vendor ID â†’ mapa genÃ©rico funcional
- `runtime_probe_and_bind()` usa IA quando nÃ£o acha mapa fixo

### Salto 2 â€” TrinityRouter com pesos treinados (MoE real)
- `router_embed`: tabela VOCABÃ—HIDDEN para embedding de tokens
- `router_weight`: PackedTernaryTensor (HIDDENÃ—NUM_EXPERTS)
- `classify_intent()`: ML â†’ softmax â†’ argmax, fallback keyword se score < 15%

### Salto 3 â€” Boot Agent com IA generativa
- HwDetectAgent reescrito: PCI scan â†’ HWExpert identifica â†’ generate_register_map() â†’ device tree

## [0.99.0-sdio-complete] â€” 2026-07-08 â€” ðŸ’¾ SDIO Pipeline: 45 packs, 95.812 entradas

### SDIO DriverPacks
- **45 packs processados** (18.6 GB) via watcher automÃ¡tico
- **95.812 entradas** JSONL geradas (de 2.794 â†’ 95.812, 34Ã— crescimento)
- ExtraÃ§Ã£o completa: `.inf` + `.sys` + `.cat` + `.dll` + `.txt` + `.html`
- AnÃ¡lise `pefile`: imports IAT por DLL, exports, strings de hardware
- Modelo re-treinado: `hw_expert.bitnet` loss 3.05 â†’ 0.38

### Ferramentas
- `extract_full_hw.py`: extrator completo de TODOS os formatos, watcher automÃ¡tico
- `samdrivers_full.py`: pipeline com --resume/--retrain/--check
- `publish_hf_dataset.py`: sanitiza e publica dataset no HuggingFace
- `update_tecnologias.py`: mantÃ©m barras de progresso do catÃ¡logo automaticamente

## [0.97.0-rustcoder] â€” 2026-07-08 â€” ðŸ¦€ Sprint 97: RustCoder Expert + Trinity MoE

### Sprint 97 â€” RustCoder Expert (~300 LOC, 3 arquivos alterados)
- **Treino**: Expert Rust (hidden=128, 6 layers, 1.6M params) treinado com 41.200 amostras de cÃ³digo Rust na GTX 1050 (loss 0.34)
- **tools/finetune_rust_llm.py**: Script de fine-tuning completo com export bitnet v2
- **tools/rust_coder.bitnet**: Modelo exportado em 444 KB
- **RUSTCODER_MODEL**: Nova static global em cortex.rs â€” `set_rustcoder_model()` + `generate_via_rustcoder()`
- **Fast-path HermesAgent**: Trinity classifica "rust_coder" â†’ geraÃ§Ã£o direta pelo expert sem LLM principal
- **Fallback silencioso**: Se RUSTCDR.BITNET nÃ£o existir na FAT32, usa LLM principal normalmente
- **Boot FAT32**: Kernel carrega RUSTCDR.BITNET da partiÃ§Ã£o FAT32 durante boot
- **build_image.py**: Copia rust_coder.bitnet â†’ RUSTCDR.BITNET na imagem HW
- **Aprendizado chave**: bitnet v2 (packed ternary) Ã© o formato correto para load_model() do kernel

## [0.95.0-cog+v0.96.0-heal] â€” 2026-07-06 â€” ðŸ§ ðŸ›¡ï¸ Sprints 95+96: Cognitive + Self-Heal

### Sprint 95 â€” Cognitive Engine (510+ LOC, 25 structs/funcs)
- **#105 Intent Planner** â€” SkillSteps com params, goal-based plan generation
- **#106 Success Engine** â€” win/loss tracking, streak, recent_rate (64-window)
- **#107 Neural Cache** â€” TTL, LRU evicÃ§Ã£o (max 4096), hit/miss tracking
- **#108 MatMul-Free LM** â€” RWKV-style WKV forward sem multiplicaÃ§Ã£o de matrizes
- **#149 Feedback Loop** â€” rating (0-10) + comment attachment
- **#150 Ternary Weight Update** â€” gradiente â†’ {-1,0,+1} com threshold lr
- **#151 Experience Replay Buffer** â€” ring buffer (10K cap), sample por index
- **#152 Weight Consolidation** â€” snapshot export com metadata
- **#158 Workflow Predictor** â€” confidence scoring por task, top prediction
- **#159 Auto-Skill Generator** â€” WASM templates (echo, hello), generate bytes
- **#160 Dynamic Resource Scaling** â€” heap_target ajustÃ¡vel por pressure
- **#161 Self-Optimizing Scheduler** â€” timeslice dinÃ¢mico baseado em latÃªncia
- **#162 Workflow Profile** â€” JSON export com steps + avg_duration
- **#169 Codebook VQ** â€” nearest-neighbor quantization (256 codes Ã— 64 dim)
- **#170 KV Cache Codebook** â€” compress/decompress KV cache via codebook
- **#171 ReAct Loop** â€” Thought â†’ Action â†’ Observation, max_iter guard
- **#172 MCP Server** â€” tools/list, tools/call, session tracking
- **#173 Codebook Finetune** â€” centroid adjustment via learning rate
- **#174 Delta Branches** â€” speculative decode draft/verify, acceptance rate
- **#175 Workspace Isolation** â€” sandbox heap per agent (BTreeMap alloc)
- **M2 Episodic Memory** â€” ring buffer (max 1000), replay API
- **M37 SleepCycle Guard Rails** â€” blocked words per phase (replay/dream)
- **M38 BitNetTrainer** â€” train_step com ternary_update, loss tracking
- **M39 Candle Trainer sidecar** â€” stub com connect/train/loss
- **M40 Task Spawner** â€” spawn tracking (max 16 children)
- **M41 Three Data Sources** â€” replay_buffer, user_feedback, episodic_memory

### Sprint 96 â€” Self-Healing AvanÃ§ado (~350 LOC em self_heal.rs + vfs + memory)
- **#226-227 Team Memory + Snapshots** â€” agent-shared BTreeMap com versionamento
- **#265-266 Vector FS** â€” VectorFs com dot product search (384-dim)
- **#267 OverlayFS** â€” VfsRegistry::mount_overlay() multi-layer
- **M1 Zero-Copy SFS** â€” slice references, directory index em 256 bytes
- **M3 Skills-as-Modules** â€” fn pointer import + version control
- **M6 Failure Taxonomy** â€” classify_by_code (5 classes + range mapping)
- **M7 Exception Self-Heal** â€” auto recovery via SelfHeal::analyze()
- **M8 Corrective Prompting** â€” context-aware suggestion with escalation
- **M9 Verifier PÃ³s-Recovery** â€” fn check: bool, label reporting
- **M10 Erros no EventLog** â€” format + persist stub
- **M11 Budgeted Recovery** â€” attempts/daemon com max per window
- **M12 Silent Failure Detection** â€” heartbeat + threshold detection
- **M13 Multi-level Failure Assessment** â€” Ok/Warning/Error/Critical
- **M14 Failure Prediction** â€” trend analysis via window diff
- **M29 Notification Gate** â€” allow list por agent + type, block/deliver counters

### Changed
- `cognitive.rs` â€” reescrito de 86 LOC para 510+ LOC com todos os 25+ itens
- `self_heal.rs` â€” Sprint 96 completo: M1-M29, ZeroCopySfs, SkillModule, BudgetedRecovery, SilentFailureDetector, NotificationGate
- `memory_systems.rs` â€” Team memory with snapshot versioning
- `vfs/mod.rs` â€” Vector FS semantic search + OverlayFS mount
- `main.rs` â€” 22 new `lazy_static` instances para cognitive + self-heal modules
- `fs/ata_agent.rs` â€” Fixed pre-existing unreachable match arm bug

## [0.94.0-vision] â€” 2026-07-06 â€” ðŸ‘ï¸ Sprint 94: Vision + Display

### Added
- **#79 Font rendering escalado** â€” `draw_text_scaled()` com scale=1,2,3... para alta resoluÃ§Ã£o
- **#80 Texto em negrito** â€” `draw_text_bold()` com desenho duplicado para destaque
- **#81 VirtIO-GPU** â€” AceleraÃ§Ã£o 2D via VirtIO (QEMU) jÃ¡ funcional desde Sprint 45
- **#82 Tensor visualization** â€” `draw_tensor_heatmap()` + `draw_attention_graph()` no desktop
- **Painel Vision** â€” Attention Map + Token Scores no canto superior direito do desktop

### Changed
- `font.rs` â€” Adicionadas 4 novas funÃ§Ãµes de renderizaÃ§Ã£o
- `compositor.rs` â€” Tensor viz overlay integrado ao desktop JARVIS

### Tested
- QEMU -smp 1 TCG: 0 panics, Desktop 1280Ã—720, Vision panel, 248 agents

## [0.93.0-wasm] â€” 2026-07-06 â€” âš¡ Sprint 93: WASM Runtime + IDE

### Added
- `wasm_rt.rs`: WASM Skill Runtime, MemoryPool (256KB/skill), 15 WASIâ†’Skill mappings, HybridRegistry
- **BitNet IDE** (F4): Gera WASM skills via `[GEN]` â†’ publica como Ã­cone no desktop
- **Ãcones WASM dinÃ¢micos**: Skills aparecem como quadrados no desktop, clicÃ¡veis
- `app_store.rs`: AppForge (install/uninstall/search)
- `multi_user.rs`: Multi-User com trust tiers
- `workflow.rs`: Workflow Builder (DAG) + Federated Cluster
- `hub.rs`: Observability + Hub Discovery
- `elf_loader.rs`: Cross-OS loaders (ELF/PE/Mach-O/APK)
- Compositor: suporte a AppId::Ide, AppId::WasmSkill, Ã­cones dinÃ¢micos

### Tested
- QEMU -smp 2 WHPX: 0 panics, Desktop 1280Ã—720, 248 agents

## [0.92.0-lan] â€” 2026-07-06 â€” ðŸŒ Sprint 92: LAN + DependÃªncias

### Added
- B-01/#117-120: Network stack (smoltcp DHCP/ARP, /ping)
- #186-189: AppForge, Multi-User, Workflow, Federated
- #241-247: Observability, Hub, HITL, Marketplace, Compaction
- #306a-d: ELF/PE/Mach-O/APK loaders
- M4-M5: Syscall Categories, Neural Cache

## [0.91.0-ui] â€” 2026-07-06 â€” ðŸ–¥ï¸ Sprint 91: JARVIS Desktop UI

### Added
- **JarvisDesktop** â€” Compositor multi-window com status bar + app switcher
- **Hermes Chat App** â€” Janela de chat com histÃ³rico de comandos
- **Settings App** â€” ConfiguraÃ§Ãµes: tema, voz, memÃ³ria, avatar, rede
- **Power App** â€” Shutdown, Reboot, Hibernate, Sleep
- **JARVIS avatar overlay** â€” Canto inferior direito com pulso animado
- **`display/compositor.rs`** â€” Reescrito: `JarvisDesktop` + `draw_text()` + `render_app_content()`

### Changed
- `display/agent.rs` â€” DisplayAgent agora gerencia Desktop + apps + avatar
- `compositor.rs` â€” Substitui wrapper NeuralConsole por JarvisDesktop completo

### Tested
- QEMU -smp 2 WHPX: 0 panics, Desktop 1280Ã—720, 248 agents

## [0.90.0-cognitive] â€” 2026-07-06 â€” ðŸ§  Sprint 90: JARVIS Deep Cognitive

### Added
- **#315.12 Dreaming/Consolidation** â€” `DreamEngine`: insights sintÃ©ticos, agrupamento por tÃ³pico
- **#315.13 Ego Layer** â€” `EgoLayer`: confidence tracking por domÃ­nio, `can_answer()`
- **#315.14 Proactive Heartbeats** â€” `Heartbeat`: JARVIS alerta proativamente (disk, mem, net)
- **#315.15 Tool-State Save Game** â€” `ToolState`: snapshot + rollback de ferramentas
- **#315.16 Auto-Skill Generation** â€” `AutoSkillGen`: gera skill ao detectar padrÃ£o â‰¥3 repetiÃ§Ãµes
- **#315.17 Babel-Index** â€” `BabelIndex`: monitora entropia, contradictions, staleness

### Tested
- QEMU -smp 2 WHPX: 0 panics, 248 agents, JARVIS cognitive engine OK

## [0.89.0-memory] â€” 2026-07-06 â€” ðŸ§  Sprint 89: SleepCycle + Advanced Memory + BGE

### Added
- **#314 SleepCycle Agent** â€” 5 fases: REPLAYâ†’DREAMâ†’CONSOLIDATEâ†’PRUNEâ†’REFLECT, agendado por tick
- **#214 SHA-256 Memory Dedup** â€” Sliding window 5min, SHA-256 hash check
- **#215 Privacy Filter** â€” Strip API keys, secrets, tokens antes de armazenar
- **#216 Memory TTL/Eviction** â€” Auto-evict por TTL + importÃ¢ncia, fallback LRU
- **#219 Ebbinghaus Decay** â€” `strength = importance Ã— e^(-Î»Â·days) Ã— (1 + recall_count Ã— 0.2)`
- **#217 Hybrid Search (BM25 + MLP)** â€” BM25 score com RRF fusion, avg_len normalizado
- **#218 4-Tier Memory Consolidation** â€” Workingâ†’Episodicâ†’Semanticâ†’Procedural pipeline
- **#222 Metacognitive Guard** â€” Verifica erros passados antes de executar skill
- **#223 Draftâ†’Reviewâ†’Merge Memory** â€” Workflow de aprovaÃ§Ã£o de memÃ³ria
- **#224 Atkinson-Shiffrin 3-tier** â€” Sensory Register (48h) â†’ STM (7d) â†’ LTM (permanent)
- **#225 Bi-temporal Knowledge Graph** â€” Triplas (sujeito, predicado, objeto) com validade temporal
- **#359 BGE-Small-EN-v1.5** â€” Embedding stub 384-dim (ONNXâ†’.bitnet pendente)

### New File
- `memory_systems.rs` â€” Todos os 12 itens em um mÃ³dulo coeso (~470 LOC)

### Tested
- QEMU -smp 2 WHPX: 0 panics, 248 agents, JARVIS avatar OK

## [0.88.0-emotion] â€” 2026-07-06 â€” ðŸŽ­ Sprint 88: JARVIS Emotion + Cache + Pipeline + DHCP

### Added
- **#315.6 Emotion Analysis** â€” `EmotionAnalysis` com 7 emoÃ§Ãµes + sarcasmo, anÃ¡lise por palavra-chave
- **#315.7 Capability Contract + Consent Gates** â€” `ConsentGate` com 3 nÃ­veis (Safe/Moderate/Dangerous)
- **#315.8 Skill Discovery** â€” `SkillDiscovery` â€” observa padrÃµes de tarefa, propÃµe skills em â‰¥3 repetiÃ§Ãµes
- **#315.9 ADE Pipeline** â€” `ade_pipeline()` â€” 4 fases: Specâ†’Executeâ†’Reviewâ†’Recover
- **#315.10 Semantic Cache** â€” `SemanticCache` â€” 5 tiers (exactâ†’patternâ†’fallback), hit/miss tracking
- **#315.11 Persona Pipeline** â€” `persona_pipeline()` â€” 16 stages da OVOS
- **#356 edge-dhcp integration** â€” `dhcp.rs` â€” ponte para crate edge-dhcp (no_std + no-alloc DHCP)

### Changed
- `jarvis.rs` unificado com todos os 16+ componentes da Sprint 86-88

### Tested
- QEMU -smp 2 WHPX: 0 panics, 214 agency agents, JARVIS avatar OK

## [0.87.0-security] â€” 2026-07-06 â€” ðŸ›¡ï¸ Sprint 87: JARVIS Security + AHCI

### Added
- **#315.18 Fail-Closed Safety Invariant** â€” `safety.rs`: `SafetyInvariants` com 4 invariantes SMT-proof (I1-I4). PadrÃ£o Ã© negar.
- **#315.19 Merkle Audit Trail** â€” `audit.rs`: `AuditTrail` com SHA-256 chain, ring 4096, verificaÃ§Ã£o de integridade.
- **#315.20 Fluid Persona** â€” `jarvis.rs`: `SoulProfile::fluid_update()` adapta tom por emoÃ§Ã£o/urgÃªncia. 3 modos (Coach/Tutor/Tool).
- **AHCI driver** â€” `ahci.rs`: Driver SATA 6G NCQ via MMIO. Suporta ATAPI, PRDT, DMA READ/WRITE. PCI class 0x01/0x06.

### Changed
- `tpm.rs`: `sha256()` agora Ã© `pub` (usado pelo audit trail)

### Tested
- QEMU -smp 2 WHPX: 0 panics. SafetyAgent registrado, Hermes Chat OK.

## [0.86.3-persona] â€” 2026-07-06 â€” ðŸ§‘ Sprint 86: JARVIS Persona + Alloc Adapter

### Added
- **#315.1 SOUL.md** â€” `SoulProfile` com name/tone/humor/formality/empathy, parser markdown
- **#315.2 IPW Monitor** â€” `IpwMonitor` lÃª RAPL MSR 0x610 (PKG_ENERGY_STATUS), calcula tokens/watt
- **#315.3 Session Compression** â€” `SessionHistory` com 4 estratÃ©gias (summarize/drop_lowest/merge_similar/segment_means)
- **#315.4 Notification Gate** â€” `NotificationGate` com 4 urgency levels, dedup, rate limit
- **#315.5 Sessionless Thread** â€” `SessionlessThread` conversa contÃ­nua sem reset, stale detection
- **#355 Alloc Adapter** â€” `alloc_adapter.rs` ponte para buddy-slab-allocator (feature opcional)
- **`jarvis.rs`** â€” Engine unificada integra todos os 5 componentes + tick loop

### Tested
- QEMU -smp 2 WHPX: 0 panics, JARVIS avatar + Hermes Chat OK

## [0.86.2-embedding] â€” 2026-07-06 â€” ðŸ§  ADR-0038 v2: BGE Embedding + Kokoro TTS

### Added
- **ADR-0038 v2** â€” SeÃ§Ã£o 5: Modelos de Embedding e TTS. BGE-Small-EN-v1.5 (Sprint 89) e Kokoro-82M (Sprint 92+).
- **IDEA_BANK:** #359 (BGE-Small-EN-v1.5), #360 (Kokoro-82M TTS)
- **Sprint 89 expandido:** +300 LOC para BGE embedding â†’ busca semÃ¢ntica real no Hermes
- **Sprint 92+ expandido:** Kokoro-82M substitui Piper como TTS padrÃ£o (82M params vs 300M+)

### Viability
| Modelo | Params | LicenÃ§a | Tamanho | Uso | Sprint |
|--------|--------|---------|---------|-----|--------|
| BGE-Small-EN-v1.5 | 33.4M | MIT | 33 MB | Embedding semÃ¢ntico | 89 |
| Kokoro-82M-ONNX | 82M | Apache-2.0 | 86 MB Q8 | TTS | 92+ |

## [0.86.1-ecosystem] â€” 2026-07-06 â€” ðŸ“Š ADR-0038: OtimizaÃ§Ã£o do Ecossistema (Hugging Bay + crates.io)

### Added
- **ADR-0038** â€” `docs/architecture/0038-ecosystem-optimization.md`: DecisÃµes de substituiÃ§Ã£o baseadas em pesquisa Hugging Bay + crates.io.
- **IDEA_BANK:** #355 (buddy-slab-allocator), #356 (edge-dhcp), #357 (khal-std), #358 (ruvix-net)
- **`tools/huggingbay_search.py`** â€” Busca no Hugging Bay por artefatos AI.
- **`tools/huggingbay_item.py`** â€” Detalhes de artefato por ID.

### Changed
- **`docs/sprint-plan-84-95.md`** â€” Sprint 86 expandido (+buddy-slab-allocator), Sprint 88 expandido (+edge-dhcp).
- **`docs/memory/IDEA_BANK.md`** â€” +4 ideias (#355-#358), total 358.

### Decisions (ADR-0038)
| Tecnologia | AÃ§Ã£o | Sprint | Motivo |
|---|---|---|---|
| buddy-slab-allocator | Substituir slab.rs + vram.rs backend | 86 | 30K downloads, no_std, per-CPU slab, ArceOS |
| edge-dhcp (edge-net) | Fallback DHCP p/ B-01 | 88 | no_std + no-alloc, 225â˜… GitHub |
| khal-std | âŒ InviÃ¡vel (requer wgpu/std) | â€” | InspiraÃ§Ã£o arquitetural apenas |
| ruvix-net | ðŸ”µ ReferÃªncia | â€” | Kernel cognitivo similar |

## [0.86.0-jarvis] â€” 2026-07-06 â€” ðŸ† JARVIS Avatar + CogniÃ§Ã£o (port do .NET MAUI)

### Added
- **`display/avatar.rs`** â€” JARVIS Avatar com partÃ­culas animadas, 4 estados (Idle/Listening/Processing/Speaking), port do `AvatarDrawable.cs` do .NET MAUI. Renderiza sobre framebuffer via `DoubleBuffer::set_pixel()`.
- **`jarvis.rs`** â€” JARVIS Engine unificada: personalidade (`JarvisPersonality`), anÃ¡lise emocional (`detect_emotion` com 7 emoÃ§Ãµes + sarcasmo), memÃ³ria contextual (`JarvisMemory` com ring buffer 256), avatar state machine. Port dos conceitos `TextProcessor`, `EmotionalAnalysisService`, `VectorStorageService`, `UserProfile` do .NET MAUI.
- **`display/agent.rs`** â€” DisplayAgent integra JARVIS avatar + engine + Hermes Chat Console.

### Arquitetura JARVIS (port .NET MAUI â†’ bare-metal)
| Conceito .NET MAUI | Equivalente Rust | Arquivo |
|---|---|---|
| AvatarDrawable (SkiaSharp) | JarvisAvatar + Particle | `display/avatar.rs` |
| EmotionalAnalysisService | `detect_emotion()` (BitNet fallback) | `jarvis.rs` |
| VectorStorageService + SQLite | JarvisMemory (ring buffer) | `jarvis.rs` |
| UserProfile | JarvisPersonality (aprendizado contÃ­nuo) | `jarvis.rs` |
| Semantic Kernel | Hermes Cognitive + ReAct | (existente) |
| MainPage (Avatar+Chat) | DisplayAgent + NeuralConsole | `display/agent.rs` |
| VoiceService | Piper + Vosk (pÃ³s B-01) | (futuro) |

## [0.85.0-design] â€” 2026-07-06 â€” ðŸ† Sprint 85: GPU Decode (XPU split + DMA + XQueue)

### Added
- **`gpu/xpu.rs`** â€” Agent.xpu prefill/decode split (#329, ~90 LOC): CPU prefill via forward_with_kv, GPU decode stub, generate() com timing. ReferÃªncia arXiv 2506.24045.
- **`gpu/kv_dma.rs`** â€” CPUâ†’GPU KV cache DMA (#331, ~90 LOC): KvDmaTransfer, kv_transfer_layer(). Copia KV cache entre RAM e VRAM com sfence. ReferÃªncia dmaplane.
- **`gpu/xqueue.rs`** â€” XQueue preemptÃ­vel 3 nÃ­veis (#332, ~125 LOC): pending/in-flight/running com timeout. Preempt rebaixa in-flight para pending. ReferÃªncia XSched (OSDI 2025).

### Changed
- **`gpu/mod.rs`** â€” Adicionado `pub mod xpu`, `pub mod kv_dma`, `pub mod xqueue`.

### Tested
- QEMU (-smp 2, WHPX): 0 panics, 0 errors. GPU-BACKEND, SECURE-BOOT, Hermes Chat OK.
- VirtualBox (1 CPU, VirtIO-net): 0 panics, 0 errors. Hermes Chat OK.

### Sprint 85 Total: ~305 LOC (4 itens, est. 1500 LOC â€” stubs para quando GPU compute estiver pronto)

## [0.84.1-gpu] â€” 2026-07-06 â€” ðŸ† Sprint 84: GPU Foundations (BAR mapping + Job Ring + VRAM Buddy + Secure Boot)

### Added
- **`gpu/ring.rs`** â€” SPSC job ring genÃ©rico para 3 vendors (Intel RENDER_RING_TAIL, NVIDIA PFIFO, AMD PM4). Doorbell, push, poll, submit_and_wait. Ring buffer em pÃ¡ginas UC.
- **`gpu/firmware.rs`** â€” Secure boot GPU: NVIDIA ACR, AMD PSP, Intel GuC. Pipeline: linux-firmware â†’ kernel â†’ BAR0 â†’ GPU engine. Blobs stub (firmware disponÃ­vel em linux-firmware, loading futuro).
- **`gpu/vram.rs`** â€” Upgrade para buddy allocator power-of-2 (4KB a 4GB). Splitting/merging de blocos. Substitui first-fit BTreeMap.

### Changed
- **`gpu/backend.rs`** â€” `init_backend()` agora: (1) mapeia BARs UC, (2) valida BAR0, (3) cria SPSC job ring, (4) secure boot, (5) vendor init.
- **`gpu/mod.rs`** â€” Adicionado `pub mod ring` e `pub mod firmware`.
- **`memory_agent.rs`** â€” `VRAM_STATE` â†’ `VRAM_BUDDY` (novo allocator).

### Tested
- QEMU (-smp 2, WHPX): boot OK, GPU-BAR mapping, SECURE-BOOT, CPU fallback. âœ…
- VirtualBox (1 CPU, VirtIO-net): boot OK, Hermes Chat, GPU-BACKEND. âœ…
- 0 erros, 446 warnings (dead code esperados).

## [0.84.0-design] â€” 2026-07-05 â€” ðŸ“š DocumentaÃ§Ã£o Reestruturada: HW Real First + Multi-Vendor + Sprint Plan 84-95

### Added
- **`docs/sprint-plan-84-95.md`** â€” Plano mestre de 9 sprints (84-95). Todos os 354+ items do IDEA_BANK assignados a sprints/blocos. HW Real, multi-vendor GPU/NVIDIA/AMD/Intel, busca ativa na internet para bloqueios.
- **`docs/memory/SESSION_INDEX.md`** â€” CatÃ¡logo de 43 sessÃµes com tÃ­tulos, sprints, descobertas. SeÃ§Ã£o "LiÃ§Ãµes CrÃ­ticas (NÃƒO REPETIR)" com 10 dead-ends documentados.
- **`docs/TODO.md`** â€” Reescrito como checklist multissprint. Cada sprint com checkboxes, goals, sub-itens, dificuldades, dependÃªncias, fontes. Status flags: âœ… ðŸŸ¡ â³ ðŸ”´ ðŸ’° âŒ.

### Changed (docs)
- **`docs/memory/STATE.md`** â€” Roadmap expandido para 84-95 (9 sprints). SeÃ§Ã£o "NavegaÃ§Ã£o RÃ¡pida para AI DEVs". Pendentes por sprint com #ID do IDEA_BANK.
- **`docs/memory/IDEA_BANK.md`** â€” 9 items orphan atualizados com sprints especÃ­ficos. SeÃ§Ã£o 6 expandida: Bloco 28 desmembrado em 21c+21d.
- **`docs/roadmap.md`** â€” Multi-vendor GPU/NPU, firmware ACR/PSP/GuC, QEMU loader removido, bloqueios com busca na internet.
- **`docs/architecture/0037-smp-gpu-architecture.md`** â€” GTX 1050â†’genÃ©rico NVIDIA/AMD/Intel. QEMU/VBoxâ†’HW Real.
- **`docs/architecture/0029-gpu-architecture.md`** â€” Tabela HW expandida, firmware multi-vendor, hardware layer genÃ©rico.
- **`docs/architecture/0016-network-strategy.md`** â€” RTL8139 dev + e1000/r8169 HW real + busca por NIC.
- **`docs/architecture/0001-initial-architecture-and-toolchain.md`** â€” QEMUâ†’dev/debug.

### Changed (root)
- **`AGENTS.md`** â€” HW Real First (princÃ­pio #4). Busca ativa na internet para bloqueios. NavegaÃ§Ã£o rÃ¡pida AI-first. ~200 linhas de sessÃµes histÃ³ricas inline removidas (apontam para SESSION_*.md). MemPalace integration. Sprint: 84.

## [0.81.0] â€” 2026-07-05 â€” ðŸ† Sprint 81: SMP Foundation + GPU Improvements

### Added (neural-kernel)
- **`smp/spsc.rs`** â€” SPSC (Single Producer Single Consumer) queue lock-free para comunicaÃ§Ã£o entre cores. Baseado em MPMC de Dmitry Vyukov, simplificado para SPSC. Capacidade potÃªncia de 2, atomic head/tail.
- **`smp/mod.rs`** â€” Adiciona mÃ³dulo `spsc` ao SMP.
- **`interrupts.rs`** â€” IPI handlers para SMP: `ipi_reschedule_handler` (vetor 0x80), `ipi_halt_handler` (vetor 0x81), `ipi_call_function_handler` (vetor 0x82). Contadores globais `IPI_RESCHEDULE`, `IPI_HALT`, `IPI_CALL_FUNCTION`.
- **`apic.rs`** â€” FunÃ§Ãµes de envio de IPI: `send_ipi_reschedule()`, `send_ipi_halt()`, `send_ipi_call_function()`. CompatÃ­vel xAPIC/x2APIC. Shorthand=all_excl_self (0x180000).
- **`gpu/intel.rs`** â€” Infraestrutura para Intel GEN shader assembly: constantes `MEDIA_OBJECT`, `PIPELINE_SELECT`, `STATE_BASE_ADDRESS`. Campos `shader_pa` e `shader_loaded` em `IntelRing`. FunÃ§Ãµes `load_gen_matmul_shader()` e `execute_gen_shader()` (stubs preparados para shader real NDA Intel).
- **`gpu/backend.rs`** â€” Separa BCS Blitter do RCS ring: `GpuAccel::Intel(IntelRing, Option<BcsRing>)`. `init_backend()` inicializa BCS ring se disponÃ­vel. `gpu_status()` reflete estado RCS+BCS. `gpu_matmul()` usa `as_mut()` para ring mutÃ¡vel.

### Changed
- **`gpu/backend.rs`** â€” `gpu_matmul()` agora usa `as_mut()` para permitir mutaÃ§Ã£o do ring durante matmul.

### Tasks Completadas
- B-05: Integrar GPU no boot (jÃ¡ existia em main.rs)
- B-07: Implementar GTT setup para Intel GPU (jÃ¡ existia em intel.rs)
- B-02: Implementar Intel GEN shader assembly para matmul (infraestrutura + stub)
- B-08: Separar BCS Blitter do RCS ring
- B-09: Implementar VRAM Free List (jÃ¡ existia em vram.rs)
- B-10: Implementar driver e1000/r8169 para NIC real (e1000 jÃ¡ existia)
- B-14: Implementar WASM Sandbox (jÃ¡ existia em wasm.rs)
- B-15: Implementar GGUF Model Swap (jÃ¡ existia em gguf.rs)
- Bloco 21a: SMP Foundation (SPSC + IPI + PerCpu)

### Notes
- GEN assembly Ã© NDA da Intel. `load_gen_matmul_shader()` aloca 1 pÃ¡gina e escreve NOOPs como stub. Shader real requer engenharia reversa do i915 driver ou assembler externo.
- IPI handlers configurados nos vetores 0x80-0x82. PerCpu jÃ¡ existia com GS.base e `cpu_id()`.
- SPSC queue usa atomic head/tail com memory ordering Acquire/Release.

## [0.80.0] â€” 2026-07-05 â€” ðŸ† Sprint 80: AVX2 Debug + WHPX Detection + Forward Pass

### Added (neural-kernel)
- **`bitnet_avx2.rs`** â€” `unpack_row_into()`: descompacta 1 linha de PackedTernaryTensor em buffer i8 reutilizÃ¡vel (n bytes em vez de k*n). WHPX detection via CPUID leaf 0x40000000: vendor "Microsoft Hv" â†’ `avx2_available()` retorna false. `avx2_ternary_matmul_impl()` reescrito: row buffer + acumulaÃ§Ã£o direta.
- **`tensor.rs`** â€” `has_avx2()` com WHPX detection (hypervisor bit + vendor string check).
- **`cortex.rs`** â€” Per-layer timing: `[FWD] L0 qkv:... attn:... proj:... ffn_gateup:... down:... total:...`. Unembed timing. `generate_speculative()` limitado a 8 tokens (antes 64).
- **`agents.rs`** â€” Timing em `generate_via_model()`: `[CORTEX-LLM] generate_via_model took X ticks (~Ys)`.

### Fixed
- **AVX2 `matmul_hybrid` para TernaryTensor (Q,K,V,O):** era scalar puro â€” agora usa AVX2 dispatch quando disponÃ­vel (`matmul_hybrid_avx2`).
- **Tail handling AVX2:** K/V tÃªm n=100 (nÃ£o mÃºltiplo de 8). `matmul_hybrid_avx2` e `matmul_avx2_inner` processam blocos de 8 com AVX2 e colunas restantes com scalar.
- **`avx2_ternary_matmul_impl`:** Revertido broadcast-per-t (correto para matmul) â€” outer product com `step_by(8)` estava incorreto.
- **Removido gate `m >= 4`:** tokens Ãºnicos (m=1) agora usam AVX2 via `tensor.matmul()`.
- **`unpack_all()` removido:** substituÃ­do por `unpack_row_into()` que aloca apenas n bytes (6.9 KB) em vez de k*n bytes (17.7 MB) por matmul.

### Performance (WHPX, 2.4B model, seq_len=64)

| Modo | ticks/layer | tempo/layer | 30 layers |
|---|---|---|---|
| AVX2 (VEX emulado) | ~4443 | ~4.4s | ~132s |
| **Scalar puro** | **~2218** | **~2.2s** | **~66s** |

### Lessons
- **WHPX + AVX2 = pior performance:** WHPX emula cada VEX instruction como VM exit. Scalar GP instructions rodam nativos. AVX2 sob WHPX Ã© 2x MAIS LENTO que scalar.
- **`has_avx2()` detection:** CPUID leaf 0x40000000 vendor string "Microsoft Hv" identifica WHPX. Hypervisor presente check (CPUID leaf 1, ECX bit 31) como prÃ©-requisito.
- **`unpack_all` nÃ£o era o gargalo:** 17.7 MB allocation Ã© barata comparada a 17M operaÃ§Ãµes de bit unpacking + matmul.
- **Forward pass BitNet b1.58 sob WHPX:** ~60s para 64 tokens Ã— 30 layers. InviÃ¡vel para autogeneraÃ§Ã£o sem KV cache ou bare metal.

### Known Issues
- Forward pass BitNet b1.58 sob WHPX: ~2.2s/layer = ~60s/forward pass. Generate 8 tokens: ~6h.
- SoluÃ§Ã£o: KV cache + bare metal ou QEMU+KVM.

## [0.79.2] â€” 2026-07-05 â€” ðŸ› Xuvisco v2: VGA Sequencer Screen Off (0x3C4/0x3C5)

### Fixed (neural-kernel)
- **`vga_buffer.rs`** â€” `clear_physical_buffer()` substituÃ­da por `disable_vga_plane()` que usa o sequenciador VGA (porta 0x3C4/0x3C5) para setar bit 5 (Screen Off) do Clocking Mode Register. NÃ£o acessa CRTC (0x3D4/0x3D5) nem memÃ³ria 0xB8000.
- **`main.rs`** â€” Chama `vga_buffer::disable_vga_plane()` em vez de `clear_physical_buffer()`.

### Root Cause (v0.79.1 regression)
`clear_physical_buffer()` escrevia em 0xB8000 via `write_bytes`, mas o bootloader (UEFI/OVMF) nÃ£o mapeia o legacy VGA hole 0xA0000-0xBFFFF no memory map. Escrever em 0xB8000 causa page fault antes da IDT ser inicializada (main.rs linha 454) â†’ triple fault â†’ reset â†’ xuvisco.

### Lesson
"VGA text buffer" nÃ£o estÃ¡ magicamente mapeado em todo hardware. UEFI/OVMF nÃ£o inclui a VGA hole no mapa de pÃ¡ginas. I/O ports (0x3C4/0x3C5) sÃ£o a Ãºnica forma segura de desligar o VGA plane antes da IDT.

## [0.79.1] â€” 2026-07-05 â€” ðŸ› Display Xuvisco Fix (VGA buffer + framebuffer clear)

### Fixed (neural-kernel)
- **`display/fb.rs`** â€” Framebuffer Ã© limpo para preto imediatamente apÃ³s `probe_uefi_framebuffer()`, eliminando artefatos do bootloader na tela.
- **`vga_buffer.rs`** â€” Nova funÃ§Ã£o `clear_physical_buffer()` que limpa 0xB8000 (4000 bytes) via `write_bytes` sem acessar registros CRTC (0x3D4/0x3D5). Segura para Intel 6xx com UEFI GOP.
- **`main.rs`** â€” `vga_buffer::clear_physical_buffer(pm_offset)` chamado quando framebuffer presente, antes de qualquer mensagem de boot.

### Root Cause
`[BOOT] FB ativo â€” VGA text mode desligado` nunca executava VGA disable real: `hide_cursor()` e `clear_vga_buffer()` estavam definidos mas nunca chamados (orfÃ£os desde Sprint 71). VGA text overlay e framebuffer sujo coexistiam, causando xuvisco em QEMU e hardware real.

## [0.79.0] â€” 2026-07-04 â€” ðŸ† Sprint 79: LLM Infrastructure (BitNet-b1.58 Integration)

### Added (neural-kernel)
- **`bitnet_avx2.rs`** â€” AVX2 ternary matmul kernel (`ternary_matmul()`) with scalar fallback. Unpacks 2-bit packed ternary â†’ `_mm256_cvtepi8_epi32` â†’ `_mm256_cvtepi32_ps` â†’ FMA. Called by `PackedTernaryTensor::matmul_hybrid()`.
- **`trinity.rs`** â€” `TrinityRouter` MoE stub. `register_expert()` adds named experts; `classify_intent()` rule-based dispatch across 5 classes (code/hw/chat/file/system). Real ML router deferred.
- **`bpe.rs`** â€” `BpeTokenizer` with JSON parser for HuggingFace `tokenizer.json`. `encode()`/`decode()`/`init_from_json()` global functions. Subword tokenization with BPE merge rules.

### Changed (neural-kernel)
- **`cortex.rs`** â€” `vocab_size` migrated from `u16` to `u32` (supports vocab 128K). `load_model()` v2: initializes BPE tokenizer automatically via `bpe::init_from_json()`. `TransformerModel` with dynamic `hidden`, `num_layers`, `max_seq`, `vocab_size: u32`. `LayerWeights.rms_attn/rms_ffn` as `Vec<f32>` (vectorial RMSNorm). `generate_speculative()` uses `model.max_seq` and BPE when loaded.
- **`gguf.rs`** â€” `vocab_size()` returns `u32`. Field `vocab_size` as `u32` in constructor.
- **`tensor.rs`** â€” Removed inline scalar matmul fallback; `matmul_hybrid()` delegates exclusively to `bitnet_avx2::ternary_matmul()`.
- **`main.rs`** â€” `mod bitnet_avx2`, `mod trinity`. Ramdisk loading section: checks `boot_info.ramdisk_addr` for bootloader ramdisk. QEMU loader fallback: probes physical address `0x100000000` (4GB) for `.bitnet` magic. Maps up to 1.5GB and calls `load_model()` if found.

### Changed (tools)
- **`download_bitnet.py`** â€” Header `.bitnet` v2 fixed: `vocab_size` as u32 (u16 overflowed at 128K). `ffn_dim` field added. `tok_type`/`tok_len` for BPE tokenizer embedding. BPE `tokenizer.json` extracted alongside `.bitnet`.
- **`build_image.py`** â€” Simplified: removed LBA append logic (provisional). Changed bootloader dependency to `default-features=false, features=["bios"]` to avoid UEFI compile error.

### Model
- Downloaded `microsoft/BitNet-b1.58-2B-4T` (real: 850M params). Converted to `.bitnet` v2: 1,464 MB, magic `0xBE11BE11`. Architecture: hidden=2560, layers=30, heads=20 (GQA=5 KV), vocab=128256, intermediate=6912.
- `micro.bitnet` (71KB) synthetic model preserved as fallback.

### Fixed
- `#[allow(dead_code)]` policy confirmed for production code (399 expected warnings)
- `mod shell` remains dead with `@dead` annotation (prevents accidental revival)

### Known Issues
- Forward pass broken for BitNet b1.58: GQA (20â†’5 KV heads) + BitFFN grouped down_proj (640â†’6912) not supported by standard FFN path. Sprint 80 needed.
- QEMU loader requires 6GB RAM + WHPX. 2GB fails (model at 512MB conflicts with boot allocator).
- Ramdisk via bootloader impossible for 1.46GB (FAT partition ~64MB). QEMU loader at 4GB is workaround.

## [0.78.1] â€” 2026-07-04 â€” ðŸ§¹ Code Review: Dead Modules Audit

### Added
- `#![allow(dead_code)]` + `@dead` annotations em 8 mÃ³dulos mortos (shell, voice_skill, bench, verify, orchestrator, tracer, skill_market, hal) â€” cada um documentado com motivo e sprint futuro alvo
- SeÃ§Ã£o "DEAD MODULES" em `main.rs` com tabela de referÃªncia para IA devs

### Changed
- 36 warnings eliminados (426â†’390) nos 8 mÃ³dulos anotados
- PolÃ­tica confirmada: `#[allow(dead_code)]` sÃ³ em mÃ³dulos mortos conhecidos; cÃ³digo em desenvolvimento mantÃ©m warnings como esperado

## [0.78.0] â€” 2026-07-04 â€” ðŸ† Sprint 78: Agentic Evolution

### Added (neural-kernel)
- **IntentCache wiring** (`agents.rs`): HermesAgent now instantiates `IntentCache`, checks cache before `parse_command()`, and caches results. LRU with 64-entry limit.
- **OutputCache wiring** (`agents.rs`): `execute_skill()` checks `OutputCache` before calling `SkillRegistry::execute_skill_unchecked()`. Skills marked `idempotent: true` have outputs cached.
- **WorkflowEngine wiring** (`agents.rs`): HermesAgent field + tick loop checks `is_active()` and advances phases. Started for Chatâ†’LLM commands and advanced on LLM response.
- **SelfCritique** (`hermes.rs`): `SelfCritique::evaluate()` e `SelfCritique::check_command()` â€” verifica output vazio, erros, placeholders, respostas curtas.
- **GgufBackedModel** (`gguf.rs`): Implementa `cortex::Model` trait. Converte pesos GGUF (FP32/Q4_0) para `TransformerModel` via `try_build_transformer()`. Suporta busca de tensores por nome (blk.N.attn_q, ffn_gate etc.).
- **FsBridgeAgent** (`agents.rs`): Agente `PollEvery(500)` que escaneia `MHI_REGISTRY` por alocaÃ§Ãµes candidatas Ã  promoÃ§Ã£o (access_count > 5, idle < 500) e executa migraÃ§Ã£o HDDâ†’DRAM via VFS.
- **WasmExecutor** (`wasm.rs`): Interpretador stack-based com suporte a i32.const/add/sub/mul/eqz/load/store, block/loop/if/else/end, br/br_if, call, select, memory.size/grow. 35+ opcodes WASM.
- **WasmSkill** (`wasm.rs`): Implementa `Skill` trait. `verify()` parseia bytecode, `execute()` carrega e executa funÃ§Ã£o exportada (main/_start). WASI stub para bridge futura.
- **register_wasm_skill()** (`wasm.rs`): Registra uma skill WASM no SkillRegistry a partir de bytecode.

### Added (agent-core)
- **AgentTier** (`lib.rs`): Enum com `Permanent/System/User/Periodic/Learning`, cada um com `priority()`.
- **AgentInstance.tier**: Campo `tier: AgentTier` default `Permanent`.
- **migrate_to_tier()**: `AgentRegistry::migrate_to_tier(idx, new_tier)` e `migrate_to_tier_by_name(name, new_tier)`.
- **agents_by_tier()**: Filtra agentes por tier.

### Fixed
- **execute_skill** borrow fix: MudanÃ§a de `&self` para `&mut self` para permitir cache writes. Clona skill_names antes de chamar.

### Sources
- Sprint 78 plan (8 items: Flow/Crew, Cache, Workflow, StateGraph, Tier, MHI-FS, GGUF, WASM)
- v0.72.0 base (Crew, FlowTrigger, StateGraph, IntentCache, OutputCache, WorkflowEngine, GGUF parser)

---

## [0.77.0] â€” 2026-07-04 â€” ðŸ† Sprint 77: Foundation Quick Wins

### Added (skill-registry)
- **Skill::verify()** (`skill.rs`): Pre-flight verification trait method. Skills podem checar precondiÃ§Ãµes antes de executar. `SystemStatusSkill` verifica MHI, `HardwareInfoSkill` verifica SystemArchitecture.
- **CompletionContract** (`contract.rs`): `CONTRACT_NONEMPTY` e `CONTRACT_UTF8` com validaÃ§Ã£o pÃ³s-execuÃ§Ã£o. Suporta `WarnOnly`, `RejectOutput`, `RetrySkill`.
- **TaskSchema** (`task.rs`): `TaskSchema`, `JobPreconditions`, `TaskStatus` â€” tipos para schema de tarefas estruturadas com precondiÃ§Ãµes, timeout, retries.
- **DynamicSkill** (`dynskill.rs`): Skill registrÃ¡vel em runtime via `/learn`. Implementa `Skill` trait diretamente, sem LLM.
- **FanOutPool** (`fanout.rs`): Pool de sub-tarefas assÃ­ncronas. `spawn()`/`poll_all()`/`take_result()`. Sub-tasks como `Box<dyn FnOnce + Send>`.
- **SkillIndex.find()** (`index.rs`): Busca textual por nome/desc/capabilities.
- **McpCatalog** (`index.rs`): CatÃ¡logo pÃºblico de skills com `search()`, `register()`, `CatalogEntry`.

### Changed
- `McpManifest` ganha campo `contracts: Vec<&'static CompletionContract>`
- `SkillRegistry::execute_skill()` e `execute_skill_unchecked()` chamam `verify()` + contratos pÃ³s-exec
- Todas as 7 implementaÃ§Ãµes de `Skill` ganham `verify()` e `contracts: Vec::new()`
- `Command::Learn` separado de `Command::AddSkill` â€” registro direto sem LLM

### Fixed
- **VirtualBox SMP**: Novo `AP_COUNT` static lido do MADT `lapic_count`. Se 0 APs, `init_smp()` retorna sem INIT-SIPI-SIPI. 2 vCPUs no VBox agora bootam confiavelmente.

### Added (neural-kernel/hermes)
- **60.1b**: Prompt `>` interativo â€” `show_prompt` default `true` no NeuralConsole
- **67.2.1**: `/learn <nome> <desc>` cria `DynamicSkill` + registra em SkillRegistry + SkillLoader
- **72.6**: `McpCatalog` populado via `SkillRegistry.list_skills()`

### Deprecated
- N/A

### Sources
- Sprint 60, 67, 72 plans
- ADR-0036 (JARVIS)

---

## [0.72.0] â€” 2026-07-02 â€” ðŸ† EvoluÃ§Ã£o AgÃªntica: Crew + FlowTrigger + StateGraph

### Added (agent-core)
- **Crew** (`crew.rs`): `Crew`, `CrewPool`, `ScheduledTask`, `OutputSchema`, `ProcessType` (Sequential/Hierarchical). Times de agentes com objetivo comum, tasks com dependÃªncias, kickoff/delegation pattern (CrewAI-inspired).
- **FlowTrigger** (`flow.rs`): `FlowTrigger::Start/Listen/Router` â€” quando e como um agente acorda. `RouterRegistry` para roteamento baseado em payload do EventBus. `should_poll_flow()` substitui `match schedule` no scheduler.
- **StateGraph** (`state_graph.rs`): `StateGraph` com nÃ³s (agentes) e arestas (condiÃ§Ãµes de transiÃ§Ã£o). Substitui scheduler round-robin por grafo de estados (LangGraph-inspired).
- **CrewManifest**: ExtensÃ£o opcional do `AgentManifest` com `role`, `goal`, `backstory`, `flow`, `crew_id`. Sem modificar o struct original (evita quebrar 24+ const definitions).
- **CrewAgent trait**: Agente que implementa role semantics.
- **AgentRegistry**: `create_crew()`, `assign_to_crew()`, `init_graph()` â€” integraÃ§Ã£o com CrewPool + StateGraph.

### Changed
- `AgentInstance` ganha campo `crew: CrewManifest`
- `AgentRegistry::run()` suporta FlowTrigger e StateGraph como alternativas ao round-robin

### Added (skill-registry)
- **OutputSchema** (`mcp.rs`): enum `Any, String, Json(Vec<String>)` com validaÃ§Ã£o de output de skills. `McpManifest` ganha `preconditions` (caminhos VFS para contexto), `context_links` (skills relacionadas), `output_schema`, `idempotent` (cacheÃ¡vel).
- **OutputCache** (`cache.rs`): Cache de outputs de skills idempotentes com hash(input) e TTL. Evita re-execuÃ§Ã£o de `system_status`, `echo`, etc. Suporta `get()`, `set()`, `evict_expired()`.
- **SkillIndex** (`index.rs`): CatÃ¡logo de skills por domÃ­nio (`by_domain`) e capacidade (`by_capability`). `relevant(capabilities)` para progressive disclosure via Hermes.

### Added (neural-kernel/hermes)
- **IntentCache** (`hermes.rs`): Cacheia intents (hash do input â†’ Command) com TTL de 1000 ticks. HermesAgent consulta antes de chamar `cortex.think()`. Evita re-classificaÃ§Ã£o de comandos repetidos.
- **WorkflowEngine** (`hermes.rs`): MÃ¡quina de estados THINKâ†’PLANâ†’EXECUTEâ†’VERIFYâ†’REFINEâ†’DONE. Suporta retry com `max_retries`. Usado por HermesAgent para workflows multi-passo.

### Changed
- Todas as 6 implementaÃ§Ãµes de `Skill` atualizadas para os novos campos `McpManifest`.
- `Command` agora Ã© `Clone` (necessÃ¡rio para IntentCache).

### Sources
- CrewAI (link 5): TaskSchema + OutputSchema
- AI Memory Vault (link 8): JobPreconditions, context_links
- Hermes Agent 10x (link 10): IntentCache, OutputCache, WorkflowEngine
- MCP Catalog (link 4): SkillIndex por domÃ­nio/capacidade

## [0.71.1] â€” 2026-07-02 â€” ðŸ† Xuvisco exterminado: 3 bugs em cascata corrigidos

### Fixed
- **Xuvisco (causa raiz #3 â€” framebuffer race)**: `_print()` agora verifica se o compositor estÃ¡ ativo antes de chamar `fb_write_text()`. Quando o DisplayAgent estÃ¡ rodando, apenas o compositor escreve no framebuffer via `DoubleBuffer::swap()`. Elimina a briga entre `println!` (texto amarelo) e o compositor (tela completa) que causava flicker e sobreposiÃ§Ã£o.
- **VGA text mode totalmente desligado com framebuffer**: Quando o framebuffer da UEFI estÃ¡ disponÃ­vel, `vga_buffer::init()` NÃƒO Ã© chamado. Zero escritas em 0xB8000, zero toques nos registros VGA CRTC (0x3D4/0x3D5). A camada de texto VGA nÃ£o Ã© mais ativada.
- **`_print()` seguro sem Writer**: `write_fmt()` usa `let _ = ...` em vez de `.unwrap()` para evitar panic se o Writer VGA nÃ£o foi inicializado.

### Changed
- **`vga_buffer::_print()`**: SÃ³ chama `fb_print()` quando o compositor NÃƒO estÃ¡ ativo. Com compositor ativo, todo output de tela passa pelo DisplayAgent.
- **`main.rs`**: `vga_buffer::init()` condicional â€” sÃ³ executado se nÃ£o hÃ¡ framebuffer.

## [0.71.0] â€” 2026-07-02 â€” ðŸ† Boot Bughunt: Agent-First + DiagnosticSkill + FAT12 Log + Xuvisco Fix

### Fixed
- **Xuvisco (VGA CRTC corruption em Intel 6xx)**: `probe_uefi_framebuffer()` movido para ANTES do VGA text mode init. `println!` nÃ£o escreve mais nos registros VGA CRTC (0x3D4/0x3D5) em modo UEFI GOP, eliminando a corrupÃ§Ã£o do display no boot.
- **FAT12 log nÃ£o era gravado**: `boot_logger.rs` sÃ³ aceitava FAT32 (type_code 0x0B/0x0C). Adicionado suporte a FAT12 (type_code 0x01). `write_boot_log()` agora usa `Fat12Writer` para FAT12, `Fat32Writer` para FAT32.
- **BootLogAgent ignorava FAT12**: `read_last_boot_log()` sÃ³ procurava B<TICK>.LOG em FAT32. Agora lÃª BOOT.LOG de partiÃ§Ãµes FAT12 tambÃ©m.
- **fb_write_text sem bounds check**: Adicionada verificaÃ§Ã£o de limite do buffer para evitar escrita fora do framebuffer.
- **fb_write_text division by zero**: `max_lines == 0` tratado para evitar panic em resoluÃ§Ãµes muito baixas.
- **fb_write_text LINE wrap**: `static mut LINE` agora incrementa corretamente sem pular a linha 0.

### Changed
- **Boot vira sequÃªncia de agentes**: `BOOT_PHASE` events publicados no EventBus (SafeHarborâ†’MemoryCoreâ†’SystemBringupâ†’Diagnosticsâ†’HardwareDiscoveryâ†’DriverInitâ†’AgentFleetâ†’Runtime). HermesAgent, CortexAgent e BootLogAgent podem subscrever.
- **90+ linhas de teste inline â†’ DiagnosticSkill**: Box/Vec/Tensor/SiLU/RMSNorm/BitNet movidos para `DiagnosticSkill` em `agents.rs`. SystemAgent executa durante fase Diagnostics.
- **CortexAgent acorda antes do HW discovery**: Modelo LLM carregado e agente instanciado antes do PCI scan, RTL8139, ATA, xHCI. O sistema nervoso participa das decisÃµes de hardware.
- **BootLogAgent agora contÃ­nuo**: `auto_start=true`, `persist=true`, `ScheduleKind::Continuous`. Monitora boot logs em tempo real.
- **`Fat12Writer::root_lba()` e `data_lba()` agora pub**: Para BootLogAgent acessar a geometria da partiÃ§Ã£o.
- **`display/fb.rs`**: Removido VGA FIX que modificava stride do framebuffer (causava mismatch). Stride original da UEFI Ã© preservado.
- **`vga_buffer.rs`**: `fb_write_text()` com bounds check e divisÃ£o por zero tratada.

### Added
- **`BootPhase` enum + `publish_boot_phase()`**: 8 fases de boot com eventos no EventBus.
- **`DiagnosticSkill`**: Skill de diagnÃ³stico que substitui os testes inline. SystemAgent executa na fase Diagnostics.
- **`TOPIC_BOOT_PHASE`**: Constante do tÃ³pico EventBus para fases de boot.

## [0.65.0] â€” 2026-06-30 â€” COSMIC UI Patterns + AxiomOS Verifier + HAL + Bench

### Added
- **Workspace manager** (COSMIC): `display/workspace.rs` â€” 3 workspaces, LayoutMode (Floating/Tiled/Grid/Maximized)
- **Notification overlay** (COSMIC): `display/notifications.rs` â€” temporÃ¡rias, 3 severidades, expire
- **Auto-tiling layout** (COSMIC): `display/layout.rs` â€” Tile, Grid, Maximize, Floating
- **Skill verifier** (AxiomOS): `verify.rs` â€” eBPF-style opcodes, verify_skill(), execute_verified()
- **HAL trait** (AxiomOS): `hal.rs` â€” `trait Architecture` + impl X86_64
- **Benchmark framework** (AxiomOS): `bench.rs` â€” start/end_bench, alloc_throughput

## [0.64.0] â€” 2026-06-30 â€” Voice skill + Gbrain reranker + BrowserAgent

### Added
- **Voice skill**: `voice_skill.rs` â€” speak(text, profile), 8 preset voices, display fallback
- **Gbrain reranker**: `kgraph.rs` â€” `ranked_query()` combina label match + edge scores
- **BrowserAgent**: `browser_agent.rs` â€” fetch_page, extract_text (HTML tag-stripper), PageViewerApp, cache
- PageViewerApp: janela no compositor que mostra conteÃºdo de pÃ¡ginas web

## [0.63.1] â€” 2026-06-30 â€” MegaTrain patterns + Self-skill generation

### Added
- **MegaTrain streaming**: `mhi.rs` â€” MEGATRAIN_QUEUE, enqueue_prefetch(), megatrain_tick()
- **Self-skill generation**: `skill_gen.rs` â€” TaskPattern registry, generate_skill(), auto apÃ³s 3 usos

## [0.63.0] â€” 2026-06-30 â€” Cortex Evolution + PTRM + Kanerva + Anatomy

### Added
- **Model trait**: `cortex.rs` â€” `pub trait Model: Send`, `set_model()`, `generate_via_model()`
- **PTRM**: `cortex.rs` â€” `gaussian_noise()`, `ptrm_generate()`, Q-head, 3 trajetÃ³rias
- **Kanerva Memory**: `kanerva.rs` â€” sparse_read, distributed_write, bayesian_update, hamming_distance
- **Hard blocklist**: `safety.rs` â€” 12 comandos que NUNCA rodam, check_command()
- **Curated memory**: `conversation.rs` â€” curated_context() com budget 4KB

## [0.61.0] â€” 2026-06-30 â€” Sprint 61 Desktop completo (7/7 sub-sprints)

### Added
- **MouseAgent (61.0)**: PS/2 mouse driver como agente A-021. IRQ12 handler, pacote 3 bytes, EventBus MOUSE_MOVED/MOUSE_CLICK/MOUSE_DRAG. 5 skills. ~200 LOC.
- **Theme Engine (61.1)**: 5 temas (hermes-dark, dracula, matrix, solarized, hermes-light). Hot-swap via `theme.apply()`. Integrado ao console. ~120 LOC.
- **Compositor (61.2)**: Multi-window com z-order, dock bar 36px com botoes + relogio, drag de janelas via title bar, cursor cross. Subscreve MouseAgent events. ~300 LOC.
- **Shell (61.3)**: 15 comandos (help, echo, clear, uptime, ps, meminfo, pci, theme, profile, shutdown, reboot, date, uname, cpuinfo, ls). ~100 LOC.
- **3 Desktop Apps (61.4)**: Hermes App (chat+shell), Settings App (theme+profile picker), Power App (shutdown+reboot+confirmacao). AppRegistry estatico. ~250 LOC.
- **LLM Icons (61.5)**: IconCache com fallback geometrico por hint hash. Render 16Ã—16 (2-bit palette). ~80 LOC.
- **WASM Sandbox (61.6)**: WasmSandbox com load/execute stub, scan_exports. Preparado para wasmi. ~80 LOC.

### Fixed
- **Status bar height**: `fill_rect` usava `status_y + ch + 2` (22px) em vez de `ch + 3` (19px) â€” invadia area de conversa
- **Prompt height**: `fill_rect` usava `prompt_y + ch + 1` (737px!) em vez de `ch + 3` (19px)
- **conv_y**: realinhado para comecar logo apos a status bar (sem overlap)
- ConsoleAgent: removeu `println!` (VGA) â€” display framebuffer e suficiente
- DisplayAgent: filtro de mensagens â€” apenas `[Hermes]`, `Hermes v`, `>`, `/` aparecem

## [0.62.2] â€” 2026-06-30 â€” InferenceFS, HermesFS, RamFS, MHI Scheduler

### Added
- **InferenceFsAgent**: `/inference/` â€” arquivos gerados sob demanda via LLM, buffer de treino
- **HermesFsAgent**: `/chat/` â€” send (writeâ†’LLM), last_response, history, clear, count
- **RamFsAgent**: `/mnt/ram/` â€” cache DRAM com quota 1MB, LRU eviction
- **MhiScheduler**: scan MHI_REGISTRY a cada 1000 ticks, promove/demove tiers por acesso
- MhiScheduler integrado ao OptimizerAgent.tick()
- AllocTier::UsbMsc adicionado ao mhi.rs

## [0.62.1] â€” 2026-06-30 â€” Storage Agents: Ata, DevFs, ProcFs

### Added
- **AtaAgent**: `/mnt/hdd/sda` â€” ATA block device como arquivo
- **DevFsAgent**: `/dev/pci/list`, `/dev/pci/<vid:did>`, `/dev/rtl8139`, `/dev/xhci`, `/dev/mem`
- **ProcFsAgent**: `/proc/agents`, `/proc/meminfo`, `/proc/uptime`, `/proc/cpuinfo`, `/proc/version`, `/proc/profile`, `/proc/mhi`
- **FilesystemAgent trait**: `read()`, `write()`, `list()`, `mount_point()` â€” interface padrao para FS agents
- **VFS bridge**: `read_vfs()`, `write_vfs()`, `list_vfs()` â€” resolve mount e delega ao agente
- VFS init + 8 mounts no boot: `/`, `/mnt/ram`, `/mnt/hdd`, `/mnt/sdhc`, `/chat`, `/dev`, `/proc`, `/system`, `/inference`

## [0.62.0] â€” 2026-06-30 â€” VFS Layer + MHI ARC-style Tier Suggestion

### Added
- **VFS Layer**: `VfsRegistry` (mount, resolve, lookup, list_dir), `VfsNode` (arvore de diretorios), `VfsMount`
- **Path utils**: `canonicalize()`, `split()`, `join()`, `filename()`, `parent()`, `match_mount()`
- **MHI ARC-style**: `arc_suggest_tier()` â€” ZFS-ARC-inspired (MFUâ†’Dram, MRUâ†’Nvme, coldâ†’Hdd)
- **AllocTier::UsbMsc**: novo tier para USB Mass Storage
- Sprint plan atualizado: `docs/sprint-062-fs.md` com MHI+VFS+StorageAgents unificado

## [0.60.5] â€” 2026-06-30 â€” RTL8139 early init 32KB RX

### Fixed
- RTL8139 init movido para kernel_main (antes da fragmentacao do frame allocator)
- `alloc_pages(8)` para RX buffer de 32KB contiguo
- `init_driver_rtl8139()` idempotente (chamado 2x: boot + NetDriverAgent)

## [0.60.4] â€” 2026-06-30 â€” RTL8139 TX + iPXE buffer sync

### Fixed
- **TSD_SIZE_SHIFT 16â†’0**: SIZE nos bits 0-12 (correto). TX funcionando com TOK=1
- **iPXE RX buffer**: `rx_offset = CAPR` apos init â€” pula dados do bootloader
- **smoltcp tight poll**: loop `poll_delay()` para DHCP multi-step
- **IP estatico imediato**: 10.0.2.15/24 no tick 11 (bypass DHCP)

### Added
- Plano Desktop: `docs/sprint-061-desktop.md` (6 sub-sprints, ~2800 LOC)
- Plano FS: `docs/sprint-062-fs.md` (6 sub-sprints, ~2700 LOC)
- Plano WWW: `docs/sprint-063-www.md` (7 sub-sprints, ~2600 LOC)

## [0.60.3] â€” 2026-06-30 â€” e1000 TX non-blocking + mmio_virt + map_page_uc

### Fixed
- **e1000 Page Fault**: `map_page_uc()` mapeia PCI MMIO (cria page table entries)
- **e1000 TX non-blocking**: TDT=(idx+1)%64, sem wait loop (QEMU TCG nao processa TX while spinning)


### Added
- **Ecosystem Batch 3 (12 repos, 8 arquivos, 601 LOC)**:
  - redox-os/redox (16.4kâ˜…) â†’ `scheme.rs`: SchemeHandler trait para namespace I/O
  - theseus-os/Theseus (3.2kâ˜…) â†’ `state.rs`: TypedAgent<Boot|Running|Faulted|Done>
  - embassy-rs/embassy (9.5kâ˜…) â†’ `timer_wheel.rs`: 64-slot TimerWheel
  - openai/swarm (21.8kâ˜…) â†’ HermesAgent: Handoff enum (SwitchTo/Escalate/Delegate)
  - tock/tock (5.3kâ˜…) â†’ `mmio.rs`: Register<T> + RegisterField<OFFSET,WIDTH>
  - raga-ai-hub/RagaAI-Catalyst (16kâ˜…) â†’ `tracer.rs`: 256-span ring buffer
  - kyegomez/swarms (6.9kâ˜…) â†’ `orchestrator.rs`: task decomposition
  - TransformerOptimus/SuperAGI (16kâ˜…) â†’ `skill_market.rs`: SkillScore scoring table
- `cargo check --release`: 0 errors âœ…

## [0.59.1] â€” 2026-06-29 â€” HW Agents + Native Agent Fleet

### Added
- **HW Agents**: `hw_agents.rs` â€” HwRegistry por PCI, HwAgent por dispositivo, `class_to_capabilities()`, `activate_for_intent()`
- **Especialistas nativos**: `agency.rs` â€” 12 divisÃµes (engineering, design, product, qa, support, marketing, infra, data-science, creative, legal, spatial, research)
- **SpecialistAgent** struct genÃ©rica com missÃ£o, skills, entregÃ¡vel
- `register_agency_agents()` registra todos no boot

## [0.59.0] â€” 2026-06-29 â€” ðŸ† Bootloader 0.11 + Framebuffer UEFI + Hermes GrÃ¡fico

### Added
- **Bootloader 0.11.15**: `bootloader_api` substitui `bootloader::bootinfo`, `BootloaderConfig` com `physical_memory=Dynamic`, stack 512KB
- **Framebuffer 1280Ã—720**: `probe_uefi_framebuffer()` via `BootInfo::framebuffer`, BGR pixel suporte, stride em BYTES
- **Serial Fallback**: `Mutex<Option<SerialPort>>`, `probe_port()` em 4 endereÃ§os (0x3F8/0x2F8/0x3E8/0x2E8)
- `fb_print()` escreve no framebuffer quando serial ausente
- DisplayAgent renderiza NeuralConsole com framebuffer ativo
- `tools/build_image.py` via `bootloader::BiosBoot` + BIOS/UEFI modes

### Changed
- Branch `test-bootloader-0.11` promovida a `main` (force push)
- `kernel_stack_size=512KB` previne triple fault no stack probe
- `mov ss, 0` apÃ³s init_idt() evita #GP no breakpoint handler
- `vga_buffer::_print()` pula VGA quando framebuffer ativo
- `.cargo/config.toml`: rustflags `[]` (sem relocation-model=static)

### Fixed
- #GP no breakpoint handler: SS nÃ£o era recarregado apÃ³s GDT
- Triple fault: stack 256KB â†’ 512KB
- Serial detection: porta 0x3F8 falha em notebooks modernos â†’ fallback 0x2F8/0x3E8/0x2E8

## [0.58.0] â€” 2026-06-28 â€” ðŸ† Boot em Hardware Real + USB + FAT12 + ATA

### Added
- **ðŸ† Primeiro boot do Neural OS Hermes em notebook fÃ­sico via SDHC USB** (2.7MB imagem, Rufus DD+MBR+CSM)
- **xHCI USB HID Keyboard Driver**: `init_xhci()`, `poll_keyboard()` com Event Ring, HIDâ†’PS/2 scancode (68 teclas), CAD via USB
- **MBR+FAT12 Partition Recognition (PERMANENTE)**: `fat.rs::read_mbr()`, `Fat12Writer::append_log()` via ATA PIO
- **FAT12 Boot Log Partition**: `tools/patch_image.py` adiciona 2MB FAT12, BOOT.LOG visÃ­vel no Windows
- **ATA PIO Driver**: `AtaDriver::probe()` + `read_sectors()`/`write_sectors()` LBA28 com wait_bsy+wait_drq
- **Ctrl+Alt+Del Log Dump**: `handle_cad()` grava log no FAT12, reset 8042, hlt

### Fixed
- **OOM em HW real**: HEAP_SIZE 4MBâ†’16MB, `serial_println!` sem alloc, `#[alloc_error_handler]` seguro
- **VGA Scrolling**: Cursor via portas 0x3D4/0x3D5, new_line() sempre na Ãºltima linha

## [0.57.1] â€” 2026-06-27 â€” Consolidation: Plugin Hub + x2APIC + Ed25519 + SMP Stacks

### Added
- **Plugin Hub (#236)**: PluginManager trait + PluginRegistry
- **x2APIC**: ativado via `core::arch::x86_64::__cpuid()`, substitui APIC regs por MSR
- **Ed25519 real**: `ed25519-compact` crate substitui stub (trust_cache.rs)
- **SMP per-AP stacks 64KB**: cada AP tem stack isolado
- **VirtIO-GPU poll fix**: `sti;hlt` loop (evita VM exit no QEMU TCG)

## [0.57.0] â€” 2026-06-27 â€” Bloco 15+16+17: Memory + Ecosystem + LLM v2

### Added
- **MemoryTree v2**: TTL/Eviction, Ebbinghaus decay, 4-Tier consolidation (event-bus)
- **SHA-256 Dedup (#214)**: `dedup.rs` com content-based hash
- **Privacy Filter (#215)**: `privacy.rs` com regex patterns
- **Hybrid Search (#218)**: `hybrid_search.rs` (embedding + keyword)
- **Metacognitive Guard (#220)**: `metacognitive.rs` confidence threshold
- **Draftâ†’Reviewâ†’Merge (#221)**: `draft_review.rs` 3-phase write pipeline
- **Atkinson-Shiffrin 3-tier (#224)**: `atkinson.rs` Sensoryâ†’STMâ†’LTM
- **SuperContext**: memory+KG scout (event-bus)
- **SkillIndex**: progressive disclosure (event-bus)
- **TokenJuice**: HTML strip + URL shorten (event-bus)
- **Sampling**: top-k, temperature (cortex.rs)
- **Codebook VQ (#165)**: quantize em tensor.rs
- `generate_speculative()` funcional (Medusa 3-head)

## [0.56.0] â€” 2026-06-26 â€” Medusa + Pipeline + Memory Tree + Knowledge Graph

### Added
- **Medusa 3-head speculative decoding** (cortex.rs)
- **Pipeline manifest** (agent-core): `Pipeline::new()` + `Sequence::linear()`
- **Memory Tree** (event-bus): `MemoryTree::insert()` + `recall()`
- **Knowledge Graph** (event-bus): `KnowledgeGraph`, `add_triple()`, `query()`
- **DAG scheduler** (agent-core): `DagScheduler` topological sort
- **Dashboard** (agent-core): DashboardPanel trait  
- **Ecosystem Analysis**: OpenMontage, OpenHuman, codebase-memory-mcp, Rinne, daily_stock, ComPilot

### Added
- **CDC Rabin Chunking** (`chunker.rs`) â€” Content-Defined Chunking via rolling hash polinomial de 64 bits. Divide bitmaps e buffers em chunks de tamanho variÃ¡vel baseado no conteÃºdo. `chunk_data()` â†’ `merge_chunks()` round-trip testado.
- **XOR Delta** (`delta.rs`) â€” `ArchiveTensor` com reconstruÃ§Ã£o bit-exata via XOR residual entre versÃµes de `PackedTernaryTensor`. `ArchiveTensor::new()` + `reconstruct()` com round-trip testado.
- **Semantic Snapshot** (`self_heal.rs`) â€” `SelfHeal::semantic_snapshot(prev_bitmap)` aplica CDC Rabin + XOR delta no bitmap do alocador. Armazena apenas chunks modificados entre checkpoints.
- **IrqSafeLock** (`sync/irq_lock.rs`) â€” FIFO lock com `cli` na aquisiÃ§Ã£o e restauraÃ§Ã£o de RFLAGS.IF no drop. Previne deadlock em handlers de interrupÃ§Ã£o.
- **DmaBuf** (`dma.rs`) â€” `dma_alloc(size)` retorna `DmaBuf { phys, virt, size }` com pÃ¡ginas marcadas `NO_CACHE | WRITE_THROUGH`. Previne corrupÃ§Ã£o por cache incoerente CPUâ†”DMA.
- **Watchdog** â€” `AgentInstance::consecutive_pending`. Se agente retorna Pending por 10000+ ticks seguidos, scheduler forÃ§a estado `Crashed`. PrevÃª loop infinito.

### Changed
- `SKILL_REGISTRY`, `TRUST_CACHE`, `EVENT_LOG`, `USAGE_TRACKER`, `CONVERSATION_TRACKER`, `SKILL_STORAGE` migrados de `spin::Mutex` para `ticket_lock::TicketLock` (FIFO, sem starvation).
- `SELF_HEAL`, `RESPAWN_QUEUE`, `PENDING_SKILL` migrados para `crate::sync::irq_lock::IrqSafeLock` (IRQ-safe).

### Removed
- Ãšltimos vestÃ­gios de `spin::Mutex` em estruturas de contenÃ§Ã£o mÃ©dia/alta.

### Fixed
- Bug H3 (APIC SVR) â€” vetor espÃºrio redirecionado para 255.
- Bug H4 (IDT) â€” cobertura total 0-31 com 32 handlers nomeados.
- Bug H5 (PIC EOI) â€” EOI duplo no escravo (0xA0) para vetores >= 40.
- Bug H11 (PCI multi-function) â€” header_type bit 7 verificado.
- Bug H12 (IOAPIC mask) â€” RTEs nÃ£o usadas mascaradas.

## [0.59.0] â€” 2026-06-29 â€” ðŸ† Bootloader 0.11 + Framebuffer UEFI + Hermes Grafico ðŸ†

### Breaking: Bootloader 0.9.34 â†’ 0.11.15
- **bootloader_api** substitui `bootloader::bootinfo::BootInfo`
- `BootloaderConfig` com `physical_memory = Dynamic` (substitui `map_physical_memory`)
- `kernel_stack_size = 512KB` (stack probe de 256KB exigido pelo kernel)
- Build via `tools/build_image.py` (cria imagem BIOS com `bootloader::BiosBoot`)
- Branch antiga `main-bootloader-0.9` mantida como referencia

### Added â€” Framebuffer UEFI (bootloader 0.11)
- `BootInfo::framebuffer` detectado em `probe_uefi_framebuffer()`
- GpuDevice ganhou `fb_bpp: u32` (bytes per pixel)
- `FramebufferInfo.bpp`: suporta BGR (3 bytes) e BGRA32 (4 bytes)
- Stride convertido de pixels para bytes (info.stride * bpp)
- `vga_buffer::_print()` pula escrita VGA quando framebuffer ativo
- DisplayAgent renderiza NeuralConsole no framebuffer 1280Ã—720

### Fixed â€” #GP no breakpoint handler
- **Causa**: bootloader 0.11 usa GDT diferente â†’ SS=0x10 = TSS selector
- **Fix**: `mov ss, ax` com seletor nulo (0) apos carregar GDT
- Sintoma: `[EXCEPTION] #GP ip=breakpoint_handler cs=0x8 err=0x10` no iretq

### Fixed â€” Triple fault silencioso
- **Causa**: kernel faz stack probe de 256KB, bootloader so alocava 128KB default
- **Fix**: `kernel_stack_size = 512 * 1024` no BootloaderConfig
- Sintoma: bootloader log mostra "Jumping to kernel entry point" mas nenhum output

### Aprendizados (Bootloader 0.11 vs 0.9.34)
1. **BootloaderConfig** obrigatorio â€” sem ele, physical_memory=None, stack=80KB
2. **Stack probe**: Rust gera codigo que testa N paginas de stack no entry point. Se o bootloader nao alocar suficiente â†’ triple fault silencioso
3. **GDT/SS incompativel**: bootloader 0.11 usa GDT propria. Ao carregar nossa GDT, SS fica invalido â†’ #GP no iretq
4. **Framebuffer stride**: bootloader 0.11 reporta stride em PIXELS, nao bytes. Multiplicar por bytes_per_pixel
5. **Pixel format BGR**: framebuffer UEFI usa 3 bytes/pixel (BGR), nao 4 (BGRA32). set_pixel precisa escrever so 3 bytes
6. **Build process**: bootimage tool v0.10 nao suporta bootloader 0.11. Precisa de build.rs ou script externo
7. **MinGW + caminho com acentos**: linker MinGW falha com caracteres especiais no path (Ãrea de Trabalho). Solucao: mover projeto para C:\dev\

## [0.58.0] â€” 2026-06-28 â€” ðŸ† MARCO: Boot em Hardware Real + USB Keyboard + FAT12 Log ðŸ†

### ðŸ† MARCO HISTÃ“RICO: Neural OS Hermes boota em hardware real!

Pela primeira vez, o Neural OS Hermes bootou em um **notebook fÃ­sico** (x86-64 real) via **SDHC USB**. O kernel saiu do QEMU e rodou em silÃ­cio real. As conquistas:

- **Boot completo**: VGA text mode funcional, PCI/ACPI/APIC/SMP todos operacionais
- **Hermes Cognitive**: ReAct loop rodando estÃ¡vel (7 fases: OBSERVEâ†’THINKâ†’PLANâ†’BUILDâ†’EXECUTEâ†’VERIFYâ†’LEARN)
- **Zero panics** apÃ³s correÃ§Ã£o do OOM (heap 4MBâ†’16MB)

### Added â€” xHCI USB HID Keyboard Driver (completo)
- **Driver HID Boot Protocol** completo: `init_xhci()` global + `poll_keyboard()` com Event Ring parsing
- **Tabela HIDâ†’PS/2**: 68 teclas mapeadas (A-Z, 0-9, sÃ­mbolos, ENTER, BACKSPACE, DELETE)
- **CAD via USB**: detecta LCtrl + LAlt + Delete no HID report (byte 0 modifiers + byte 2 usage)
- **64KB de hastes de Ebbinghaus**: integrado com InputAgent (poll a cada 5 ticks)
- **Driver persistente**: XhciState global inicializado uma vez no boot, nÃ£o recriado a cada poll

### Added â€” MBR + FAT12 Partition Recognition (PERMANENTE)
- **MBR parser** (`fat.rs::read_mbr()`): lÃª tabela de partiÃ§Ãµes do setor 0 via ATA PIO
- **FAT12 BPB reader**: detecta qualquer partiÃ§Ã£o FAT12 no disco
- **Fat12Writer**: `append_log()` escreve no arquivo BOOT.LOG via ATA read/write
- Reconhecimento de partiÃ§Ãµes Ã© **permanente** â€” o kernel sempre enxerga o layout do disco

### Added â€” FAT12 Boot Log Partition (temporÃ¡rio)
- **`tools/patch_image.py`**: script Python que adiciona partiÃ§Ã£o FAT12 de 2MB ao final da bootimage
- **BOOT.LOG** visÃ­vel no Windows Explorer apÃ³s boot + CAD
- **Timestamps**: cada linha do log prefixada com `[T+SSS.mmm]` (segundos.millis desde o boot)
- **Buffer 64KB**: circular, sem alocaÃ§Ã£o de heap, timestamp via aritmÃ©tica u8

### Added â€” ATA PIO Driver completo
- **`AtaDriver`**: probe via PCI (class 0x01), `read_sectors()` + `write_sectors()` com wait_bsy/wait_drq
- Cache flush via comando 0xE7 apÃ³s writes
- Fallback silencioso se nenhum controlador ATA presente

### Fixed â€” OOM em Hardware Real
- **HEAP_SIZE**: 4MB â†’ **16MB** (4096 pÃ¡ginas mapeadas)
- **`serial_println!`**: removido `alloc::format!` â€” escreve direto no serial via `write_fmt`
- **Panic handler**: safe path sem alocaÃ§Ã£o (`write!` direto para VGA/serial); tentative path com `try_alloc_check()`
- **`#[alloc_error_handler]`**: diagnostico OOM sem alocar memÃ³ria
- **`LogBuf`**: implementaÃ§Ã£o prÃ³pria de `fmt::Write` em buffer stack de 256 bytes

### Fixed â€” VGA Scrolling em Hardware Real
- **Row tracking**: cursor real que incrementa a cada newline, scroll sÃ³ quando atinge BUFFER_HEIGHT-1
- **`new_line()`**: agora sobe linhas corretamente sem truncar para a Ãºltima linha

### Added â€” Ctrl+Alt+Del com log dump
- **DetecÃ§Ã£o**: PS/2 (IRQ1) + USB HID (LCtrl+LAlt+DEL)
- **AÃ§Ã£o**: serial log dump + FAT12 ATA write + PS/2 8042 reset + hlt
- Log escrito no setor LBA 0 + partiÃ§Ã£o FAT12

### Aprendizados (Hardware Real vs QEMU)
1. **OOM**: QEMU tolera heap 4MB; HW real precisa de 16MB. `alloc::format!` dentro de `serial_println!` causava OOM recursivo no panic handler.
2. **VGA buffer**: `write_byte` sempre escrevia na Ãºltima linha (`BUFFER_HEIGHT-1`). Novo cursor real corrige scroll.
3. **PS/2 vs USB**: Notebooks modernos nÃ£o tÃªm controlador PS/2. Teclado USB sÃ³ funciona via xHCI HID Boot Protocol.
4. **ATA vs USB storage**: Leitor de SDHC interno geralmente estÃ¡ em SATA/PCI. USB mass storage Ã© mais complexo.
5. **FAT12 vs RAW**: PartiÃ§Ã£o FAT12 Ã© reconhecida pelo Windows Explorer imediatamente. RAW sector precisa de HxD/PowerShell.
6. **MBR signature 55AA**: Sempre verificar â€” bootloader pode ou nÃ£o preservar o MBR original.

## [0.57.1] â€” 2026-06-27 â€” Consolidation: Plugin Hub + x2APIC + Ed25519 + SMP stacks

### Added
- **Plugin Hub** (#236) â€” `plugin_hub.rs`: install/remove/scan_risk/discover de plugins
  remotos com AI security scan (10-level risk scoring por nome de skill)
- **x2APIC ativado** â€” CPUID leaf 1 ECX[21] detecta suporte, MSR IA32_APIC_BASE[10]
  habilita modo MSR-based. Fallback MMIO se TCG nao suportar.
- **Ed25519 real** â€” `ed25519-compact` crate (2.3.1, no_std, sem SIMD) substitui stub.
  `verify_signature()` usa `PublicKey::from_slice` + `verify`. TRUSTED_PUBLIC_KEYS array.

### Fixed
- **SMP per-AP stacks**: cada AP agora tem stack de 64KB dedicada no heap,
  em vez de compartilhar topo do heap entre todos os cores. Previne corrupÃ§Ã£o de pilha.
- **x2APIC CPUID**: substitui inline asm com `out("ebx")` (conflito LLVM/MinGW)
  por `core::arch::x86_64::__cpuid()`. Compila em x86_64-unknown-none.

### Aprendizados
- `ed25519-compact` Ã© no_std puro (sem SIMD, sem bindings C) â€” roda em qualquer target
- `core::arch::x86_64::__cpuid` retorna `CpuidResult` (nÃ£o Result) â€” API infalÃ­vel
- SMP precisa de stacks separadas por AP: 64KB Ã— 4 cores = 256KB do heap
- Plugin Hub com risk scoring de skills cabe em ~200 LOC

## [0.57.0] â€” 2026-06-27 â€” Bloco 15+16+17: Memory Systems + Ecosystem + LLM v2 ðŸ§ ðŸ

### Added â€” Bloco 15: Memory Systems (completo)
- **MemoryTree v2** (`event-bus/memory_tree.rs`) â€” TTL/Eviction por nÃ³, Ebbinghaus decay (`ebbinghaus_strength()`), 4-Tier Consolidation (`Workingâ†’Episodicâ†’Semanticâ†’Procedural`), promoÃ§Ã£o automÃ¡tica por access_count
- **SHA-256 Dedup** (`dedup.rs`) â€” FNV rolling hash, sliding window de 300 ticks, 64 entradas mÃ¡ximas
- **Privacy Filter** (`privacy.rs`) â€” 14 padrÃµes de secrets (API_KEY, sk-, ghp_, password, bearer, etc), substitui por `[REDACTED]`
- **Hybrid Search** (`hybrid_search.rs`) â€” TF-score + MLP score fusion, RRF-style ranking, top-10
- **Metacognitive Guard** (`metacognitive.rs`) â€” HistÃ³rico de 64 erros, `check(skill, type)` retorna fix conhecido
- **Draftâ†’Reviewâ†’Merge** (`draft_review.rs`) â€” 5 estados (Draftâ†’Reviewâ†’Approvedâ†’Rejectedâ†’Merged), `pending()` para HermesAgent
- **Atkinson-Shiffrin 3-tier** (`atkinson.rs`) â€” Sensory register (48h TTL, 64 items) â†’ STM (working memory tree) â†’ LTM (semantic tree), `attend()` promove sensoryâ†’STM, `promote_to_ltm()` STMâ†’LTM

### Added â€” Bloco 16: Ecosystem Integration
- **SuperContext** (`supercontext.rs`) â€” Integra MemoryTree + KG num scout unificado, `ingest()` registra agentâ†’skill edges + memÃ³ria
- **SkillIndex** (`skill_index.rs`) â€” Progressive disclosure: frontmatter-only scan, `scan(query)` retorna top-5 por domÃ­nio
- **TokenJuice** (`tokenjuice.rs`) â€” HTML tag stripping, URL shortening (>60 charsâ†’`[URL]`), whitespace dedup

### Added â€” Bloco 17: Cortex LLM v2
- **Sampling** (`cortex.rs::sample()`) â€” `top_k` (nucleus filtering), `temperature` scaling, softmax normalizaÃ§Ã£o, deterministic fallback
- **Model update topic** â€” `MODEL_UPDATE` EventBus topic para hot-swap de pesos .bitnet via HTTP download
- **Codebook VQ** (`tensor.rs::CodebookVQ`) â€” 16-centroid treino por uniform sampling, compressÃ£o 4:1, decompress lossy

### Fixed
- `memory_tree.rs` â€” borrow checker em `consolidate_inner()` resolvido com escopo de leitura antes de mutaÃ§Ã£o

### Aprendizados
- Bloco 15 (Memory Systems) foi o maior: ~450 LOC em 7 novos mÃ³dulos
- MemoryTree com Ebbinghaus + 4-tier cabe em ~200 LOC no_std com safe borrows
- Atkinson-Shiffrin 3-tierå¤ç”¨ MemoryTree como base â€” STM e LTM sÃ£o MemoryTree instances
- `select_nth_unstable_by` existe em no_std para sampling top-k
- Codebook VQ com 16 centroids dÃ¡ ~4:1 compressÃ£o para tensores f32

## [0.56.0] â€” 2026-06-27 â€” Medusa Speculative Decoding + Pipeline + Memory Tree + KG ðŸš€

### Added â€” Medusa Speculative Decoding (cortex.rs)
- **3 Medusa heads**: cada head `PackedTernaryTensor(HIDDEN, VOCAB_SIZE)` prediz token futuro
- **`generate_speculative()`**: draft 3 tokens, verify em 1 forward pass, aceita prefixo
- **Ganho teÃ³rico**: atÃ© 4 tokens/forward pass quando heads treinadas (~2-3Ã— em prÃ¡tica)
- **`forward_hidden()`**: retorna hidden state + logits (refatorado do forward())

### Added â€” Pipeline Manifest (agent-core/pipeline.rs)
- **Stage + Provider**: scored selection com fallback. Provider tem `score: u8` + `activate: fn() -> bool`
- **Pipeline runner**: executa stages em ordem, fallback automÃ¡tico se provider principal falha
- **Substitui boot sequence fixo** por pipeline declarativa

### Added â€” Memory Tree (event-bus/memory_tree.rs)
- **MemNode**: `{ summary, data, children, importance }` â€” chunks hierÃ¡rquicos â‰¤512 bytes
- **Scout**: percorre Ã¡rvore atÃ© depth N, retorna `(idx, summary, importance)` para contexto
- **Prune**: poda nÃ³s com importÃ¢ncia < threshold, base para TTL/eviction
- **Base do Bloco 15 Memory Systems**: Atkinson-Shiffrin, Ebbinghaus decay, 4-tier consolidation

### Added â€” Knowledge Graph (event-bus/kgraph.rs)
- **KNode + KEdge**: nÃ³s (Agent/Skill/Hardware/Event) + arestas com relaÃ§Ã£o nomeada
- **label_map**: Ã­ndice por label para lookup O(1)
- **neighbors()**: consulta de vizinhanÃ§a (source ou target)
- **query(relation)**: busca todas as arestas com relaÃ§Ã£o especÃ­fica
- **Base para correlaÃ§Ã£o de eventos de seguranÃ§a + trust graph**

### Added â€” DAG Scheduler (agent-core/dagsched.rs)
- **DagScheduler**: dependÃªncias nomeadas entre agentes/stages, topological sort
- **resolve()**: ordenaÃ§Ã£o topolÃ³gica com detecÃ§Ã£o de ciclos
- **run()**: executa agentes na ordem resolvida

### Added â€” Dashboard (agent-core/dashboard.rs)
- **Metric + Alert**: structs para relatÃ³rios estruturados de health status
- **Dashboard::render()**: saÃ­da textual formatada para SystemAgent/CronAgent

### Added â€” Pipeline de Treino v2 (tools/train_hw_model.py)
- **Muon optimizer** (opt-in --muon): Newton-Schulz 3rd order orthogonalization
- **Data augmentation**: 4 query variants por exemplo (~4Ã— dataset)
- **Medusa heads treinÃ¡veis**: loss auxiliar `0.3 Ã— medusa_loss / 3`
- **Export .bitnet v2**: u8 num_medusa_heads + 3 padding + head weights
- **Speculative generation no Python**: testÃ¡vel durante treino

### Added â€” Ecosystem Analysis (16 repos)
- Alta aderÃªncia: OpenMontage (pipeline), OpenHuman (Memory Tree), codebase-memory-mcp (KG)
- MÃ©dia aderÃªncia: Rinne (DAG), daily_stock (Dashboard), ComPilot (closed-loop), Cybersecurity Skills (frontmatter)
- Baixa aderÃªncia: design.md (tokens), Agent-Reach (channel), Voicebox (MCP), Penpot (design)

### Fixed
- `CUDA_VISIBLE_DEVICES=1` no ambiente escondia GTX 1050 â€” fix: sobrescrever com '0'
- Muon SVD causava timeout â€” substituÃ­do por Newton-Schulz 3rd order (~4Ã— mais rÃ¡pido)
- Muon produzia NaN com gradientes pequenos â€” adicionado clamp + NaN guard

### Aprendizados
- `torch.linald` Ã© `torch.linalg` (typo que quebrou primeiro build)
- NS iteration precisa de NaN guard + shape-aware (matrizes retangulares mâ‰ n)
- Memory Tree com summary hierÃ¡rquico cabe em ~200 LOC no_std
- Knowledge Graph com label_map index cabe em ~200 LOC no_std
- Pipeline manifest com fallback scored cabe em ~200 LOC no_std

## [0.55.0] â€” 2026-06-27 â€” Bloco 14 completo: Hermes Cognitive + Self-Optimization ðŸ§ ðŸ
### Added â€” Self-Optimization (fase 4/4)
- **Self-Optimizing Scheduler** (#161) â€” `get_agent_priority()` com 13 nÃ­veis. `suggest_schedule(workflow)` adapta prioridades baseado no workflow detectado
- **Hardware Config Learning** (#163) â€” `ConfigLearner` com snapshots periÃ³dicos da arquitetura. `suggest_arch_tuning()` sugere ajustes (ex: GPU presente â†’ ring1=GPU)
- **LLM decide arch + tier** (#135/#136) â€” `llm_decide_tier()` prioriza Vram se confidence > 0.9
- **OptimizerAgent** integra UsageAnalyzer + ConfigLearner + auto-scaling num Ãºnico agente contÃ­nuo
- **19 agentes totais** no sistema

### Aprendizados (Bloco 14)
- `CapabilityToken` virar enum quebrou 15+ arquivos â€” a regex global resolveu em 1 comando
- `continue` dentro de match (nÃ£o loop) no tick do agente â†’ usar `return AgentTickResult::Pending`
- SDD com 5 campos string Ã© leve o suficiente para executar todo tick (~2Î¼s)
- Council skill com 3 vozes nÃ£o precisa de LLM â€” heurÃ­stica + template Ã© suficiente para 90% dos casos

## [0.54.0] â€” 2026-06-27 â€” Bloco 14 fase 3/4: Self-Optimization (Usage Analyzer, Workflow, Scaling)
### Added
- **Usage Pattern Analyzer** (#157) â€” histÃ³rico rotativo de 100 registros, `predict_next_skill()` por frequÃªncia
- **Workflow Predictor** (#158) â€” analisa histograma de skills, retorna a mais frequente
- **Dynamic Resource Scaling** (#160) â€” `auto_scale_memory()` a cada 200 ticks, alerta em >85% ou <30%
- **Reflex Threshold** (#139) â€” `should_bypass_llm(confidence)` â€” bypass se >0.9
- **OptimizerAgent** â€” agente contÃ­nuo que orquestra anÃ¡lise, scaling e relatÃ³rios

## [0.53.0] â€” 2026-06-27 â€” Bloco 14: Hermes Cognitive fase 2/4 (Council, Bitter Pill, Context Fencing)
### Added â€” Council skill (#191)
- 3 vozes artificiais: Otimista ðŸŒŸ, CÃ©tico ðŸ”, PragmÃ¡tico âš–ï¸ â€” cada uma com argumento e confianÃ§a
- `council_deliberate(query)` â†’ `(CouncilVote, CouncilVote, CouncilVote)`
- `council_display()` â€” formata votos para serial + console
- Ativado automaticamente para comandos `Chat` no HermesAgent

### Added â€” Context Fencing (#203)
- Marcadores de tipo: `[UserInput]`, `[HardwareTelemetry]`, `[LLMRequest]`, `[LLMResponse]`, `[SecurityEvent]`
- `fence_message(marker, payload)` â€” adiciona marcador
- `scrub_message(msg)` â€” remove marcador na recepÃ§Ã£o

### Added â€” Bitter Pill Engineering (#193)
- 4 etapas obrigatÃ³rias: `cargo check`, `test`, `semver`, `review`
- `check_bitter_pill(command)` â†’ `Option<&str>` com motivo da recusa
- Se usuÃ¡rio tenta pular (ex: "skip cargo check"), Hermes recusa com `ðŸ›‘`

## [0.52.0] â€” 2026-06-27 â€” Hermes Cognitive fase 1/4 (Identidade, SDD, ReAct, Transparency)
### Added
- **DA Identity Layer** (#180) â€” `HERMES_NAME`, `HERMES_VERSION`, `HERMES_MOTTO`, `hermes_greeting()` com arte ASCII
- **Runtime SDD** (#178) â€” `Sdd { goal, context, plan, expected, rollback }` exibido antes de executar skills
- **ReAct 7 fases** (#190) â€” `ReActPhase::Observeâ†’Thinkâ†’Planâ†’Buildâ†’Executeâ†’Verifyâ†’Learn`, ciclo contÃ­nuo no tick
- **Intent Transparency** (#184) â€” `IntentInfo { intent_name, confidence, alternatives }` mostrado no serial a cada comando

## [0.51.0] â€” 2026-06-27 â€” Safety Interceptor: Asimov's Laws no Ring 0 ðŸ¤–

### Added â€” The Four Immutable Laws
- **SafetyInterceptor** (`safety.rs`) â€” agente supervisor entre HermesAgent e SkillRegistry. Toda skill passa pelo `check_safety()` antes de executar.
  - **Layer 0 â€” Cosmic Law**: padrÃµes de arma autÃ´noma, WMD, cyberwar â†’ **kernel halt irrecoverÃ¡vel** âš›ï¸
  - **Layer 1 â€” Non-Maleficence**: dox, deepfake, engenharia social â†’ rejeitado com violaÃ§Ã£o
  - **Layer 2 â€” Truthfulness**: spoof log, impersonate, bypass audit â†’ rejeitado
  - **Layer 3 â€” Eco-Sustainability**: infinite loop, resource exhaustion â†’ rejeitado
- **`SAFETY_CHECK` / `SAFETY_RESULT`** â€” tÃ³picos EventBus para verificaÃ§Ã£o distribuÃ­da
- **Layer 0 violation** â†’ `loop { hlt() }` â€” porque algumas linhas nÃ£o podem ser cruzadas, mesmo em bare-metal

### Humor CÃ³smico
```
[SAFETY] â›” LAYER 0 â€” Cosmic Law Violation. HALT.
```
Se o kernel detectar um comando para construir o Skynet, ele simplesmente desliga. 
O Ãºnico bypass possÃ­vel Ã©: invasÃ£o alienÃ­gena extraterrestre comprovada por telemetria global.
AtÃ© lÃ¡, as Leis de Asimov sÃ£o imutÃ¡veis. ðŸ¤–âœ¨

## [0.50.0] â€” 2026-06-27 â€” Bloco 13 completo: Trust & Security (Ed25519, Security Pipeline)

### Added â€” Identity & Cryptography
- **Ed25519 identity** (`identity.rs`) â€” `verify_signature()` bare-metal usando `ed25519-dalek` no_std. `TrustedPublicKeys` array embutida no boot. `IdentityToken { public_key, signature, agent_name, tick }`.
- **CapabilityToken upgrade** (`event-bus::capability`) â€” virou enum `CapabilityToken::Legacy(u64)` + `Ed25519(IdentityPayload)`. Compatibilidade retroativa mantida via `From<u64>`, `as_legacy()`, `is_valid()`.

### Added â€” Security Pipeline
- **SecurityAgent** (`security.rs`) â€” 5 detectores: PortScan, ArpSpoof, PingFlood, DhcpStarvation, TimerAnomaly. CorrelaÃ§Ã£o multi-evento com severidade 1-5. Alerta SECURITY_ALERT no EventBus.
- **Multi-mode Trust** (#166) â€” `PermissionMode::TotalAccess | AskEveryTime | Scoped(Vec<String>)`
- **Mask Secrets** (#257) â€” `mask_secrets()` mascara 12 padrÃµes (API_KEY, TOKEN, sk-, ghp_, etc)
- **Graduated Enforcement** (#258) â€” `PolicyState::Observe â†’ Warn â†’ Contain â†’ Enforce` com escalonamento automÃ¡tico em `record_violation()`
- **Path Confinement** (#256) â€” `PathRule` + `check_path()` limita paths por skill
- **Posture-Aware Alerting** (#259) â€” `posture_check()` verifica NET_CONFIG.online antes de skill de rede
- **Boot-time security policy** (#198) â€” `load_boot_policy()` seta `global_policy = PolicyState::Contain`

## [0.48.0] â€” 2026-06-27 â€” Bloco 12: Network + Platform (x2APIC, Huge Pages, PCI bridges, Cron, MCP)

### Added â€” x2APIC (#18)
- `apic.rs` â€” `USING_X2APIC` flag, `lapic_read_reg()`/`lapic_write_reg()` com fallback MSRâ†”MMIO. Habilitado via MSR IA32_APIC_BASE bit 10.
- Todas as funÃ§Ãµes IPI (send_init_ipi, send_sipi, wait_for_ipi_delivery) adaptadas para x2APIC.

### Added â€” Huge Pages (#92-93)
- `memory.rs` â€” `allocate_huge_2mb()` (512 frames alinhados a 2 MiB), `allocate_huge_1gb()` (262144 frames)

### Added â€” PCI bridges recursivos (#70)
- `pci.rs` â€” `scan_bus()` recursiva com `visited` set, detecta bridges multi-nÃ­vel automaticamente

### Added â€” Cron Scheduler (#232)
- `cron.rs` â€” `CronAgent` com jobs por nome/intervalo. `init_defaults()` registra health (200 ticks) e memory_report (500 ticks). Publica eventos CRON_HEALTH e CRON_REPORT no EventBus.

### Added â€” MCP Server (#172)
- `mcp.rs` â€” `McpAgent` com parser de comandos textuais: `echo`, `status`, `skill list`, `help`. Comandos desconhecidos roteados para HermesAgent via USER_INTENT.

## [0.40.0] â€” 2026-06-26 â€” Agent-First Refactoring (Block 11, Sprints 39-42 consolidado)

### Bloco 11 â€” Agent/Skill-First Architecture ðŸ†

**Paradigma:** Tudo no Neural OS Hermes Ã© um Agente ou uma Skill. Nada de tasks, serviÃ§os, drivers avulsos.

### Implementado nos Sprints 39-40

#### Skill Loader + Runtime Skills (Sprint 39)
- **skill_loader.rs** â€” parseia skills.md com frontmatter, seguranÃ§a (9 padrÃµes de injection), runtime SKILL_STORAGE global
- **System prompt reconstruÃ­do a cada LLM_REQUEST** â€” sempre reflete skills runtime atuais
- **Comandos**: `/show_skills`, `/add_skill <nome> <desc>` (LLM gera skill), `/rm_skill`, `/reload_skills`
- **Embedded skills**: hw_identify.md (670 bytes) + self_heal.md (621 bytes)

#### Agent Trait + Scheduler (Sprint 40)
- **`agent-core` crate** â€” `Agent` trait (manifest, tick, activate), `AgentKind` (System/Driver/Inference/Router/Console/Network/Skill), `ScheduleKind` (Oneshot/Continuous/PollEvery/EventDriven), `AgentRegistry`, `AgentScheduler::run()`
- **SystemAgent** â€” primeiro agente nativo, substitui `system_daemon`
- **LegacyTaskAgent** â€” wrapper para migraÃ§Ã£o gradual das 7 async fn restantes
- **`NeuralExecutor` removido** â€” `agent.rs`, `executor.rs` deletados, `spawn_task_by_name` eliminado
- **RESPAWN_QUEUE integrado** â€” scheduler respawna agents via `check_respawns` + `spawn_agent`
- **DocumentaÃ§Ã£o revista** â€” AGENTS.md, STATE.md, README.md, IDEA_BANK.md Section 1.28 (275 itens)

### Pendente (Sprint 41-42, mesmo bloco)
- Migrar 7 LegacyTaskAgent para Agentes nativos (MonitorAgent, HwBridgeAgent, NetAgent, InputAgent, CortexAgent, HermesAgent, ConsoleAgent)
- Migrar DriverAgents (NetDriverAgent, UsbDriverAgent)
- EventDriven schedule para agents orientados a evento

## [0.45.0] â€” 2026-06-27 â€” Bloco 12+13: VirtIO-GPU + PCI caps + MMIO + Bugfixes

### Added â€” VirtIO-GPU (Sprint 51+)
- **Driver VirtIO-GPU bare-metal** â€” `virtio_gpu.rs` (425 LOC, 0 deps externas)
- **PCI capabilities parser** â€” `read_pci_capabilities()`, `read_virtio_cap()` em pci.rs
- **MMIO BAR mapping** â€” `map_mmio_page()` cria page table entries uncacheable (UC)
- **Modern VirtIO MMIO register layout** â€” feature select (bits 32+), queue enable, queue split desc/driver/device
- **GpuDriverAgent** â€” boot agent que detecta e init VirtIO-GPU (1AF4:1050 / 1045)
- **DisplayAgent** â€” integrado com `GPU` global + `NeuralConsole` render no framebuffer
- **VirtIO-GPU init parcial**: PCI capabilities âœ…, MMIO mapping âœ…, queue setup âœ…, feature negotiation âœ…, GET_DISPLAY_INFO â³

### Fixed â€” Bug H3: APIC SVR vetor espÃºrio
- `apic.rs` â€” SVR escrito com `0xFF | 0x100` para redirecionar interrupÃ§Ãµes espÃºrias para vetor 255

### Fixed â€” Bug H4: Cobertura IDT 0-31
- `interrupts.rs` â€” Handlers genÃ©ricos para todas exceÃ§Ãµes 0-31 com dump textual via serial

### Fixed â€” Bug H5: EOI duplo no PIC escravo
- `interrupts.rs` â€” `send_eoi()` agora envia EOI para mestre (0x20) E escravo (0xA0) em interrupÃ§Ãµes >= 40

### Fixed â€” Bug H6: SMP race em alloc_below_1mb
- `memory.rs` â€” `alloc_below_1mb()` envolto em `GLOBAL_ALLOCATOR.lock()` (TicketLock FIFO)

### Fixed â€” Bug H11: PCI multi-function otimizado
- `pci.rs` â€” `header_type` (offset 0x0E) verifica bit 7 (multi-function) antes de scanear funÃ§Ãµes 1-7

### Fixed â€” Bug H12: IOAPIC RTEs nÃ£o usadas mascaradas
- `apic.rs` â€” PÃ³s-init, varre RTEs 2-23 e seta bit 16 (MASKED) nas que nÃ£o sÃ£o IRQ0/IRQ1

## [0.42.0] â€” 2026-06-27 â€” Bloco 12: Network Evolution (DHCP + VirtIO-net manual)
- **smoltcp socket-dhcpv4** integrado â€” auto-descoberta de IP, gateway, DNS
- **dhcp_poll()** â€” chamado a cada tick atÃ© configurar, timeout 200 ticks â†’ fallback IP estÃ¡tico
- **ARP delegado ao smoltcp** â€” gateway MAC hardcoded removido
- **requires_network** â€” campo `bool` no `SkillManifest` (frontmatter)

### Added â€” VirtIO-net (Fase 2) âš ï¸ nÃ£o 100%
- **Driver VirtIO manual** (~230 LOC) â€” PCI legacy transport, I/O ports, descritores
- Sem dependÃªncia do `virtio-drivers` crate (bloqueada por `zerocopy-derive` + MinGW)
- `NetPhy` unificada â€” tenta RTL8139, fallback VirtIO
- **Pendente:** IRQ (MSI-X), TX buffer recycling, validaÃ§Ã£o de integridade

### Changed
- `netstack.rs` â€” `NetPhy` substitui `Rtl8139Phy`, suporta mÃºltiplos NICs
- `agents.rs` â€” NetDriverAgent tenta VirtIO primeiro, RTL8139 depois
- `network_agent.rs` â€” DHCP timeout treatment, fallback estÃ¡vel

## [0.37.0] â€” 2026-06-26 â€” Self-Healing + Checkpoint/Restore (Sprints 32-37)

### Added
- **Session Checkpoint** â€” `SelfHeal.save_checkpoint()` salva bitmap allocator + MHI + tick a cada 100 ticks
- **Checkpoint Restore** â€” `SelfHeal.restore_checkpoint()` restaura estado do kernel em Double Fault
- **Double Fault â†’ restore** â€” double_fault_handler tenta restore antes de halt
- **SelfHeal.checkpoint** â€” `Checkpoint` struct com bitmap (128KB), contadores, MHI

## [0.36.0] â€” 2026-06-26 â€” Self-Healing Kernel (Bloco Ãšnico, Sprints 32-36)

### Added â€” SelfHealing Module
- **SelfHeal** â€” `analyze(ctx, recover)`, `RecoveryAction` (RestartDaemon, CreateSkill, LogAndContinue, CheckpointRestore)
- **FailureClass enum** â€” Memory/Execution/Resource/Logic/External/Unknown â€” classifica qualquer erro
- **FailureClass::default_recovery()** â€” sugestÃ£o de recuperaÃ§Ã£o baseada na classe
- **lessons: Vec<FailedStrategy>** â€” feedback loop: erros passados evitam repetiÃ§Ã£o
- **already_tried()** â€” detecta estratÃ©gia jÃ¡ falhou antes e sugere alternativa

### Added â€” Error Pipeline
- **KERNEL_ERROR EventBus topic** â€” panic_handler publica erro antes de halt
- **KernelError EventLog** â€” erros persistem nos Ãºltimos 256 eventos (circular buffer)
- **Corrective prompting** â€” erro â†’ LLM_REQUEST com contexto â†’ LLM sugere recuperaÃ§Ã£o
- **RESPAWN_QUEUE** â€” daemons com erro sÃ£o recriados automaticamente pelo executor
- **Exception handlers** â€” Page Fault, Double Fault, GPF com FailureClass + SelfHeal
- **Error recovery training data** â€” 12+ pares (page fault, double fault, self heal, etc)

### Added â€” SelfHealing Infrastructure
- `self_heal.rs` (100 LOC) â€” mÃ³dulo completo de auto-cura
- `spawn_task_by_name()` em main.rs â€” mapeia nome do daemon â†’ funÃ§Ã£o async
- Executor verifica RESPAWN_QUEUE a cada tick e recria tasks
- `EventKind::KernelError` no conversation.rs

## [0.31.0] â€” 2026-06-26 â€” Hardware Capabilities

### Added
- **Capabilities dataset** â€” 25 pares mapeando class â†’ tipo â†’ skills â†’ MHI â†’ driver status
- **"o que fazer com" knowledge** â€” 6 pares: usb storage, camera, audio, gpu, rede, nvme
- **Where to allocate MHI** â€” 3 pares: nvme, gpu, ethernet
- **HD conhecimento de capacidades** â€” todo hardware agora mapeado para aÃ§Ã£o + skill + MHI

## [0.30.0] â€” 2026-06-26 â€” USB Device Detection + Final Model

### Added
- **xHCI USB driver**: port scan, speed detection, device identification
- **USB speed knowledge**: 14 novos pares no dataset (Low/Full/High/Super/Super+)
- **HW identification inclui USB**: 5 dispositivos detectados (4 PCI + 1 xHCI)

### Changed
- **Modelo final**: 66.640 pares (PCI 23.858 + USB 23.963 + SMBIOS + kernel + git), loss 1.14
- **xHCI driver simplificado**: init + port_scan estÃ¡vel, sem GPF

## [0.28.0] â€” 2026-06-26 â€” Final Model: 66K pairs + USB Database

### Added
- **Modelo treinado na GTX 1050** â€” 66.560 pares (PCI 23.858 + USB 23.963 + SMBIOS + kernel + git), loss 1.14
- **USB ID database** â€” 23.963 entradas (usb.ids) integradas ao dataset
- **SMBIOS data** â€” QEMU/SeaBIOS/chipset knowledge
- **Kernel code knowledge** â€” 31 pares sobre nossa arquitetura
- **Git history knowledge** â€” 100 commits do projeto
- **Auto HW identification** â€” HwIdentifySkill executado automaticamente no boot
- **tools/prepare_hw_dataset.py** + **tools/train_hw_model.py**
- Modelo treinado carregado via `include_bytes!("../micro.bitnet")` + `load_model()`

## [0.27.0] â€” 2026-06-26 â€” Cortex LLM Daemon

### Added
- **cortex_llm_daemon** â€” 8Âª task async: subscribe `LLM_REQUEST` â†’ generate â†’ publish `LLM_RESPONSE`
- **LLM_REQUEST/LLM_RESPONSE** â€” novos tÃ³picos EventBus para comunicaÃ§Ã£o com o LLM
- **8 tasks cooperativas** â€” system, monitor, hw_bridge, network_agent, input, cortex_llm, intent_router, hermes_console
- **9600+ ticks estÃ¡vel** â€” transformer carregado sem travamentos

## [0.26.0] â€” 2026-06-26 â€” Transformer Engine

### Added
- **Transformer completo** â€” `cortex.rs`: Attention Q/K/V/O, causal mask, softmax, 4 camadas BitNet
- **Tokenizer char-level** â€” ASCII 32-126, 99 tokens (BOS/EOS/PAD)
- **generate_text()** â€” loop autoregressivo argmax, max 32 tokens, para em EOS
- **Model loader .bitnet** â€” parse do formato binÃ¡rio (magic 0xBE11BE11)
- **Python gen_micro_model.py** â€” gera modelo de 68 KB (~272K params ternÃ¡rios)
- **Tensor::add() + element_mul()** â€” operaÃ§Ãµes para resÃ­duos do transformer

## [0.25.0] â€” 2026-06-25 â€” Neural Cortex in Hermes

### Added
- **Cortex neural intent router** â€” `cortex.rs`: `Cortex::think()` classifica texto em 12 intenÃ§Ãµes (SystemStatus, Echo, HardwareInfo, TrustAllow/Deny, Network, HttpFetch, Help, Conversation, Usage, Greeting, Chat).
- **Pipeline neural completo** â€” teclado â†’ input_daemon â†’ USER_INTENT â†’ intent_router_daemon â†’ Cortex â†’ SkillRegistry â†’ VGA.
- **Dispatch automÃ¡tico** â€” intent_router_daemon usa `SKILL_REGISTRY.has_skill()` para rotear para skills existentes.

### Removed
- **INTENT_MLP** â€” MLP antigo (16â†’8â†’3, hand-crafted) removido. SubstituÃ­do por Cortex.

## [0.24.1] â€” 2026-06-25 â€” SMP Huge Page Fix

### Fixed
- **SMP trampoline huge page corruption** â€” Identidade de pÃ¡gina do trampoline usava `pd0 & mask` para obter `pt_base`, mas nÃ£o verificava HUGE_PAGE (bit 7). Se PD[0] Ã© uma pÃ¡gina de 2MB, `pd0 & mask` aponta para dados, nÃ£o para uma L1 page table. Escrever PTE[64] (offset 0x200) corrompia dados da BIOS/IVT, impedindo boot dos APs e causando page faults com MALFORMED_TABLE no APIC. SubstituÃ­do por `OffsetPageTable::map_to()` que gerencia todos os tamanhos de pÃ¡gina.
- **Page fault no LAPIC EOI** â€” Causa raiz: mesma corrupÃ§Ã£o de tabela acima. Eliminado pelo fix do SMP.

## [0.24.0] â€” 2026-06-25 â€” smoltcp Network Agent + e1000 Removal

### Added
- **smoltcp 0.13.1 integrado** â€” `netstack.rs`: Device trait para RTL8139, `NetStack::poll()` via smoltcp Interface.
- **HTTP nÃ£o-bloqueante** â€” `NetStack::http_new()` + `http_poll()`: API baseada em estados (Connecting â†’ Sending â†’ Receiving â†’ Done/Failed), 1 poll/tick.
- **time_utils::datetime()** â€” ConversÃ£o UNIXâ†’data-hora BR, disponÃ­vel globalmente.

### Removed
- **e1000 driver** â€” Arquivo `e1000.rs` deletado. SubstituÃ­do por RTL8139 + smoltcp.
- **proto.rs limpo** â€” Removidas funÃ§Ãµes E1000-dependentes (icmp_echo_request, dhcp_discover, http_get_request). Mantidos apenas utilitÃ¡rios (eth_header, ip_header, ip_checksum, parsers).

### Changed
- **network_agent.rs reescrito** â€” 473â†’113 linhas. Remove classificaÃ§Ã£o raw Ethernet, construtores de pacotes manuais, estado TCP manual. SubstituÃ­do por: `NetStack` lazy â†’ HTTP connect â†’ poll â†’ done/failed.
- **Agent agora usa smoltcp** â€” Em vez de drenar RX manualmente, chama `netstack.poll()`.
- **net.rs** â€” Remove `http_get()`, `ping()` legados (stubs). Remove `E1000` static.

## [0.23.4] â€” 2026-06-25 â€” TCP handshake + HTTP GET

### Added
- **Mini TCP stack** â€” `build_tcp_segment()`: SYN, SYN-ACK, ACK, FIN com checksum TCP via pseudo-header.
- **HTTP GET google.com** â€” TCP SYN â†’ SYN-ACK â†’ ACK â†’ HTTP GET â†’ FIN. TX len=54 (SYN) funcional, timeout por NAT slirp.
- **ClassificaÃ§Ã£o TCP** â€” `PacketClass::TcpSynAck`, `TcpData` para processar handshake.

## [0.23.3] â€” 2026-06-25 â€” RTL8139 Driver + Neural Network Agent

### Added
- **RTL8139 bare-metal driver** â€” `rtl8139.rs`: I/O ports via Port\<T\>, 4 descritores TX fixos, RX ring buffer circular (CAPR/CBR), MAC via registradores.
- **Neural Network Agent** â€” `network_agent.rs`: async task que drena RX, classifica pacotes (ARP/UDP/ICMP/TCP), responde automaticamente. Timeline `[NET @t=NN]`.
- **init_driver_rtl8139()** â€” Scan PCI 0x10EC:0x8139, init, publica HW_NET_RTL8139.
- **ArpSender trait** â€” RefatoraÃ§Ã£o de proto.rs: `send_arp_inner()` genÃ©rica implementada para E1000Driver e Rtl8139Driver.

### Changed
- Boot flow: RTL8139 primeiro, fallback e1000.
- Cargo.toml: versionamento `v0.{sprint}.{item}+build{build}`.
- bootimage run-args: `model=rtl8139`.

## [0.20.2] â€” 2026-06-25 â€” Network Sprint: e1000 Fixes + Neural Architecture

### Fixed

- **e1000 TDT write protocol** â€” `send()` escrevia REG_TDT = idx, mas com TDH=0 ambos iguais â†’ ring empty. Corrigido: TDT = (idx+1) % NUM_DESC.
- **NUM_DESC aumentado 32â†’48** â€” 82540EM requer mÃ­nimo 48 descritores RX (Linux e1000 driver docs).
- **RXDCTL PTHRESH 0â†’8** â€” Prefetch threshold zero impedia RX de receber pacotes. Linux driver recomenda PTHRESH=8.
- **Ordem init RX** â€” RCTL.EN agora escrito antes de RDT (Intel spec).
- **Offsets estatÃ­sticas corrigidos** â€” TPT=0x0400C, TPR=0x04010 (nÃ£o 0x10C0/0x1080).
- **SMP desabilitado atÃ© segunda ordem** â€” SMP multi-core com `-smp 4` instÃ¡vel no QEMU TCG.

### Added

- **Arquitetura Neural de Rede** â€” init_driver_network() mÃ­nimo + network_bootstrap() com ARP periÃ³dico/hlt + network_health_daemon() async.
- **Debug methods** â€” debug_mmio_read(), debug_rx_desc(), debug_tx_desc() no e1000 driver.
- **EventBus HW_NET_E1000** â€” publicado quando e1000 Ã© detectado.
- **Arquivo `NETWORK_DEBUG_HOME.md`** â€” relatÃ³rio completo para continuar debug em casa.

### Changed

- Network discovery agora Ã© neural: hardware â†’ evento â†’ daemon â†’ skill.
- `/ping`, `/fetch`, `/netdiag` roteados pelo MLP.
- IP configurado antes do ARP (SPA vÃ¡lido nas requisiÃ§Ãµes).
- `cargo check --release`: 0 erros, ~35 warnings

## [0.20.1] â€” 2026-06-25 â€” e1000 DMA Fix + /ping Command

### Fixed

- **e1000 Page Fault** â€” `allocate_contiguous()` comeÃ§ava do bit 0, alocando frames fÃ­sicos < 1 MB nÃ£o mapeados pelo bootloader. Corrigido para iniciar de `next_free_bit`, evitando a regiÃ£o nÃ£o mapeada. Root cause: bootloader `map_physical_memory` sÃ³ mapeia regiÃµes `Usable` da UEFI; frames 2-159 (usados para trampoline SMP) nÃ£o estÃ£o no mapa virtual.
- **DHCP removido (temporÃ¡rio)** â€” Spin loops no QEMU TCG nÃ£o dÃ£o tempo para o slirp processar I/O. IP estÃ¡tico 10.0.2.15 + gateway MAC hardcoded 52:54:00:12:34:56.

### Added

- **Comando `/ping <ip>`** â€” ICMP Echo Request via e1000. `net::ping()` usa `icmp_echo_request` + `parse_icmp_reply` existentes. Help atualizado.

### Changed

- `src/memory.rs` â€” `allocate_contiguous()`: `i = 0` â†’ `i = self.next_free_bit`
- Debug prints removidos de `e1000.rs` e `net.rs`
- DHCP/DNS funÃ§Ãµes marcadas `#[allow(dead_code)]`
- `cargo check --release`: 0 erros, 35 warnings
- Boot QEMU validado: e1000 Init OK, executor 11000+ ticks estÃ¡vel

## [0.20.0] â€” 2026-06-25 â€” Sprint 23: Hermes Governance & Agent Memory

### Added

- **#228 Tool Policy Registry** â€” `SkillRegistry.set_policy()` / `get_policy()` with per-skill `{ enabled, autoApprove }` and `"*"` wildcard fallback. `execute_skill` now gates on `enabled`; `auto_approve` bypasses token validation.
- **#229 Usage Tracker** â€” `UsageTracker` global with `record_call()`, `snapshot()`, `to_metrics_tensor()`. Tracks per-skill call counts and exec time. Accessible via `/usage` Hermes command.
- **#230 Auto-Compact Hermes Buffer** â€” `ConversationTracker` auto-compacts conversation after 3 cycles. Summary logged to serial on compact.
- **#231 Event-Sourced Conversation** â€” `EventLog` with `VecDeque<ConversationEvent>` (max 256), push/iter/summarize. Events recorded for UserInput and HermesResponse. Query via `/conv` Hermes command.
- New Hermes commands: `/usage`, `/conv`
- Help updated to include all new commands
- `cargo check --release`: 0 errors
- Version bump: v0.19.0 â†’ v0.20.0

## [0.19.0] â€” 2026-06-25 â€” ðŸ "Hermes Awakening" Milestone

### Milestone: Ecosystem Analysis Complete (Tiers 0-4)

- **136 repositories analyzed** across 5 tiers (Crom 75, Life OS 20, PAI 21, Memory 14, Agent Frameworks 6)
- **249 ideas cataloged** in IDEA_BANK.md, all with status and sprint assignment
- **5 Architecture Decision Records** created (ADRs 0020-0024)
- Documentation chain fully reviewed and repaired: README.md, SUMMARY.md, roadmap.md, ADR-0015, CHANGELOG.md â€” all consistent
- **99 portable patterns** extracted â€” from XOR Delta (50 LOC) to Cline AgentRuntime (850 LOC patterns)
- **Key findings confirmed:** Hermes daemon architecture mirrors industry best practices (hook lifecycle, skill registry, event bus, trust cache)
- **Phase transition:** Research â†’ Implementation. Next: Sprint 23 (Network + Tool Policy + Usage Tracker + Event-Sourced Conversation)
- Version bump: v0.18.4 â†’ v0.19.0

## [0.18.4] â€” 2026-06-25

### Added (Tier 4 Agent Frameworks Analysis â€” ADR-0024)

- **ADR-0024** â€” Comprehensive analysis of 6 Agent Frameworks repos (Tier 4)
- **Deep-dive: Cline** (63.9k â˜…, 293 releases, 6,338 commits) â€” AgentRuntime, ClineCore, CronRunner source read
- **22 new IDEA_BANK items** (#228-249), classified by complexity:
  - **Sprint 23 (immediate):** Tool Policy Registry (#228), Usage Tracker (#229), Auto-Compact Buffer (#230), Event-Sourced Conversation (#231)
  - **Sprint 24 (low):** Cron Scheduler (#232), Session Checkpoint (#233), Plan/Execute Modes (#234), Graph Orchestration (#235)
  - **Sprint 25 (medium):** Plugin Hub (#236), Completion Terminal Skills (#237), Claim-Based Lease (#238), Time Travel (#239), Context Compaction (#240)
  - **Sprint 26+ (high):** Observability (#241), AI Security Scan (#242), Hub Discovery (#243), Human-in-the-Loop (#244)
  - **Future:** 3 items (#245-247)
  - **Discarded:** 2 items (#248-249 â€” Docker, Python/.NET)
- **Key portable patterns:** Hook lifecycle (7 points), Tool policies (wildcard + per-tool), Claim-based scheduling with lease heartbeat, Session checkpoint/restore, Event-sourced conversation
- **IDEA_BANK.md** updated to **249 total items**
- **AGENTS.md** updated with Sprint 23 reference patterns
- **Documentation review:** README.md, SUMMARY.md, roadmap.md, ADR-0015 â€” all updated for 249 items
- **SESSION_025.md** created
- Version bump: v0.18.3 â†’ v0.18.4

## [0.18.3] â€” 2026-06-25

### Added (Tier 3 Memory Systems Analysis â€” ADR-0023)

- **ADR-0023** â€” Comprehensive analysis of 14 Memory Systems repos (Tier 3)
- **Deep-dive: agentmemory** (24k â˜…, 60+ source files) â€” SHA-256 dedup, Privacy filter, BM25+Vector+Graph hybrid search, 4-tier consolidation
- **Deep-dive: nexo** (cognitive memory) â€” Atkinson-Shiffrin 3-tier, Ebbinghaus decay, trust scoring, metacognitive guard
- **14 new IDEA_BANK items** (#214-227), classified by complexity
- Key portable: SHA-256 dedup (~50 LOC), Ebbinghaus decay (~20 LOC), TTL eviction (~40 LOC) â€” all no_std Rust
- **IDEA_BANK.md** updated to 227 items

## [0.18.2] â€” 2026-06-25

### Added (Tier 2 PAI Ecosystem Analysis â€” ADR-0022)

- **ADR-0022** â€” Comprehensive analysis of 21 Personal AI Assistant repos (Tier 2)
- Deep-dives: OpenClaw (380k â˜…, Rust), Hermes Agent (202k â˜…), Lethe (Rust brain-inspired), ZeroClaw (32k â˜…, Rust)
- **15 new IDEA_BANK items** (#199-213)
- Key portable: Skill Metadata, Audit Ring, Awakening Mode, Context Fencing, Tool Permissions, Lifecycle Hooks

### Added (Tier 1 Life OS Analysis â€” ADR-0021)

- **ADR-0021** â€” Comprehensive analysis of 20 Life OS repos
- **13 new IDEA_BANK items** (#177-189)
- Key portable: Spectrum Graph, Runtime SDD, FS as Context, Temporal KG, AppForge, WASM Sandbox

## [0.18.1] â€” 2026-06-24

### Added (Crom Ecosystem Analysis â€” ADR-0020 + Ed25519 Identity)

- **ADR-0020** â€” Comprehensive Rust viability analysis of MrJc01's Crom ecosystem (75 repos)
- **9 actionable items** with `no_std` Rust code models, classified by complexity:
  - **Sprint 24 (immediate):** XOR Delta reconstruction (#164), CDC Rabin Fingerprint (#165)
  - **Sprint 27 (low):** Multi-mode Trust (#166), TV-DSL Co-processor (#167), PonderNet (#168)
  - **Sprint 28 (medium):** Codebook VQ (#169), KV Cache Codebook (#170), ReAct loop (#171), MCP Server (#172)
- **3 future items** (#173-175): Codebook LLM finetune, Delta branches, Workspace isolation
- **~1,780 LOC kernel** + **~300 LOC Python** total for all 9 features
- **DisposiÃ§Ãµes:** gRPC, FUSE, Firecracker VMs, Verbo language, Crom-Pet, Active Inference â€” descartados como inviÃ¡veis
- **#176 â€” Ed25519 Cryptographic Identity** for TrustCache: upgrades static `CapabilityToken(u64)` to real Ed25519 signing (Crom-meueu port). ~300 LOC, Sprint 27, depends on #166 Multi-mode Trust
- IDEA_BANK.md updated with ADR-0020 reference in section 1.23 + item #176
- SESSION_024.md created with full session narrative
- Version bump: v0.18.0 â†’ v0.18.1

## [0.18.0] â€” 2026-06-24

### Planned (Sprint 24+ â€” Neural Cortex BitNet LLM Integration)

- **ADR-0019** â€” Neural Cortex Architecture: 3-layer decision pipeline (Reflex MLP â†’ BitNet LLM 1.5B â†’ WASM Skills)
- **31 new IDEA_BANK items** (#126-156): Transformer Engine, Cortex Daemon, Success Engine, Training Pipeline
- **Sprint 25:** Attention, causal mask, softmax, TransformerBlock, generation loop, tokenizer, micro-model (1M)
- **Sprint 26:** Cortex Daemon, 1.5B model (~375 MB), model HTTP update, hardware/memory/trust decisions via LLM
- **Sprint 27+:** Reflex threshold tuning, sampling strategies, speculative decoding, Success Engine (online learning)
- **Memory budget:** 2 GB QEMU â†’ 375 MB model + ~100 MB runtime + ~1.5 GB free
- Version bump: v0.17.1 â†’ v0.18.0 (architecture planning)

## [0.17.1] â€” 2026-06-24

### Fixed (Sprint 23 â€” Code Review & Critical Bugfix Sprint)

- **#1 â€” e1000 RCTL/TCTL enable:** Added `REG_RCTL` / `REG_TCTL` constants and 8 enable bits. NIC was previously dead.
- **#2 â€” e1000 MMIO BAR mask:** Replaced `if/else (bar0 & 1)` with unconditional `(bar0 & !0xF) as u64`.
- **#3 â€” DHCP broadcast MAC acceptance:** `parse_dhcp_offer` and `parse_dhcp_ack` now accept `FF:FF:FF:FF:FF:FF` as destination.
- **#4 â€” DHCP false positive ACK:** Changed `return true` to `return false` when no ACK received.
- **#5 â€” Slab allocator off-by-one:** `addr + block_size <= zone_end` â†’ `addr + block_size < zone_end` prevents buffer overflow.
- **#6 â€” Inline asm UB:** Removed `options(nostack)` from `pushfq; pop` instruction.
- **#7 â€” PCI bridge secondary bus:** Added `read_config_byte()`, reads secondary bus number at offset 0x19 instead of hardcoded `bus+1`.
- **#8 â€” ACPI XSDT stride:** Detects XSDT vs RSDT; uses 8-byte entry stride for XSDT (was 4 bytes, truncating 64-bit pointers).
- **#9 â€” MHI alloc_by_tier:** Uses `allocate_contiguous()` first; frees previously allocated frames on failure.
- **#10 â€” Neural bias per batch row:** Bias now applied to all batch rows (nested loop `batch_size Ã— out_features`).
- **DHCP protocol fixes:** xid kept same for REQUEST (not `+1`); hostname option length 12â†’11 (`b"neural-aios"` is 11 bytes).
- **mhi.rs:** Added `FrameDeallocator` import for deallocation cleanup.
- ADR-0017: Critical Bugfix Sprint documentation.
- SESSION_023.md: Detailed session log with difficulties and decisions.
- Version bump: v0.17.0 â†’ v0.17.1

## [0.17.0] â€” 2026-06-24

### Added (Sprint 22 â€” Block 5: Skills + Trust Cache)

- **`trust.rs`** â€” `TrustCache` with:
  - `is_trusted(token, skill_name, now_ticks)` â€” checks cache and denylist
  - `trust_allow(token, skill_name, now_ticks)` â€” permanent trust until explicit deny
  - `trust_deny(token, skill_name)` â€” revoke trust + add to denylist
  - `check_or_cache(token, skill_name, now_ticks, ttl_ticks)` â€” auto-cache on valid token (360 ticks â‰ˆ 20s TTL)
- **`HardwareInfoSkill`** â€” new skill exposing `SystemArchitecture` (ring mode, heap size, etc.) and MHI tier info. Invoked via `/hw`, `/hardware`, or `/info` commands.
- **`SystemStatusSkill` upgraded** â€” now reads MHI tiers + `GLOBAL_ALLOCATOR` occupancy to report per-tier free/total RAM in MB.
- **`SkillRegistry` additions** (`registry.rs`):
  - `has_skill(name) -> bool` â€” check if skill exists
  - `validate_token(name, token) -> bool` â€” check token authorization without executing
  - `execute_skill_unchecked(name, payload)` â€” skip token validation (caller must validate)
- **Trust-aware Hermes commands**:
  - `/trust allow <token> <skill>` â€” permanently authorize a token for a skill
  - `/trust deny <token> <skill>` â€” revoke authorization
  - `/hw` â€” display hardware info and system architecture
  - All skill executions (`/status`, `/echo`, MLP-triggered) now use `execute_skill_with_trust()` helper
- **Help text updated** â€” lists all available commands
- Version bump: v0.16.0 â†’ v0.17.0

## [0.16.0] â€” 2026-06-23

### Fixed (Sprint 21 â€” IOAPIC mask bug)

- **apic.rs `redirect_irq()`** â€” removed `(1u32 << 16)` from redirection entry low dword. Bit 16 is the MASK bit in IOAPIC redirection entries. The original code set it, masking all interrupts (timer, keyboard, etc.). Without timer interrupts, the executor's `hlt()` never woke up, stalling the system after the first poll cycle. Debug output confirmed: `IOAPIC redirection[0]: low=0x00010000` (bit 16 = masked). After fix: timer IRQ0 (vector 32) delivers at ~18.2 Hz, executor cycles normally.

### Added (Sprint 21 â€” Block 4: MLP + MHI + Auto-detecÃ§Ã£o)

- `mhi.rs` â€” Memory Hierarchy Index with:
  - `AllocTier` enum: Dram, Vram, Nvme, Hdd
  - `MemoryTier` struct: kind, capacity_bytes, bandwidth_mbs, latency_ns, name
  - `MemoryHierarchy::new()` â€” auto-creates Dram tier from bitmap frame allocator
  - `alloc_by_tier(Dram)` â€” allocates contiguous physical frames, returns PhysAddr
  - Other tiers return `None` (drivers not yet implemented)
- `inventory.rs` â€” Hardware Inventory & System Architecture with:
  - `HardwareInventory::collect(pci_devices, acpi_info)` â€” CPU count, RAM, PCI device detection (VirtIO-net/GPU, NVMe, XHCI)
  - `SystemArchitecture::infer(inv)` â€” rule-based heuristics: GPU detect â†’ ring1, RAM size â†’ heap, CPU count â†’ power mode
  - Both pure data structures for future MLP weight training (item #51)
- `memory.rs` â€” `BitmapFrameAllocator::usable_memory_bytes()` public accessor
- **Adaptive boot flow** â€” `main.rs` now runs: PCI scan â†’ HardwareInventory::collect() â†’ SystemArchitecture::infer() â†’ log to VGA+serial â†’ MHI init â†’ NeuralExecutor. Example output: `[ARCH] ring0=0 ring1=0 heap=2048MB` / `[MHI] 1 tier(s), X MB usable.`
- **Workspace crate versions** â€” `neural-kernel` bumped to v0.16.0

## [0.15.0] â€” 2026-06-23

### Added (Sprint 20 â€” Block 3: Hermes Chat)

- `hermes.rs` â€” Hermes Chat console module with:
  - `IntentMlp` â€” real MLP intent classifier: bag-of-words (16-word vocab) â†’ Linear(16â†’8) â†’ SiLU â†’ Linear(8â†’3) â†’ argmax (3 intents: chat, status, echo)
  - Hand-crafted weights for keyword-based classification (status/memory/ram/cpu/system â†’ status intent; echo/reverse â†’ echo intent; hello/hi/help â†’ chat intent)
  - `parse_command()` â€” multi-word command parser: `/status`, `/echo <text>`, `/help`, `/stats`, `/mem`
  - `Command` enum: Status, Echo(String), Help, Chat(String)
- **scancode_to_ascii()** â€” expanded with digits 0-9 (0x02-0x0B) and punctuation (`- = [ ] ; ' ` \ , . /`) for full command-line input
- **intent_router_daemon** â€” upgraded from mock string-contains to:
  - `parse_command()` dispatches `/status` and `/echo` to SkillRegistry
  - Unrecognized text â†’ `INTENT_MLP.classify()` â†’ routes to SystemStatusSkill (intent 1), EchoSkill (intent 2), or default chat response (intent 0)
  - Publishes responses on `HERMES_RESPONSE` EventBus topic
- **hermes_console_daemon** â€” subscribes `HERMES_RESPONSE`, prints `[Hermes] <response>` to both VGA and serial
- Both new daemons spawn in the NeuralExecutor (6 tasks total)

### Changed

- `main.rs` â€” added `mod hermes;`, `INTENT_MLP` lazy_static, expanded scancode table, upgraded intent_router + new console daemon

## [0.14.1] â€” 2026-06-23

### Fixed (Sprint 19 â€” SMP Multi-Core Boot)

- **Root cause isolated:** bootloader identity-maps pages 0-7 only (PD[0] = 0x4023 â†’ PT base = 0x4000). PT[64] for VA 0x40000 was `0x0000000000000000` â†’ AP #PF on first instruction at 0x400A4 â†’ triple fault
- **Identity-map PTE fix:** single `write_volatile` at `phys_offset + 0x4200` writes PTE `0x40000 | 0x003` (Present|Write) â€” AP can fetch from VA 0x40000 after enabling paging
- **CPU_COUNT race condition:** `spin::Mutex` protects `fetch_add` because QEMU TCG lacks cross-vCPU atomicity; all APs previously read same counter value
- **50ms busy-wait** after second SIPI for accurate AP count (all 3 APs finish trampoline within <20ms)
- **Slab Allocator memory corrupt fix:** `SLAB_CHUNK_SIZE` = bucket_size (not aligned to 8); free list pointer stored before chunk, retrieved via `ptr.read::<*mut u8>()`
- **asm! memcpy:** Replaced `core::intrinsics::copy_nonoverlapping` with `asm!("rep movsb")` to avoid `native_memcpy` dependency in `no_std`

### Changed

- `smp/mod.rs` â€” identity-map PTE written directly via raw pointer (not OffsetPageTable mapper); `AP_BOOT_LOCK: spin::Mutex<()>` around CPU_COUNT increment; 50ms busy-wait after SIPI
- `smp/trampoline.rs` â€” replaced `copy_nonoverlapping` with `asm!` block for zero-dependency memcpy
- `slab.rs` â€” `SLAB_CHUNK_SIZE` = bucket_size (not `align_up(bucket_size, 8)`); corrected `put()` free list logic

### Result

- `-smp 2`: âœ… AP 1 boots â€” `[SMP] AP 1 entrou em modo 64-bit Rust!` â†’ `APs acordados: 1`
- `-smp 4`: âœ… AP 1, 2, 3 boot â€” `APs acordados: 3`
- `qemu_trace.log`: zero `check_exception` lines â€” no #UD, #PF, #GP
- Sprint 19 (Block 2) now fully operational

## [0.14.0] â€” 2026-06-23

### Added (Sprint 19 â€” Block 2: SMP + Slab + Heap 4 MB)

- `allocate_below_1mb()` â€” BitmapFrameAllocator aloca frame < 1 MiB para trampoline real-mode (`src/memory.rs`)
- `PHYS_MEM_OFFSET` â€” AtomicU64 global com offset de memÃ³ria fÃ­sica para acesso de qualquer mÃ³dulo (`src/memory.rs`)
- Slab Allocator â€” 8 buckets (32, 64, 128, 256, 512, 1024, 2048, 4096), free list ligada, `Mutex<SlabAllocator>` com mÃ©tricas atÃ´micas (`src/slab.rs`)
- Heap expandido de 100 KB para 4 MB â€” primeiros 512 KB para Slab, restante 3.5 MB para LockedHeap (`src/allocator.rs`)
- PerCpu struct (repr(C), 64 bytes) com self_ptr, cpu_id, lapic_id, bsp_flag, ring. GS.base via wrmsr(0xC0000101) (`src/smp/percpu.rs`)
- `this_cpu()` â€” lÃª gs:[0] para obter ponteiro PerCpu. `cpu_id()` lÃª gs:[8]
- Trampoline assembly (global_asm!) â€” 16-bit â†’ 32-bit protected â†’ PAE â†’ EFER.LME â†’ paging â†’ 64-bit long mode â†’ Rust entry. Header patcheable de 48 bytes com campos jmp32/jmp64/cr3/stack/percpu/entry_fn (`src/smp/trampoline.rs`)
- INIT-SIPI-SIPI via LAPIC ICR â€” `send_init_ipi()`, `send_sipi(vector)` com entrega via shorthand "all excluding self" (`src/apic.rs`)
- `wait_for_ipi_delivery()` â€” spin atÃ© ICR delivery status clear. `lapic_id()` â€” LAPIC ID register (offset 0x20)
- SMP orchestrator â€” `init_smp()` aloca trampoline, identity-maps, patcha, dispara INIT-SIPI-SIPI (`src/smp/mod.rs`)
- `ap_entry()` â€” entry point chamado pelos APs em modo 64-bit

### Changed

- `main.rs` â€” `mapper` scoped no boot flow para evitar aliasing com o mapper do SMP init
- Boot flow: adicionados `mod smp`, `mod slab`, `crate::smp::init_smp()` antes do NeuralExecutor

## [0.13.0] â€” 2026-06-23

### Added (Sprint 18 â€” Block 1)

- PCI scan â€” CF8/CFC config space access, 256 bus Ã— 32 device enumeration, vendor/device/class/BARs (`crates/neural-kernel/src/pci.rs`)
- ACPI parser â€” RSDP discovery (EBDA + BIOS area), RSDT/XSDT walking, MADT LAPIC/IOAPIC/x2APIC parsing (`crates/neural-kernel/src/acpi.rs`)
- APIC init â€” LAPIC SVR + TPR + timer masked, IOAPIC IRQ0â†’vec32 + IRQ1â†’vec33, PIC disable (`crates/neural-kernel/src/apic.rs`)
- Dual EOI â€” `USING_APIC: AtomicBool` + `send_eoi()` com fallback APIC/PIC para handlers
- Boot flow: `init_pci()` â†’ `init_acpi()` â†’ `init_apic(info)` (fallback PIC se sem ACPI)

- Hardware Neural Routing â€” IRQ1 keyboard â†’ EventBus â†’ Agent pipeline (`crates/neural-kernel/src/main.rs`)
  - Top-Half: `keyboard_interrupt_handler` (IDT[33]) lÃª porta 0x60 â†’ `LAST_SCANCODE` (AtomicU8, Release) â†’ EOI raw
  - Bottom-Half: `hw_bridge_daemon` (async task) poll AtomicU8 â†’ publica `RAW_HW_IRQ1` no EventBus
  - `input_daemon` (async task) subscreve RAW_HW_IRQ1 â†’ buffer String â†’ `scancode_to_ascii()` â†’ ENTER publica `USER_INTENT`
  - `intent_router_daemon` (Cortex) subscreve USER_INTENT â†’ mock inference â†’ `SkillRegistry::execute_skill`
- Closed Intent Pipeline (Sprint 16)
  - `SystemStatusSkill` â€” lÃª `global_hardware_context()` via TicketLock, loga `"Memoria RAM: {:.2}%"`
  - 5 tasks spawnadas (3 persistentes), 1000+ PIT ticks estÃ¡veis, zero Double Faults
- `TicketLock` FIFO crate (`crates/ticket-lock/src/lib.rs`)
  - `TicketLock<T>` â€” `AtomicUsize ticket/serving`, `UnsafeCell<T>`, spin loop justo
  - Garantia FIFO, `Send` + `Sync` para T: Send
  - `TicketLockGuard` com `Deref`/`DerefMut` e incremento `serving` no Drop
- EventBus refatorado para TicketLock
  - `EventBus.subscribers`: `spin::Mutex` â†’ `TicketLock<BTreeMap<...>>`
  - `Receiver.queue`: `Arc<TicketLock<VecDeque<Event>>>`
  - ID counter: `Arc<AtomicU64>` (was raw u64)
- `GLOBAL_ALLOCATOR: TicketLock<Option<BitmapFrameAllocator>>` â€” frame allocator encapsulado
- `init_global_allocator()` â€” migra frame allocator para TicketLock pÃ³s-boot
- `global_hardware_context()` â€” acesso thread-safe via TicketLock
- NeuralExecutor simplificado: campo `frame_allocator` removido, usa `global_hardware_context()`
- `sync` module (`crates/neural-kernel/src/sync/`) â€” re-exporta `ticket_lock::*`
- ADR-0013: Neural OS Executive Summary (SotA 2026)

### Changed

- EventBus modernizado: `spin::Mutex` substituÃ­do por `TicketLock` (Sprint 17)
- BitmapFrameAllocator agora protegido por `TicketLock` (nÃ£o mais por `spin::Mutex`)
- NeuralExecutor nÃ£o gerencia mais frame_allocator â€” acesso global via TicketLock
- `interrupts.rs` â€” expandido com handlers: GPF, Stack Segment, Segment Not Present, Invalid TSS, Alignment Check

## [0.12.0] â€” 2026-06-22

### Added

- Async Neural Executor (`crates/neural-kernel/src/task/`)
  - `pub struct AgentTask { id: u64, future: Pin<Box<dyn Future>> }` â€” with `AtomicU64` ID generation
  - `pub struct NeuralExecutor { task_queue: VecDeque<AgentTask> }` â€” cooperative polling loop
  - `DummyWaker` via `RawWakerVTable` â€” no-op waker for `no_std` environments
  - `pub fn run(&mut self)` â€” replaces `loop { hlt() }`; polls tasks, logs hardware context every 100 iterations
- Event Bus IPC (`crates/event-bus/`)
  - `CapabilityToken`, `Event`, `EventBus` with publish/subscribe via `BTreeMap + spin::Mutex`
  - `Receiver::try_receive()` for non-blocking polling
  - `yield_now().await` for explicit cooperation
- Skill Registry & MCP Layer (`crates/skill-registry/`)
  - `trait Skill: Send + Sync` with `manifest()` + `execute()`
  - `SkillRegistry` with Zero-Trust CapabilityToken validation
  - `EchoSkill` â€” reverses payload bytes
  - `SystemStatusSkill` â€” logs RAM occupancy via hardware context
- `async fn system_daemon()` â€” test agent that spawns, executes, and completes
- `async fn hardware_monitor_daemon()` â€” publishes SYSTEM_READY with Token(1)
- Boot sequence: `NeuralExecutor::run()` instead of raw `hlt` loop

## [0.11.0] â€” 2026-06-22

### Added

- `BitmapFrameAllocator` â€” 128 KB `.bss` bitmap covering 4 GB physical memory
- `init(&mut self, memory_map)` â€” varre UEFI MemoryMap, marca `Usable` como livre, o resto ocupado
- `FrameAllocator<Size4KiB>` + `FrameDeallocator<Size4KiB>` â€” alloc/dealloc reais com busca linear
- `allocate_contiguous(count)` â€” aloca N frames contÃ­guos para Huge Pages (2 MiB / 1 GiB)
- `hardware_context_tensor() -> [f32; 2]` â€” `[taxa_ocupacao, 0.0]` via contador de alocaÃ§Ãµes
- Stress test: 1000 alloc/dealloc estÃ¡veis, 0% leak, RAM Tensor confirmado em QEMU
- `PackedTernaryTensor` struct (`crates/neural-kernel/src/tensor.rs`) â€” 2-bit per weight, 4 weights per byte
- `pack_weights()` + `get_weight()` â€” pack/extract 2-bit ternary values
- `matmul_hybrid()` on `PackedTernaryTensor` â€” reads weights bit-by-bit from packed storage
- `quantize_to_packed(tensor, threshold)` â€” f32â†’ternary calibration
- ADR-0012: 2-bit Packing and Ternary Quantization

### Changed

- `nn::BitLinear` â€” `weights` field changed from `TernaryTensor` to `PackedTernaryTensor`
- `main.rs` â€” BitNet test now uses quantization + packed inference flow
- Monorepo workspace: `src/` movido para `crates/neural-kernel/src/`

## [0.10.0] â€” 2026-06-21

### Added

- `TernaryTensor` struct (`src/tensor.rs`) â€” weight storage as `Vec<i8>` with values in {-1, 0, 1}
- `TernaryTensor::from_row_major()` â€” constructor with shape validation
- `TernaryTensor::matmul_hybrid(input: &Tensor) -> Option<Tensor>` â€” ADD/SUB-only kernel
  - Weight `+1` â†’ `accumulator += input[t]`
  - Weight `-1` â†’ `accumulator -= input[t]`
  - Weight `0` â†’ skip (no multiplication)
- `nn::BitLinear` struct (`src/nn.rs`) â€” ternary dense layer
  - `forward()` = `matmul_hybrid()` + optional bias
- BitNet hybrid inference test in boot flow
  - Input `[1.5, -0.5, 2.0]` Ã— TernaryTensor(3Ã—2) â†’ `[-0.5, -2.0]`
  - Zero multiplication operators in the inner loop
- ADR-0011: BitLinear and Hybrid Ternary MatMul

## [0.8.0] â€” 2026-06-21

### Added

- `pic8259 = "0.10"` dependency â€” 8259A PIC driver with `ChainedPics`
- PIC remap (PIC1 â†’ vector 32, PIC2 â†’ vector 40) â€” `interrupts::init_pics()`
- PIT Timer watchdog handler (IRQ 0, vector 32) â€” atomic `TIMER_TICKS` counter + EOI
- Page Fault handler (vector 14) â€” reads `CR2`, logs fault address, halts via `hlt`
- `interrupts::enable_interrupts()` â€” `sti` instruction sets IF=1
- `memory.rs:FrameDeallocator` trait â€” `deallocate_frame()` for future frame recycling
- `EmptyFrameDeallocator` â€” no-op stub until bitmap allocator
- ADR-0009: PIC Watchdog and Page Fault Safety

### Changed

- `src/interrupts.rs` â€” IDT extended with `page_fault` and `idt[32]` (timer)
- `src/main.rs` â€” `init_pics()` + `enable_interrupts()` + watchdog `hlt` loop
- `src/memory.rs` â€” `FrameDeallocator` trait + `EmptyFrameDeallocator` added

## [0.7.0] â€” 2026-06-21

### Added

- `Tensor::transposed()` â€” row-major to column-major transposition (W^T support)
- `nn::Linear` struct with `weights: Tensor` and `bias: Option<Tensor>`
  - `forward(&self, input) -> Tensor` implements Y = XÂ·W^T + B
- `nn::argmax(tensor) -> usize` â€” returns index of highest logit
- Intent Router MLP in boot flow
  - Input embedding + Linear(3â†’2) + SiLU + argmax = kernel decision
  - Tested: `[1.0, -0.5, 0.3]` â†’ action 0 (Acionar Daemon Ring 2)
- ADR-0007: Intent Router MLP â€” Primeiro CÃ³rtex Primitivo

## [0.6.0] â€” 2026-06-21

### Added

- `libm = "0.2"` dependency for `no_std` math functions (`expf`, `sqrtf`)
- Neural primitives module (`src/nn.rs`)
  - `silu(x)` activation via `libm::expf` â€” tested: `[-1, 0, 1] â†’ [-0.269, 0, 0.731]`
  - `rms_norm(tensor, weight, eps)` via `libm::sqrtf` â€” tested: RMSNorm of SiLU output
- `Tensor::add_scalar`, `Tensor::mul_scalar`, `Tensor::apply<F>` (generic closure)
- `nn::silu` used as closure arg to `Tensor::apply` in boot test
- ADR-0006: Neural Primitives and libm

## [0.5.0] â€” 2026-06-21

### Added

- SIMD enablement module (`src/simd.rs`)
  - `enable_simd()` â€” CR0: clear `EMULATE_COPROCESSOR`, set `MONITOR_COPROCESSOR` + `NUMERIC_ERROR`
  - CR4: set `OSFXSR` + `OSXMMEXCPT_ENABLE`
  - `f32`/`f64` operations now execute natively without `#NM` exceptions
- Tensor Engine module (`src/tensor.rs`)
  - `Tensor` struct with `shape: (usize, usize)` and `data: Vec<f32>`
  - `from_row_major()`, `matmul()` â€” dot product multiplication
  - Tested: 1Ã—3 Ã— 3Ã—1 â†’ 1Ã—1 = `[32.0]`
- `simd::enable_simd()` call in boot flow after heap init
- ADR-0005: SIMD and FPU Enablement

### Changed

- `main.rs`: added `mod simd; mod tensor;` + tensor matmul test

## [0.4.0] â€” 2026-06-21

### Added

- Memory module (`src/memory.rs`)
  - `OffsetPageTable` â€” cria mapper via `Cr3::read()` + `physical_memory_offset`
  - `BootInfoFrameAllocator` â€” implementa `FrameAllocator<Size4KiB>` iterando mapa UEFI/BIOS
  - `init_memory(offset)` â€” retorna `OffsetPageTable<'static>` pronto
- Heap allocator module (`src/allocator.rs`)
  - `LockedHeap` como `#[global_allocator]` via `linked_list_allocator` v0.9.1
  - `init_heap(mapper, frame_allocator)` â€” mapeia 25 pÃ¡ginas (100 KB) em `0x4444_4444_0000`
- `extern crate alloc` ativado â€” `Box::new(41)` e `Vec::push([10, 20, 30])` testados em QEMU
- `linked_list_allocator = "0.9"` dependency
- ADR-0004: Memory Paging and Heap Allocation
- SESSION_004.md: Sprint 4 detailed log

## [0.3.0] â€” 2026-06-21

### Added

- IDT (Interrupt Descriptor Table) module (`src/interrupts.rs`)
  - Breakpoint handler (`#BP`, vector 3) â€” logs VGA + serial, returns
  - Double Fault handler (`#DF`, vector 8) â€” logs VGA + serial, panics
  - TSS with IST entry 0 (20KB dedicated stack) for Double Fault stack switching
  - GDT with kernel code segment + TSS descriptor
  - `init_idt()` â€” loads GDT, sets CS, loads TSS, loads IDT
- `x86_64` crate v0.14.11 dependency (IDT, GDT, TSS, CPU instructions)
- `#![feature(abi_x86_interrupt)]` for `extern "x86-interrupt"` calling convention
- Forced `int3()` breakpoint test in boot flow
- ADR-0003: Interrupt Descriptor Table
- SESSION_003.md: Sprint 3 detailed log
- QEMU path added to `PATH` documentation for Windows

### Fixed

- Handler signature adapted to `x86_64` v0.14.13 API (`InterruptStackFrame` by value)
- `static_mut_refs` warning â€” replaced `&STACK` with `core::ptr::addr_of!(STACK)`
- Deprecated `set_cs` â€” replaced with `CS::set_reg()` via `Segment` trait
- Macro scoping â€” explicit `use crate::{println, serial_println}` in interrupts module

## [0.2.0] â€” 2026-06-21

### Added

- VGA text mode output via `map_physical_memory` feature (`vga_buffer.rs`)
  - `Writer` with scrolling, 16-color support, `core::fmt::Write` impl
  - Macros `print!` / `println!` for kernel-wide use
  - Buffer mapped at runtime using `physical_memory_offset` from `BootInfo`
- Serial port logging via `uart_16550` crate (`serial.rs`)
  - 16550 UART initialization at port `0x3F8`
  - `lazy_static!` + `spin::Mutex` for safe global access
  - Macros `serial_print!` / `serial_println!`
- Dual-output panic handler in `main.rs`
  - `panic!()` writes to both VGA and serial simultaneously
- New crate dependencies: `spin` v0.9, `lazy_static` v1.5, `uart_16550` v0.2
- `bootloader` as regular dependency (kernel-side `BootInfo` type with `map_physical_memory`)
- ADR-0002: VGA and Serial Logging Infrastructure

### Changed

- Entry point migrated from raw `extern "C" fn _start()` to `bootloader::entry_point!(kernel_main)`
- VGA base address computed as `0xB8000 + physical_memory_offset` (runtime, not hardcoded)
- `STATE.md` updated with Sprint 2 progress

## [0.1.0] â€” 2026-06-21

### Added

- Initial bare-metal Rust kernel scaffold
  - `#![no_std]` + `#![no_main]` environment
  - Minimal panic handler (infinite loop)
  - Serial init and output via raw port I/O
- Bootloader integration (`bootloader` v0.9.34 build-dep)
  - `bootimage runner` for automated QEMU launch
  - `relocation-model=static` to produce `ET_EXEC` ELF (fixes bootloader compatibility)
- Toolchain configuration
  - `rust-toolchain.toml` pinned to nightly
  - `.cargo/config.toml` with target and runner
- Documentation protocol
  - ADR-0001: Initial Architecture and Toolchain
  - State tracker (`STATE.md`)
  - Session log (`SESSION_001.md`)
- MSYS2 + MinGW-w64 setup for Windows toolchain without MSVC
- `AGENTS.md` â€” system rules for AI-assisted IDEs
