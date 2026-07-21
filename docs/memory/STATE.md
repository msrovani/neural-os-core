# ═════════════════════════════════════════════════════════
#   STATE — neural-os-core v1.9.1 TEST — NÃO ESTÁVEL
#   ADR-0042 N1–N5 + wire N2.5→N5.7 ✅
#   ADR-0041 H4+/H5+/AS shallow ✅ PoC (SESSION_140)
#   ADR-0055 FeatureGate+SMP ✅ SESSION_141 (TCG APs=1; WHPX BSP-only)
#   Multi-model hub ✅ SESSION_142 (TinyStories/3B/GGUF-WASM/RustCoder)
#   Pista ativa: v1.9.1 TEST — BitNet 850 #PF+BPE SP32 (SESSION_162); ath10k A3 (161); TLS smoke (157)
#   SESSION_143: auditoria ideias antigas ✅
#   SESSION_144: Onda 3 — exFAT write opt-in (#417); FS agents 282e–g ✅; #418 BLOCKED lan
#   SESSION_145: Onda 4 — USB Trust #6/#12–15; #84 UAC-HW AWAITING_REAL_HW; soft-float defer
#   SESSION_146: Onda 5 — [MHI-DMA] [GDS-HW] [GPU-HW] AWAITING; #67 Vram hook; sem fake Ready
#   SESSION_147: Trilha R soft-float/VITS — pesquisa defer (sem fake hardfloat)
#   SESSION_148: Onda 6 AIRLLM-DMA AWAITING + Onda 7 NET-HW/WIFI-HW scaffold
#   SESSION_149: Onda 7 LAN RX ✅ — e1000 TX 0x3800; L3.5 PASS
#   SESSION_150: L4 DNS raw + L5 HTTP ✅ — internet smoke QEMU/WHPX
#   SESSION_151: Fecho plano Residuals ondas 0–7 (PreFlight pass_marker; WiFi AWAITING)
#   SESSION_152: Pós-LAN — net_bridge DNS/HTTP; NetFs peer; SelfUpdate HTTP; TLS BLOCKED
#   SESSION_153: Release v1.9.0 TEST (docs + tag)
#   SESSION_154: TLS pesquisa (B) opções A–D; WiFi inventário API77 + plano S0–S5
#   SESSION_155: TLS #123 probe A PASS — embedded-tls 0.19 soft-float
#   SESSION_156: TLS N4 wire — https_get; trust=unsecure
#   SESSION_157: TLS smoke PASS — google 80952 B; soft crypto cfgs; WHPX qemu64
#   SESSION_158: TLS PKI híbrido — root_learn→root_pin; pins+TOFU RAM
#   SESSION_159: WiFi S0 honesty + prep S1 DID/FAT; fw_resolve=SKIP QEMU; sem ALIVE
#   SESSION_160: WiFi pivô ath10k QCA6174 Note1050; A0–A2; iwlwifi secondary
#   SESSION_161: ath10k A3 BMI CE+LZ → fw_ready; runtime Note AWAITING
#   SESSION_162: BitNet ladder 850 — AVX2 OOB #PF fix; BPB1 SP32+MRG1; LLM-TEST BPE (coh semântica residual)
#   SESSION_163: ADR-0057 Compute Dispatch (WS-A wake multi-AP -smp4 APs=3; WS-B/C dispatch; WS-G #412) + ADR-0058 Generative Card Desktop (embedded-graphics; cards close/mover/resize; orb+HUD preservados)
#   ADR-0056 H1 runtime: unlock_dag + recipe FAT gate + HalOffer NeedsFw/Escalate (#464)
#   Gate v2.0.0: por_fazer zerado ou residual replanejado + OK maintainer
#   PreFlight: tools/preflight_wave.py · cache docs/memory/.preflight_cache/ · depends_on: lan · AWAITING_HW
#   Cadeia: k-nano → k-hal → k-ai → cortex → hermes → jarbas  (**K³CHJ**)
# ═════════════════════════════════════════════════════════

## HW real prep (2026-07-16 / SESSION_139)
- **USB unificado (recomendado):** `python tools/build_usb_unified.py --size 2048 --fat32 --build-boot -o target/usb_hw.img` (ou `build_image.py --hw --unified`). Layout: MBR **FAT32 dados `0x0C` + ESP `0xEF`** (+ GPT UEFI). Rufus DD 1 stick. Format NeuralFS em USB só com `NEURALFS_USB_FORMAT=1` (ou debug build).
- **Dois meios (opcional):** `target/uefi.img` + `python tools/build_image.py --hw` → `target/disk_hw.raw`.
- **HW Expert:** precisa só `HWEXPRT.BIN` (não precisa linux-firmware).
- **GPU/WiFi/NIC:** precisam blobs `firmware/` no FAT.
- **E2e clima QEMU:** gated — default off; `cargo nk --features weather-e2e` para HIT.
- **Log HW sem serial:** `BOOT.LOG` no volume FAT (`fat-boot-log` on); tela = `console_print` / `boot_ckpt` K0–K17 (SESSION_139). COM1 ainda útil em bancada.
- **Bootloader vendor:** `vendor/bootloader` patch BltOnly→SetMode Rgb/Bgr (Intel HD 620).
- Serial `[STATUS]`/`[HWEXPERT]`/`[GEN]`/`[TTS]`/`[BGE]` **mantidos**.
- **Pista HW:** kernel chega APIC/x2APIC; falta PLATFORM sync / USB flush em várias máquinas — ver SESSION_139.

## Roadmap Atual
**Versão:** **v1.9.1 TESTE / NÃO ESTÁVEL** (2026-07-19) — BitNet 850 generate sem #PF + BPE SP32; base v1.9.0 Pós-LAN.  
**Base:** v1.8.0 = ADR-0042 N1–N5 + wire; v1.8.6 = ADR-0041 H4+/H5+/AS + HalOffer; v1.9.0 = Residuals 0–7.  
**Runtime marco:** v1.7.2 clima PASS parcial forte+; gates N2–N5 `logs/boot_n2_20260716_131837.txt` … `logs/boot_n5_20260716_145943.txt`.  
**Gate `v2.0.0`:** pré-requisitos funcionais ✅ — **review + `por_fazer` zerado + OK explícito do maintainer**. **Não** declarar v2.0 automaticamente.  
**Cadeia canônica:** `k-nano → k-hal → k-ai → cortex → hermes → jarbas`.  
**Nota:** 1.6.0-dev absorvida por 1.7.0 (sem tag `v1.6.0`).

### Consolidação pós-v1.8.0 — v1.8.6 TEST (pós 1.8.5)
- **SESSION_142:** ModelHub multi-.bitnet (TinyStories / generator_fast 850M / generator_pro 3B) + `gguf_wasm` SkillMarket + RustCoder 2B/3B FAT; Trinity router inalterado.
- **SESSION_141 / ADR-0055:** FeatureGate + CpuFeatures/CacheTopology + SMP real. WHPX `smp=false`; TCG `-smp 2` APs=1 + CorePools; RSDP `BootInfo.rsdp_addr`; OSXSAVE/XCR0; affinity R0→R2. GPU = 0048–0050.
- **SESSION_140 / ADR-0041:** H4+ QUEUE_NOTIFY; residual MMIO→k-hal; H5+ Cap nos ports + HalOffer grant; AS shallow demo CR3. Lifecycle ADR `fazendo`.
- **Sprint Sound:** pipeline e ferramentas fechados como parcial honesto; soft-float/VITS, CTC WER, UAC iso e cutover abertos.
- **ADR-0040 / NeuralFS:** MVP aceito; mount/GPT/USB ✅; Onda 1 smokes level2+power_loss_soft; USB power-cycle ▶️ AWAITING_HW `[NRFS-HW]`.
- **ADR-0046:** AirLLM layer-wise + hot-swap ATA/Net code; DMA, stream-to-disk, K-quants e e2e grande abertos.
- **ADR-0047:** família Latent/Evolve/Probe/GPU/HMI em MVP/PoC; sem promoção indevida a produção.
- **ADRs 0048–0050:** `fazendo` — NVIDIA ACR/D2–D4; **AMD ADR-0049** Degrau; **Intel ADR-0050** ampliado; Ready só golden HW.
- **ADR-0041 emenda §9–§10 (2026-07-18):** hierarquia L0–L4; **H1–H5** + **H4+/H5+/AS shallow** (`crates/k_hal` DeviceCap + GPU/net/HDA BE + QUEUE_NOTIFY real + Cap enforce nos ports + `demo_as_r1_r3_shallow`); IDEA #459. Lifecycle ADR continua `fazendo` (PoC ≠ produção). Versão **v1.8.6 TEST**. Planos Cursor canônicos em ADR-0041 **§11** + INDEX “Planos Cursor → ADR”.
- **HalOffer (2026-07-18, 1.8.x):** API R3→R1 `k_hal::offer` (query/bind/release) para **qualquer** `DeviceClass` (gpu/net/wifi/block/snd/video/display/input). Bind **granta Cap Fe***; ports `fe_*` Deny sem bind. Hermes: `request_from_intent` + PnP `request_from_pnp_next`. Tópicos `HW_OFFER` / `HW_BOUND`. VirtIO = só transporte BE.
- **Log estruturado (2026-07-18):** formato canônico `[T+n] [Rn] [k-xxx] [Item] [subitem] - …` via `k_nano::slog_*!` (`slog.rs`); **~1526** calls migrados em k-nano/k-hal/k_ai/cortex/hermes/jarbas/neural-kernel (`tools/migrate_slog_all.py`). Leftover `serial_println!` só em `slog.rs` (backend) + comentários.
- **Evidência consolidada:** `SESSION_121.md`–`SESSION_129.md`.

### Pista limpa (2026-07-16)
| Track | Status |
|-------|--------|
| **ADR-0053 HANR parity** | ✅ **MVP++** (SESSION_136–137) — Cognitive Bridge + **route_user_intent** Trinity→Trust→Skill/LLM |
| **ADR-0042 N1–N5** | ✅ **CLOSED** (v1.7.7) — cadeia K³CHJ funcional; **N2.5** ✅ (v1.7.8); **N3.5** ✅ (v1.7.9); **N4.6** ✅ (v1.7.10); **N5.7** ✅ (v1.7.11) |
| **ADR-0040 FS MVP** | ✅ **CLOSED** (SESSION_124) — soft-migrate MHI; exFAT FilesystemDriver; NeuralFS `/mnt/neural` (SESSION_123 RAM); residuals SESSION_125 → todos `por_fazer` |
| Sprint 107 Voice | ✅ FECHADA — PASS parcial forte+ |
| Sprint Sound | ✅ pipeline Mic→Wake→STT→TTS; STT PCM; UAC parse; neural-lite; residual soft-float/VITS + cutover |
| Sprint 108 | ✅ **CLOSED** — self_evolve + SelfEvolveAgent (observe→gen→verify→improve→reflect) |
| **N-gram spec (ADR-0047 §3.7)** | ✅ **OK** (SESSION_125/127) — decode + bench empírico `[ADR-0047-NGRAM]` speedup_est |
| **ADR-0047 família MVP** | ✅ **Accepted parcial** (SESSION_126–127) — L1–L3 + Genesis + G1–G5 PoC + H1/H2/H4/H5; H3/ISA/adapter ❌ descartados |
| **ADR-0046 AirLLM GGUF** | ✅ **MVP completa** (SESSION_127) + hot-swap Net code (SESSION_128) — ATA+`set_model`; Net→FAT→AirLLM (L3.5/RX se RX=0); residuals: DMA / stream-to-disk / K-quants / e2e GGUF grande |

### NeuralFS (SESSION_123 + 132 + 133)
| Item | Estado |
|------|--------|
| Format/mount + file R/W | ✅ RAM 4MB em `/mnt/neural` |
| B-tree multi-nivel | ✅ leaf + internal split; path CoW; smoke_multilevel |
| Free-list reclaim | ✅ LIFO + page `NRFSFREE`; smoke_reclaim |
| VFS agent | ✅ ATA → USB (mount) → RAM; format USB **opt-in** |
| GPT NeuralFS | ✅ GUID `GPT_TYPE_NEURALFS` + virgin `gpt_format_single` |
| Disco fisico | ✅ ATA cauda; USB mount; USB format só com flag/debug |
| Boot dados exFAT | ✅ `mkexfat` + unified ESP FAT / dados exFAT |
| Espelho k_nano | ✅ gpt sync; agent USB fica no bin |
| Residual | ▶️ USB power-cycle AWAITING_HW; interop host exFAT; smokes level2+power_loss_soft wired |
| exFAT write (#417) | ✅ opt-in `EXFAT_WRITE=1` + `exfat_write.rs` (SESSION_144); smoke SKIP sem flag |

### Sound / Voice (ADR-0045) — Sprint Sound ✅
| Item | Estado |
|------|--------|
| Truth path | `neural-kernel/src/audio/*` (boot) — residual N5.7 |
| Espelho | `jarbas/src/audio/*` — sync VAD/settings/wake Continuous; **sem cutover** |
| Stack | HDA + Piper neural-lite (+formant) + STT CTC PCM + VAD adapt + mixer + barge-in |
| WakeWord | Continuous + gate pós-WAKEWORD (bypass `weather-e2e`) |
| UAC | parse+probe+USB-TRUST; iso ▶️ `[UAC-HW] VERDICT=AWAITING_REAL_HW` (SESSION_145) |
| USB Trust | ✅ `usb_trust` + `system/trust/usb.tbl`; `USB_TRUST_ENFORCE` |
| STT | `train_stt.py` PCM→MFCC; `STT.BIN` regenerado; CTC tiny WER ainda fraco |
| Piper | neural-lite polish; **VITS/HiFi-GAN = soft-float blocker** (defer Onda 4) |
| Obsoleto | sherpa / Pocket / Kokoro-primário / Vosk / Wyoming / Rustpotter |

### Adequação N0–N5 (ADR-0042)
| Fase | Status |
|------|--------|
| **N0** Baseline boot Runtime | ✅ |
| **N1** k-nano legível | ✅ N1.1+N1.2+N1.3 |
| **N2** k-ai HW-AI / SelfHeal | ✅ **CLOSED** (v1.7.4) — heal/noop + HEALTH_ISSUE/honest noop + VID+subclass gate + Trust; **N2.5** link `k_ai` no bin ✅ (v1.7.8) |
| **N3** cortex cérebro | ✅ **CLOSED** (v1.7.5) — llm=LOADED + MAP_WEIGHTS + Trinity (keyword+R3) + generate path; soft-float fluency → Sound; **N3.5** link `cortex` ✅ (v1.7.9) |
| **N4** hermes orquestra | ✅ **CLOSED** (v1.7.6) — intent routing + ReAct/skills + WASM SFI + cortex orchestrate + EventBus; **N4.6** link `hermes` ✅ (v1.7.10) |
| **N5** jarbas ego/UI | ✅ **CLOSED** (v1.7.7) — compositor + persona + voz via Hermes + FB paint; **N5.7** link `jarbas` ✅ (v1.7.11) |

### Sprint 107 close loops (2026-07-16 sessão 2) — **FECHADA (parcial forte+)**

| Loop | Log | HWEXPERT | STT ctc | GEN | Notas |
|------|-----|----------|---------|-----|-------|
| L1 | `logs/boot_whpx_20260716_095549.txt` | ✅ LOADED | ❌ blanks=100% | ❌ `'LOA,BLOA…'` h=128 | Trinity default→hw_identify; cargo 0e/0w |
| L2 | `logs/boot_whpx_20260716_101215.txt` | ✅ LOADED | ▶️ `ctc='so'` (blank-suppress→seed) | ✅ `' tempo esta bom'` | force generator; weatherish |
| L3 | `logs/boot_whpx_20260716_102813.txt` | ✅ LOADED | ▶️ `ctc='so'` + retries | ✅ `' tempo esta bom'` | multi-probe STT |
| L4 | `logs/boot_whpx_20260716_104440.txt` | ✅ LOADED | ▶️ `ctc='so'` + EventBus | ✅ `' tempo esta bom'` | CMVN+TOPIC_STT_TEXT+USER_INTENT |
| L5 | `logs/boot_whpx_20260716_110041.txt` | ✅ LOADED | ▶️ `ctc='so'` + EventBus | ✅ `'O tempo esta'` | bias O↑; **canônico fecho** |

**Veredito Sprint 107:** **PASS parcial forte+** (fechada para voz). Avanços vs baseline `033322`: HWEXPERT LOADED; CTC path non-empty (`so`); EventBus STT→INTENT; GEN weatherish estável no 2B; TTS Continuous=`synthesize_tts`/Piper; FB paint. **Gaps de voz → Sprint Sound (reaberta)** (não 108): STT retrain PCM-real; soft-float latency; Mic→Wake→STT runtime; jarbas wire pleno; Piper VITS pleno; UAC/VAD/SER polish.

### Evidência clima e2e (2026-07-16 — Sprint 107 fecho L5)

**Log canônico fecho:** `logs/boot_whpx_20260716_110041.txt`  
**Baseline antigo:** `logs/boot_whpx_20260716_033322.txt`

| Critério | Resultado L5 fecho |
|----------|-------------------|
| Bridge WHPX | ✅ `run_weather_e2e.ps1` `-KillMinutes 15 -Window -Smp 2` |
| GEN | ✅ `decoded_len=12 text='O tempo esta'` — h=2560 soft_stride=3 weatherish |
| TTS | ✅ Piper neural-lite `pcm_samples=13769` via `synthesize_tts` |
| FB | ✅ `[JARBAS-TTS-FB] painted len=12 1280x800` |
| STT | ▶️ CTC LOADED; blank-suppress `ctc='so'` (non-empty); LLM ainda seed (domain synth) |
| EventBus | ✅ `TOPIC_STT_TEXT` + `USER_INTENT` no path clima |
| WakeWord | ✅ registrado; Mic→WAKE no e2e clima ainda não exercitado |
| Experts | ✅ HWEXPERT·RUSTCODER·STT·BGE LOADED |
| Soft-float | ❌ known blocker (doc-only; sem fake fix) |

### Evidência clima e2e (2026-07-16 — Sprint 107 loops 1–5 antigos)

**Log canônico:** `logs/boot_whpx_20260716_033322.txt` (Loop 5 sessão 1)

| Critério | Resultado |
|----------|-----------|
| Bridge WHPX | ✅ `run_weather_e2e.ps1` `-Window -Smp 2`; kill 18 min |
| GEN | ✅ `decoded_len=12 text='O tempo esta'` — frase PT climática (logits reais + máscara posicional; **não** canned) |
| Evolução | L1 panic STT → L2 `' tempo Tempo dia'` → L4 `' tempo esta bom'` → **L5 `'O tempo esta'`** |
| TTS | ✅ Piper **neural-lite** `emb.weight` vocab=256 · `pcm_samples=15428` (não formant-only) |
| FB | ✅ `[JARBAS-TTS-FB] painted len=12 1280x800` |
| STT | ▶️ CTC LOADED 10 tensors 55K; path real (formant+Piper retry) mas `ctc=''` → seed prompt (não STT-sim puro) |
| WakeWord | ✅ `WakeWordAgent` registrado no boot |
| Experts | ✅ RUSTCODER · STT · BGE · ❌ HWEXPERT parse FAILED |
| **Veredito clima** | **PASS parcial forte** — meta 1+2+3+6; loop TTS↔STT↔LLM fechado só com seed STT |

**Ops rebuild:** `CARGO_TARGET_DIR=target` + `cargo nk` + `cargo build --release -p boot`. Piper: `python tools/convert_piper_to_bitnet.py` → `target/PIPER_PT_BR.BIN` (v3 index + alias `emb.weight`←`sid`).

### Evidência clima e2e (2026-07-16 — `logs/boot_whpx_20260716_012934.txt`) — superseded
| Critério | Resultado |
|----------|-----------|
| Bridge WHPX | ✅ `run_weather_e2e.ps1` `-Window -Smp 2`; soft_stride FWD ok |
| Chat template | ✅ `prompt_len=6` `first=128000 last=128007` (BOS+cue+eot+assistant hdr) |
| soft_stride | ✅ `[FWD] soft_stride=3 layers≈10/30` |
| GEN | ✅ `decoded_len=11 text=' tempo rain'` — **weatherish** (tempo+rain), não mash EN / não `"6666"` |
| Constrained | ✅ `argmax_row_weather_only` no lexicon (logits reais; sem string canned) |
| TTS | ✅ `pcm_samples=13920` (formant; emb invalid) |
| FB | ✅ `[JARBAS-TTS-FB] painted len=11 1280x800` |
| Experts | ✅ RUSTCODER LOADED · STT CTC LOADED · BGE LOADED · ❌ HWEXPERT parse FAILED |
| HIT e2e | ✅ `readable + weatherish=True + pcm + fb` |
| **Veredito clima** | **PARTIAL** — melhor que mash EN; ainda não frase PT climática plena (`O tempo está bom…`) |

**Ops rebuild:** `CARGO_TARGET_DIR` sandbox mascara `cargo nk` → forçar `$env:CARGO_TARGET_DIR=…\target` + `bootloader_linker -u` (crate `boot` trava em `cargo install bootloader-x86_64-uefi`).

### Evidência clima e2e REAL (2026-07-15 — `logs/boot_whpx_20260715_185914.txt`)
| Critério | Resultado |
|----------|-----------|
| Bridge WHPX | ✅ `run_weather_e2e.ps1` sem `-NoSerialBridge`; kill 18 min |
| BPE | ✅ `[BPE] BPB1 LOADED vocab_n=128256` @0x150000000 |
| GEN | ✅ IDs HF reais `bpe=1 first=128000 last=24108` (`Ġtempo`); `decoded_len=50` com letras (não `"6666"`) |
| Texto PT clima | ❌ saída mash EN (` importantlyabil-worker…`) — não frase climática |
| TTS | ✅ `pcm_samples=59040` (formant; `emb invalid`) |
| FB | ✅ `[JARBAS-TTS-FB] painted len=50 1280x800` |
| Canned | ✅ texto vem do decode do modelo |
| **Veredito clima** | **FAIL fechar pleno** / **PASS mínimo anti-`6666`** (letras+len+TTS+FB) — **superseded** pela evidência 2026-07-16 |

**Root `"6666"`:** `.bitnet` só embute stub `CHAR:32-126`; sem BPE, encode CHAR aponta embeddings HF errados e `argmax_row_char_vocab` travava em token 25 = `'6'`.

**Fix parcial:** `tools/export_bpe_bin.py` → `target/bpe_vocab.bin` (BPB1); loader QEMU; tokens `u32`; chat frame Llama 6 toks; soft_stride=3; constrained weather lexicon; e2e exige letras + weatherish≥2 hits.

**Blocker restante:** soft-float 2B — logits HF ainda fracos p/ frase PT gramatical; Piper `emb.weight`; HWEXPERT parse FAILED (magic OK); float/AVX path ou merges BPE encode pleno.

### Evidência multi-token smoke antigo (2026-07-15 — `logs/boot_whpx_20260715_155416.txt`)
| Item | Resultado |
|------|-----------|
| Bridge | via `run_weather_e2e.ps1` + `-Window` (GUI); kill 15 min |
| GEN | `max_gen=4` → `decoded_len=4 text='6666'` (CHAR argmax; qualidade baixa = dígito repetido) |
| TTS | `[JARBAS-TTS] 6666` + `pcm_samples=1600` (formant fallback; `emb invalid len=0`) |
| FB | `[JARBAS-TTS-FB] painted len=4 1280x800` — splash Orb + texto na janela QEMU |
| HIT e2e | `HIT multi-token+TTS … decoded>=4 pcm ok fb=True` (**obsoleto** — critério endurecido) |

**Antes (smoke 1-token):** `logs/boot_whpx_20260715_145128.txt` — `decoded_len=1` `"6"`; FB só DIAG/AIOS.

### Evidência clima e2e + bridge (2026-07-15 — `logs/boot_whpx_20260715_145128.txt`)
| Item | Resultado |
|------|-----------|
| Bridge | `[BRIDGE] started` → QEMU → `[BRIDGE] killed` (PS1 finally; **sem** `-NoSerialBridge`) |
| Status | `llm=LOADED bge=LOADED piper=LOADED` |
| GEN | `prompt_len=2 (raw=39)` (nao mais EOS-sozinho) → `next=25` → `decoded_len=1` → texto `"6"` |
| TTS | `[TTS] Piper: "6" (1600 samples)` + `[JARBAS-TTS] piper=LOADED pcm_samples=1600` |
| Clima e2e | **PASS** smoke antigo (generate nao-vazio + samples>0) — **não** fecha qualidade |

**Root cause empty generate:** soft-float 2B truncava prompt ao **ultimo token = EOS** (`prompt_len=1`) → argmax→EOS → string vazia; vocab HF 128k + decode CHAR sem BPE tambem devolvia vazio. Fix: slim BOS+1 char + argmax no range CHAR.

**Loop:** attempt1 `prompt_len=5` timeout FWD; attempt2 generate OK + Piper panic `%0`; attempt3 PASS 1-token; attempt4 multi-token `max_gen=8` timeout 10m (3 steps); attempt5 `max_gen=4` + 15m kill → PASS smoke; attempt6 BPE HF → letras mas não PT clima.

**Ops:** soft-float + `cargo nk`; QEMU 6G/SMP; timeout ~18 min BPE; helper `tools/run_weather_e2e.ps1`. Ver `SESSION_108.md`.

### Micro-experts QEMU loaders (2026-07-16)
Mapa phys (após BPE `@0x150000000`): **HW Expert** `@0x160000000` (`hw_expert_v3.bitnet` ~260KB), **RustCoder** `@0x161000000` (`rust_coder.bitnet` ~270KB), **BGE** `@0x162000000`, **STT** `@0x163000000`. Kernel consome via QEMU-loader + fallback FAT (`HWEXPRT.BIN` / `RUSTCDR.BITNET`). Alias legado `hw_expert.bitnet` ausente no disco — usar v3/tf. Regenerar: `python tools/train_hw_expert_v3.py`, `python tools/download_and_train.py --rustcoder`.

**Boot 2026-07-16 (`012934`):** `[RUSTCODER] LOADED` · `[STT] CTC LOADED` · `[BGE] …LOADED` · `[HWEXPERT] parse FAILED` (magic OK @0x160000000 — layout/size hint).

### Sprint Net / e1000 → Pós-LAN (SESSION_149–152)
- **Gate canônico:** e1000 PCI + smoltcp. **SLIP/COM2 FROZEN** (opt-in `-SerialBridge` apenas).
- **Launch:** user/slirp + static `10.0.2.15`; `-Bridge` = TAP + DHCP. Peers host: `netfs_peer.py` :4446, `serve_tiny_gguf.py` :8080.
- **✅ L3.5–L5:** TX regs `0x3800/0x3818`; DNS raw; HTTP 301 (SESSION_149/150).
- **✅ Pós-LAN:** `net_bridge`; NetFs `[NETFS] VERDICT=PASS`; TLS `[TLS] VERDICT=BLOCKED softfloat_or_crate`; SelfUpdate HTTP (SESSION_152).
- **🔬 SESSION_154:** TLS opções A–D; WiFi API77 + S0–S5; iwlwifi FW MAC.
- **✅ SESSION_155:** TLS probe A — soft-float compile PASS.
- **✅ SESSION_156:** TLS N4 wire — `https_get` + `NetTcpIo` + `KernelRng`; `trust=unsecure`.
- **✅ SESSION_157:** TLS smoke PASS — `[TLS] VERDICT=PASS bytes=80952` + `smoke=PASS` (google); PKI residual.
- **✅ SESSION_158:** TLS PKI híbrido — `trust=root_learn`→`root_pin` (google×2); CertVerify/FAT residual.
- **✅ SESSION_159:** WiFi S0 + prep S1 — `VERDICT=AWAITING_REAL_HW` + `fw_resolve=SKIP`; DID→FAT; sem ALIVE.
- **✅ SESSION_160:** WiFi pivô ath10k QCA6174 — `168C:003E`; FW 1.45MB; BMI/CE scaffold; A3 Note AWAITING.
- **✅ SESSION_161:** ath10k A3 — CE/BMI/LZ wired; `VERDICT=PASS` só com FW_IND pós-DONE no Note.
- **Histórico:** smoke `190530` L3.5 FAIL — supersedido; não repetir “RX morto” como estado atual.

### Serial SLIP bridge (FROZEN — nao e path do gate Net)
- Script: `tools/serial_bridge.py` — TCP **server** `127.0.0.1:4444`; QEMU COM2 = **cliente**.
- Default: **nao** sobe peer. Opt-in: `-SerialBridge`. Alias `-NoSerialBridge` = skip (ja e default).
- `-Bridge` = WinTAP/e1000 (distinto do SLIP). PS1 ASCII-only.

### Piper + BGE (2026-07-15)
| Item | Antes | Agora |
|------|-------|-------|
| **Piper** | LOADED 400 tensors 15M; **neural-lite** via `emb.weight`/`sid` (não formant-only no e2e) |
| **Weather TTS** | FAILED empty generate | generate real + `pcm_samples>0` (texto ainda pobre) |
| **BGE** | FAILED | **LOADED** stub |

### Próximo
- **Pista ativa:** Boot Note ath10k A3 (`fw_ready`) · TLS `#123` ✅ · `/model-fetch` · gate v2.0.0 review.
- **Sound residuals (não pista):** soft-float/VITS defer · UAC `#84` AWAITING_HW · cutover jarbas/audio.
- **Gate `v2.0.0`:** review ADR `fazendo` + `por_fazer`/AWAITING defer + OK maintainer — não auto-declarar.
- **Sprint 108:** ✅ self-evolving agents.
- Ops: `CARGO_TARGET_DIR=repo\target`; evidência Pós-LAN: `SESSION_152.md` + `logs/boot_postlan_152c_*.txt`.

### N3 CLOSED (2026-07-16) — cortex cérebro ✅
| Item | Onde | Serial / aceite |
|------|------|-----------------|
| N3.1 llm LOADED | QEMU-loader BitNet 2B + `[STATUS]` | `llm=LOADED dim=2560 bpe=LOADED` |
| N3.2 MAP_WEIGHTS | P5 `cortex_mmap` + gate | `MAP_WEIGHTS pages>0 (P5 Cap OK)` |
| N3.3 Trinity MoE | experts + HWEXPERT/RustCoder | `experts=6 generator=OK moe_router=ABSENT(keyword)` |
| N3.4 prompt→texto | path + prior weather-e2e | boot `generate=GATED soft-float`; prior `decoded_len=12 'O tempo esta'` |
| N3.5 crate link | `cortex-crate` wired; residuals integração bin | ✅ v1.7.9 |
| Gate | `n3_cortex_gate()` em `main.rs` | `[N3-CORTEX] gate complete … criteria=MET` |

### N2 CLOSED (2026-07-16) — SelfHeal gated ✅
| Item | Onde | Serial / aceite |
|------|------|-----------------|
| `SelfHeal::run_vid_gated_scan` | `k_ai` + espelho `neural-kernel` | `[N2-SELFHEAL] heal\|noop` + `done scanned=…` |
| Inventário VID+subclass | `vid_class_triples` / `fw_gated_devices` / `device_needs_fw` | Intel e1000 02:00 ≠ iwlwifi; NVIDIA 10DE:03 intacto |
| Trust (agent,skill) | `trust_allow_agent` / `check_or_cache_agent` | `[TRUST] allow (token,agent,skill)=(1,self_heal,recover)` |
| Boot order | Trust **antes** SelfHeal no registry | gate não DENY sob Observe |
| HEALTH_ISSUE | I3 signal-only **ou** honest noop `fw_gated=0` | EventBus + log explícito |
| Link crate | hermes → `k_ai::*`; bin monólito espelha | ⏳ N2.5 `#[global_allocator]` clash |

## Sprint 107 Part B — fixes pontuais (2026-07-16)

Parte A = `c74ab95` + tag `v1.7.2`. Parte B = fixes 10→2 abaixo, **sem** bump de versão (continua 1.7.2), **sem** push, **sem** claim v2.0.0, **sem** strings de clima "canned". `cargo clean -p neural-kernel` + `cargo nk` (target isolado `target/check-s107b`, e também default `target/`) = **0 erros** em ambas as vezes, com e sem feature `jarbas-bridge`.

| # | Item | Status | Evidência |
|---|------|--------|-----------|
| 10 | Doc drift WakeWord | ✅ | `SESSION_INDEX.md`/`IDEA_BANK.md` já diziam "registrado"; drift real estava em `docs/architecture/0045-sound-voice-stack.md` e `TECNOLOGIAS.md` (diziam "não registrado") — corrigido para "registrado (Loop 5, `main.rs`)" |
| 9 | jarbas/audio wired | ▶️ incremental | `jarbas` adicionado como dep **opcional** (`Cargo.toml` feature `jarbas-bridge`, off por padrão). Referenciar `jarbas::audio::*` direto quebra link: `#[global_allocator]`/`#[alloc_error_handler]` de `k_nano` (via `jarbas→hermes→k_ai→cortex→k_nano`) colide com o de `neural-kernel`. Módulo novo `crates/neural-kernel/src/jarbas_bridge.rs` documenta o blocker e compara `TOPIC_*` via cópia local (`jarbas_mirror_literals`), sem `use jarbas::*` — não dispara o conflito. `cargo nk --features jarbas-bridge` = 0 erros. Wiring pleno = fora de escopo (exigiria remover allocator de `k_nano` ou trocar o de `neural-kernel` — refactor grande) |
| 8 | HWEXPERT parse FAILED | ✅ | Causa raiz: `tools/train_gpu_full.py::write_bitnet` gravava `vocab_size`/`num_medusa` como `u16`; kernel lê como `u32` → `vocab_size=4194368` lixo → parse FAIL. Fix: (1) `write_bitnet` corrigido p/ `u32` (alinhado com `train_models_gpu.write_header`); (2) `tools/fix_bitnet_header.py` (novo) reescreve o header de `target/hw_expert_v3.bitnet` e `hw_expert_tf.bitnet` existentes sem retreinar — arquivos agora 266130B (era 266126B), header `vocab=64 num_medusa=0` corretos; (3) `main.rs` `hw_sz` QEMU-loader atualizado p/ 266130; (4) `tools/sim_load_model_hwexpert.py` (novo) simula `load_model()` em Python e confirma `[PASS] load_model() simulation returns Some(model)` — **não** mais `None`/parse FAILED. Ainda existe mismatch de *layout* de pesos (custom BitNetLM vs. esperado, ~220KB sobrando) — separado do bug do header, pesos ficam semanticamente incorretos mas o parse não falha mais |
| 7 | UAC stub | ▶️ pequena melhoria | `audio/usb.rs::probe_uac()` era `false` fixo. Agora escaneia PCI (`crate::pci::scan_pci()`) por controlador USB (classe `0x0C`/subclasse `0x03`) e retorna `UacProbeResult` (`NoUsbController` / `ControllerPresentClassScanDeferred`) com log honesto — sem enumeração de interface USB real (fora de escopo, exigiria parser de descriptors xHCI completo) |
| 6 | Unify TTS | ✅ | `JarvisVoiceAgent::speak()` em `audio/voice.rs` trocado de `audio::tts::synthesize` (formant puro) para `audio::skills::synthesize_tts` — mesmo path do e2e Piper. `audio/jarvis.rs` confirmado sem path de TTS próprio (só publica `TOPIC_TTS_CMD`) |
| 5 | Piper VITS fuller | ▶️ gap documentado + melhoria concreta | Doc-header de `piper.rs` reescrito para não afirmar pipeline VITS completo (encoder→duration predictor→flow→HiFi-GAN) — hoje é "neural-lite": embedding real (`emb.weight`) + oscilador harmônico 3-senoides + ADSR. Melhoria concreta: duração por fonema agora varia (vogal +30%, consoante -20%, espaço -50%) em vez de fixa 50ms/fonema — aproxima (levemente) do duration predictor real sem fingir implementá-lo |
| 4 | Mic→WakeWord→STT→LLM→TTS EventBus | ✅ skinny wiring | `JarvisVoiceAgent` agora assina `TOPIC_WAKEWORD` (seta `woken=true` + log). No fim de fala, chama `audio::stt::transcribe_global(&pcm_buffer)` real (era stub `[audio N samples]`); se retorno não-vazio publica em `TOPIC_STT_TEXT` **e** `USER_INTENT` (consumido por Hermes); se vazio, publica placeholder em `TOPIC_STT_TEXT` (fallback honesto, sem fingir texto) |
| 3 | Generate livre PT | ✅ | `bpe.rs::weather_step_candidates()` — máscara de clima relaxada: `step=0`/`step=1` agora aceitam conjunto mais amplo de tokens iniciais (antes fixo), `step>=3` usa lexicon completo `weather_candidate_ids()` em vez de subset rígido — mais liberdade de frase PT dentro do mesmo `soft_stride` budget, sem strings canned |
| 2 | STT CTC empty | ✅ 2 bugs corrigidos | (1) `mfcc()` recalculado com DFT real via tabelas seno/cosseno pré-computadas — implementação anterior produzia espectro fraco/incorreto; (2) `load()` — heurística de offset (byte vs. f32-index) para pesos LSTM corrigida, evitando carregar `lstm0.weight_ih`/`weight_hh` corrompidos; (3) `transcribe()` ganhou log de debug (`n_frames`, `raw_path` = melhores chars antes do collapse) quando resultado vazio, para diagnóstico futuro. **Não re-testado em WHPX real** nesta sessão (rebuild de `target/uefi.img` via `cargo build -p boot` sem `bootloader_linker` travou — ver "Ops" acima); validado apenas via `cargo nk` (0 erros) |
| 1 | Soft-float perf | ❌ SKIP (pedido explícito) | Known blocker, sem fix fake. Ver `SESSION_110.md` |

**Verificação pós-código:** `cargo clean -p neural-kernel` + `$env:CARGO_TARGET_DIR=target/check-s107b; cargo build --release -p neural-kernel --target x86_64-unknown-none` (equivalente a `cargo nk`) = **0 erros**, 3 warnings pré-existentes (unused imports em `bitnet_avx2.rs`/`piper.rs`, `model_loaded` unused-assignment em `main.rs` — não introduzidos por Part B). Repetido com `--features jarbas-bridge` = **0 erros** também. Rebuild adicional no `target/` default (não isolado) para tentar e2e WHPX: `cargo nk` OK, mas `cargo build --release -p boot` (sem `bootloader_linker`) travou (nested cargo lock) — morto após ~10min; e2e WHPX **não executado** nesta sessão. Fallback usado: `tools/sim_load_model_hwexpert.py` (host Python) confirma fix do #8.

### Identidade funcional K³CHJ (ADR-0042)
| Anel | Função |
|------|--------|
| **k-nano** | Sistema **legível** (HW bruto, Caps, CR3, log honesto) |
| **k-hal** | **HAL R1** — DeviceCap, HalOffer, MMIO BE, VirtIO transporte |
| **k-ai** | AI **para hardware** + SelfHeal + HMI de máquina |
| **cortex** | **Cérebro** — MoE, learn, busca, mmap pesos |
| **hermes** | **Orquestrador** agentic — intent, skills, criação |
| **jarbas** | **Ego / persona / +10%** — UI, humor, frontend |

Cadeia: `k-nano → k-hal → k-ai → cortex → hermes → jarbas`. Histórico **K²CHJ** = sem `k_hal` na marca (ADR-0042 §0).

**Nota ops:** Builds isolados sob `target/` (`target/agent-*`, `target/check-*`, `target/n16-*`). Rebuild `uefi.img` via `cargo build --release -p boot` (pode travar em `cargo install` bootloader — liberar lock `.cargo`).

### Boot endurecido + Capability Rings (2026-07-14)
| Pacote | Conteúdo | Status |
|--------|----------|--------|
| **A** | STI+PIC, stack heap ≥2MB, `init_phase` RR, `BOOT_PHASE`+consumer, DiagnosticSkill | ✅ |
| **B** | `init_platform_sync` antes dos drivers; Platform/NetDriver idempotente; Agency → EventDriven | ✅ |
| **MVP C→P9** | ADR-0041: AS/CR3/SPSC/Cap/`int 0x90` + CapGate + FB + DMA/mmap + Ring3 + #PF + vring + GGUF | ✅ PoC |

### Real vs stub (pós P0–P9)
| Peça | Real | Stub / limite |
|------|------|----------------|
| 2 AS + CR3 + SPSC + Cap + `int 0x90` | ✅ | Shallow L4 (PTs kernel compartilhadas) |
| CapGate Hermes (`aios_*`) | ✅ | Sem AS separado / SFI pleno (#426 🟡) |
| JARBAS FB map + present | ✅ | VSync stub; path = bootloader FB |
| DMA pin + weight mmap | ✅ | Pesos simulados no eager path |
| Ring3 `iretq` + stub | ✅ código | **Untested QEMU estável**; `TRY_ENTER_RING3`; sem ELF/preempt |
| Demand-page #PF | ✅ | Frames pré-alocados; **sem I/O no fault** |
| VirtIO vring | ✅ layout+pin | **Sem QUEUE_NOTIFY**; NIC live observe-only |
| GGUF/FAT mmap | ✅ pré-fill 1–4 pág. | Prefixo só; fallback `NFIL`; sem streaming 8GB |

### K³CHJ Capability Rings — P0–P9 (ADR-0041) — todos ✅ PoC
P0 gap · P1 ADR · P2 MVP C · P3 CapGate · P4 FB · P5 DMA/mmap · P6 Ring3 · P7 #PF · P8 vring · P9 GGUF/FAT.  
**Módulos:** `address_space`, `syscall`, `ipc/*`, `capability_gate`, `jarbas_fb`, `k_ia_dma`, `cortex_mmap`, `user_mode`, `demand_page`, `virtio_vring`, `gguf_mmap` + demos non-fatal em `main.rs`.

**Riscos / follow-ups:** Ring3 default `TRY_ENTER_RING3=false` (PoC); VirtIO sem QUEUE_NOTIFY; #PF sem I/O; telemetria modelo ainda inconsistente (alvo N1); Agency EventDriven ociosa sem eventos; crates K³CHJ ≠ bin até wiring; **Boot OK ≠ visão completa** (ADR-0042).

## Marcos Acumulados
- **🏆 v1.7.4 (2026-07-16):** ADR-0042 **N2 CLOSED** — SelfHeal VID+subclass + Trust + QEMU serial. N2.5 = link `k_ai` (allocator). Ver `SESSION_112.md`.
- **🏆 v1.7.3 (2026-07-16):** Docs — Sprint 107 voice fechada; leftovers → Sprint Sound; pista limpa ADR-0042 N2. Ver `SESSION_111.md`.
- **🏆 v1.7.2 (2026-07-16):** Sprint 107 loops 1–5 clima PASS parcial forte — GEN `'O tempo esta'`, Piper neural-lite, WakeWord registrado. Ver `SESSION_110.md`.
- **🏆 v1.7.1 (2026-07-16):** ADR-0045 Sound Voice Stack (docs). Ver `SESSION_109.md`.
- **🏆 v1.7.0 (2026-07-15):** N1 ✅ + BitNet 2B LOADED (~590MB, 30L, FWD); soft-float/`cargo nk`; TTS empty known. Ver `SESSION_108.md`.
- **🏆 v1.5.7 (2026-07-14):** Boot A/B + ADR-0041 capability ladder P0–P9 (PoC non-fatal). Ver `SESSION_107.md`.
- **🏆 v1.8.0 (2026-07-16):** ADR-0042 N1–N5 + wire N2.5–N5.7 consolidados. Gate `v2.0.0` permanece sujeito a review formal; Sprint Sound concentra a qualidade de voz.
- **🧪 v1.8.5 (2026-07-16):** consolidação não estável pós-v1.8.0: Self-Evolve, Sound, NeuralFS/ADR-0040, AirLLM/ADR-0046 e família ADR-0047; ADRs GPU 0048–0050 propostas.
- **🧪 v1.8.6 (2026-07-18):** ADR-0041 H4+/H5+/AS shallow + HalOffer Cap grant; crate `k_hal` + slog canônico; SESSION_140. Gate v2.0.0 intacto.
- **🏆 Sprint 106 (2026-07-14):** Ecossistema de Anéis Lógicos completo (10/10), sem constituir release `v2.0.0`. Workspace K³CHJ, SOUL.md via VFS, MicroPython/WASM, SkillOpt e AIOS API.
- **🏆 v1.5.3 (2026-07-13):** Ponytail audit 100% implementado. 6 dead files → LEGACY/v1.5-dead-k2chj/.
- **🏆 v1.5.2 (2026-07-13):** 0 erros. RingBufStore extraído em fs/mod.rs (ram_fs + log_fs delegam para tipo genérico com evicção FIFO). LEGACY/v1.5-neural-kernel-src/ snapshot criado — baseline para migração v2.0.
- **🏆 v1.5.1 (2026-07-13):** 0 erros. ~600 LOC removidos, 11 dep entries eliminados. 6 dead files movidos do neural-kernel para K³CHJ crates. pic8259 eliminado. #[cfg(not(x86_64))] branches removidos. Architecture trait removido.
- **🏆 v1.5.0 (2026-07-13):** 0 erros. K³CHJ workspace migration: monólito → 5 crates (k_nano, cortex, k_ia, hermes, jarvis). Dep chain linear. k_nano compila independentemente. migrate_k2chj.py (193 files, 79 refs).
- **🏆 v1.2.0 (2026-07-12):** ATA PIO bug fix crítico — READ_SECTORS e IDENTIFY usavam `in al, dx+1` para byte alto (lendo FEATURES/ERROR). Fix: `in ax, dx`. TODO acesso a disco desde o início do projeto era lixo.
- **🏆 v1.1.5 (2026-07-12):** 0 erros, ~26.000 LOC, 116 firmwares. HW Expert v3 (61.453 VID/DID), SelfHealing I3/I4, WiFi Intel AX200 ucode loading, 3 camadas visuais (Orb + Hermes CLI + WM), HDA playback, BrowserAgent real, FFT audio.
- **🏆 B-01 MORTO (v0.109.3 — 2026-07-09):** O bloqueador de 18 sprints caiu. Serial tunnel TCP bridge resolveu o RX=0 que perseguia o projeto desde o início. Primeiro RX: 304 bytes.
- **v0.109.1** — Correção em massa: 32 erros de compilação mascarados pelo cache incremental. `cargo clean -p neural-kernel` revelou imports faltando, APIs trocadas, format string.
- **v0.56.0-v0.67.0** — 22 sprints de OS neural, GPU, desktop, agentes, ecossistema
- **v0.68.0-v0.70.0** — USB Mass Storage, xHCI bulk, BootLogAgent, FAT32 writer
- **v0.71.0** — Boot Bughunt: Agent-First + DiagnosticSkill + FAT12 log + Xuvisco
- **v0.73.0-0.73.1** — Consciousness (10 métricas), Self-Improvement Loop, Shutdown tracking
- **v0.74.0-0.74.2** — TPM TIS driver, Ed25519 kernel signing, Partition mask 0x1C
- **v0.75.0-0.75.6** — FAT32-only, DiskIntelligenceAgent (680 LOC, 6 controllers, 10+ FS probes)
- **v0.76.0-0.76.1** — NVMe driver, S.M.A.R.T., Adaptive heap, Dynamic tick, Event-driven Hermes
- **v0.80.0-0.80.1** — AVX2 Debug, WHPX Detection, KV Cache (200x+ speedup)
- **v0.84.0-0.84.1** — GPU Foundations (BAR UC, SPSC job ring, VRAM alloc, secure boot)
- **v0.85.0** — GPU Decode (BitNet offload, CPU↔GPU KV cache DMA)
- **v0.86.0** — JARVIS Persona (SoulProfile, EmotionAnalysis, EgoLayer, Heartbeat)
- **v0.87.0** — Security + AHCI (TPM extend, Audit Trail, SATA 6G NCQ)
- **v0.88.0** — Emotion + Cache (EmotionEngine, SleepCycle, NeuralCache)
- **v0.89.0** — JARVIS Deep Cognitive (DreamEngine, BabelIndex, AutoSkillGen)
- **v0.90.0** — Desktop UI (JarvisDesktop compositor, Hermes Chat, Settings, Power)
- **v0.91.0** — LAN + Dependencies (DHCP, ARP cache, smoltcp upgrade)
- **v0.92.0** — WASM Runtime + IDE (MemoryPool, HybridRegistry, BitNet IDE F4)
- **v0.93.0-wasm** — WASM Skill Runtime refinado (+WASI mappings)
- **v0.94.0-0.94.1** — Vision + Display + TTF (TrueType font engine, tensor heatmap viz)
- **2026-07-06** — **v0.95.0-cog+v0.96.0-heal:** Sprint 95 (Cognitive Engine) + Sprint 96 (Self-Healing Avançado). cognitive.rs reescrito com 25+ itens (510+ LOC): IntentPlanner, SuccessEngine, NeuralCache, MatMulFreeLM, FeedbackLoop, TernaryUpdate, ReplayBuffer, WorkflowPredictor, AutoSkillGen, DynamicScaler, SelfOptScheduler, CodebookVQ (com KV Codebook e Finetune), ReActLoop, McpServer, DeltaBranches, WorkspaceIsolation, EpisodicMemory, BitNetTrainer, CandleSidecar, TaskSpawner, SleepCycleGuard. Sprint 96 completo com M1-M29 (350 LOC): ZeroCopySfs, SkillModule, FailureTaxonomy, ExceptionSelfHeal, CorrectivePrompting, Verifier, EventLog, BudgetedRecovery, SilentDetection, MultiLevelFailure, FailurePrediction, NotificationGate. +860 LOC totais. 0 erros.

## Arquitetura Fundamental
**Tudo no Neural OS Hermes é um Agente ou uma Skill.**
Fleet scheduler: **41 nativos + N PCI** (+ Agency **0** até AGENT.md assinado — ADR-0052 / SESSION_135). Stubs SESSION_134 apagados. FS VFS (8) fora do scheduler.
Bootloader 0.11.15 com `bootloader_api`. Boot sequence agent-centric.

### Activation on Demand
Agentes só congestionam o tick-tock quando necessário.
- Apenas Hermes, Display, HwBridge usam `Continuous`
- Todo agente importado declara `on_demand: true` no manifesto
- AgentScheduler não polla sem evento pendente
- Penalidade: Continuous não-essencial >5% ticks → rebaixado para EventDriven

### DiskIntelligenceAgent (v0.75.x)
StorageController trait com 6 implementações (ATA, USB-MSC, NVMe, stubs AHCI/SCSI/VirtIO).
FilesystemProbe registry com 10+ probes (FAT32, NTFS, EXT4, XFS, ISO9660, exFAT, Btrfs, HFS+, EROFS, ReFS).
VolumeManagerProbe (LVM2, LUKS). GPT partition table. SED/OPAL detection.
S.M.A.R.T. monitoring (ATA READ DATA 0xB0+0xD0, health alerts).
ARC cache 1MB DRAM + tier migration MHI. I/O scheduler (batched writes). Read-ahead (32KB).

### MemoryAgent (v0.76.1)
Adaptive heap: `resize_heap_to_mb()` dinâmico via frame allocator + map_page_uc.
Orçamento calculado do modelo AI: `heap = clamp(128, params/10MB, 2048)`, `kv = params/40`.
CPU measurement via rdtsc. Dynamic tick calibration via LAPIC init_count.

### Security Stack
TPM 2.0 TIS driver (SHA256 embedded, PCR[8] extend, fallback silencioso).
Ed25519 kernel signing + auto-verification. Partition mask 0x1C (Hidden FAT32 LBA).

### Tick System (v0.76.1)
LAPIC timer com init_count dinâmico: 12-192 ticks/s baseado em agentes ativos.
Hermes event-driven: ReAct cycle só avança com entrada real (silêncio sem trabalho).
EventDriven scheduler fix: `has_event=true` + `has_pending()` early-return pattern.

### Agent Tier Classification
| Tier | Schedule | Exemplos |
|---|---|---|
| Permanent | Continuous | Hermes, Display, HwBridge |
| SystemDemand | EventDriven | DiskAgent, Cortex, Net |
| UserDemand | EventDriven | Skills, Apps, Plugins |
| Periodic | PollEvery(N) | Cron, Observer, Optimizer |
| Learning | PollEvery(2000) | Novos agentes → analisados 5000 ticks → promovidos |

## Roadmap v1.0 — Sprints 92-100 (plano arquivado)

| Sprint | Foco | LOC | Status |
|--------|------|-----|--------|
| **92** | Fundação Estável (VirtIO, AHCI, serial, cleanup) | ~2.000 | ✅ Completa |
| **93** | WASM Runtime + IDE (wasmi, sandbox, marketplace) | ~3.200 | ✅ Completa |
| **94** | GPU Polish + Display (MSched, compositor, co-exist) | ~2.000 | ✅ Completa |
| **95** | Memory + VFS Final (BGE HNSW, MHI bridge, agents) | ~2.000 | ✅ Completa |
| **96** | GGUF + Model Loading (loader, streaming, RoPE) | ~1.500 | ✅ Completa |
| **97** | Rede + AIOS Evolution (WWW, self-update, marketplace) | ~3.000 | ✅ Completa |
| **98** | BitNet + Training Pipeline (100M params, fine-tune) | ~2.500 | ✅ Completa |
| **99** | SkillOpt + Structured Decoding + Code Freeze Prep | ~1.500 | ✅ Completa |
| **100** | **Code Freeze & Release v1.0.0** | ~500 | ✅ Completa |
| **Total v1.0** | | **~18.200 LOC** | |

**v2.0 "Cognição"** começa na Sprint 101: Kernel, Cortex, Hermes, JARVIS como entidade viva.

**Ver também:** `docs/archive/sprints/sprint-plan-92-100.md` para o plano histórico.

## Aprendizados Chave
1. **Roadmap readequado 2026-07-04:** Reorganização completa por dependências. Itens independentes primeiro (Foundation → Agentic → LLM → JARVIS → GPU). B-01 e dependentes no final.
2. **Activation on Demand:** Hermes/Display/HwBridge (+ nativos Net/Input/Cortex…) Continuous; Agency SpecialistAgent → EventDriven (Pacote B).
3. **VGA CRTC + UEFI GOP = incompatível** (Sprint 71)
4. **Cortex acorda antes do HW** — LLM deve participar das decisões de hardware
5. **FAT12 removido** — FAT32-only, 102 LOC eliminados
6. **Partition mask 0x1C** — mbr_nostd aceita Hidden FAT32, bootloader OK, SO não monta
7. **TPM fallback** — silencioso se ausente (0xFFFF FFFF), Ed25519 como enforcement primário
8. **RX=0 persistente** — QEMU slirp + VirtualBox bridge, pre-existente (B-01)
9. **Hermes event-driven** — 84 linhas/seg → 0 quando ocioso
10. **Tick dinâmico** — calibrado por workload (12-192 t/s)
11. **Sprint 77** — 7 Foundation Quick Wins: Prompt `>`, Pre-Flight, FanOut, TaskSchema, SkillIndex, CompletionContracts, DynamicSkill. ~380 LOC, 0 erros.
12. **Sprint 78** — 8 Agentic Evolution items: IntentCache wiring, OutputCache wiring, WorkflowEngine wiring, SelfCritique, GgufBackedModel, AgentTier+migrate_to_tier, FsBridgeAgent, WasmExecutor+WasmSkill. ~400 LOC, 0 erros.
12. **VirtualBox SMP fix** — AP_COUNT static from MADT lapic_count. 2 vCPUs now boot reliably on VB.
13. **Sprint 79** — LLM Infrastructure: BitNet-b1.58 850M downloaded + .bitnet v2 conversion (1.5GB). AVX2 ternary matmul kernel. BPE tokenizer. Trinity MoE stub. QEMU loader boot pipeline at phys 4GB. Ramdisk via bootloader impossível (FAT limit). Forward pass blocked by GQA + BitFFN grouped projections.
14. **BitNet b1.58 real arch** — Microsoft's model is 850M params (not 2B). GQA (20 heads Q, 5 KV heads). BitFFN with grouped down_proj (640→6912). `tie_word_embeddings=true`. vocab_size=128256 (requires u32).
15. **QEMU loader strategy** — `-device loader,file=.bitnet,addr=0x100000000` com `-m 6G` + WHPX. Model in high memory avoids frame allocator conflicts. ~30s boot overhead acceptable for dev.
16. **Build_image.py UEFI issue** — bootloader 0.11.15 default features include UEFI. `default-features=false, features=["bios"]` avoids serde compile panic.
17. **VGA buffer clear fix (v0.79.1):** `[BOOT] FB ativo — VGA text mode desligado` agora é verdade. 0xB8000 limpo via `write_bytes` sem CRTC I/O. Framebuffer limpo para preto imediatamente no probe.
18. **VGA sequencer fix (v0.79.2):** `clear_physical_buffer()` write a 0xB8000 causa page fault pre-IDT. UEFI/OVMF não mapeia legacy VGA hole. Solução: VGA sequencer I/O (0x3C4/0x3C5) Screen Off bit — zero acesso a memória desmapeada.
19. **WHPX emula AVX2/VEX lentamente (v0.80.0):** CPUID mostra AVX2=disponível, mas cada instrução VEX causa VM exit (~10k+ ciclos). Scalar GP instructions rodam nativos. `has_avx2()` deve detectar WHPX via CPUID 0x40000000 e retornar false. AVX2 sob WHPX = 4443 ticks/layer vs scalar = 2218 ticks/layer (~2.2s/layer, ~60s/forward pass).
20. **`unpack_all()` não é o gargalo (v0.80.0):** Substituir alocação de 17.7 MB por row buffer de 6.9 KB não acelerou o forward pass — o gargalo real é a emulação VEX + WHPX memory virtualization. Operações aritméticas dominam, não alocação.
21. **Forward pass BitNet b1.58 sob WHPX:** ~60s para 64 tokens × 30 layers. Generate_speculative de 8 tokens levaria ~6h. Inviável sem KV cache ou bare metal.
22. **Build incremental mascara erros de compilação (v0.109.1):** `cargo clean -p neural-kernel` revelou 32 erros que o cache incremental escondia por meses. Causas comuns: imports faltando (`alloc::vec`, `Vec`, `String`, `ToString`), APIs que mudaram de nome (slab, VFS, jarvis), format string não escapada, `.sqrt()` sem trait `F32Ext`.
23. **RTL8139 RX=0 root cause (v0.109.2):** Bit CR_RE (0x01) nunca era escrito no Command Register (offset 0x37). `cr=0x0c` no log confirmava — só RXE+TXE ativos, RE=0. MAC da Realtek descartava pacotes antes do DMA. E1000 não tem esse bit. **Lições**: dumps de registrador na telemetria salvam dias; sempre verificar enable bits individuais vs combinados.
24. **AHCI funciona, mas sem disco SATA no QEMU:** `scan_pci_cb()` zero-alloc encontrou o controlador AHCI (00:1f.2 class=01/06), driver init OK (Porta 0 com SATA sig=0x101). Mas `-drive if=ide` não anexa disco ao barramento SATA — precisa `-device ide-hd` explícito para testar FAT32 via AHCI.
25. **SkillOpt viability (Microsoft Research, maio/2026):** Primeiro otimizador sistemático de skills em espaço textual. Viável para neural-os-core (~145 LOC) usando CortexAgent como optimizer + SleepCycle como scheduler de épocas. Recomendado para Sprint 99.
26. **SGLang Compressed FSM (Stanford/Berkeley, 2023):** Decodificação constraint via FSM comprimido. RadixAttention inviável em bare-metal (memória), Compressed FSM viável. Mascara logits no BitNet decoder para tokens válidos (JSON/SKILL.md/shell). ~120 LOC, impacto imediato na confiabilidade da saída LLM. Sprint 99.
27. **FlashAttention (Stanford, NeurIPS 2022):** IO-aware tiling para atenção. Aplica-se ao BitNet CPU: processar atenção em blocos de 16 tokens no cache L1 (32 KB). ~3-5× speedup para sequências >256 tokens. Sprint 100+.
28. **🏆 B-01 MORTO (v0.109.3 — 2026-07-09):** O bloqueador de 18 sprints caiu. Causa real: incompatibilidade Windows 11 × QEMU TCG × NIC emulada. Solução: bypass serial TCP. Kernel `slip.rs` (82 LOC) + `serial_bridge.py` (Python como servidor TCP) + `-serial tcp:127.0.0.1:4444` (QEMU como cliente). Primeiro RX: 304 bytes. O kernel sempre esteve correto — era o ambiente que isolava fisicamente o RX.

## Pendente Técnico (Roadmap v1.5.x → v2.0)

### ✅ COMPLETO (Sprints 84-91 + Sound + 95-97)
Todos os sprints de infraestrutura (GPU, JARVIS, SleepCycle, Cognitive, Self-Healing, Trinity MoE) estão implementados e verificados.

### ✅ Sprint 92-100 — Todos completos
- **Code cleanup**: 94 warnings → 0 em todos os crates
- **Zero-Trust Syscall (#364)**: `check_syscall()` + `exempt_tokens` + wireado no WASM runtime
- **Neural Cache (#365)**: Verificado em `cognitive.rs`
- **Serial bridge**: Watchdog + DNS healthcheck + reconexão automática
- **Human-in-the-Loop (#244)**: `/approve`, `/deny`, `/pending` + bloqueio de skills
- **LLM Icons**: `generate_llm_icon()` integrado no compositor com cache
- **GGUF streaming (ADR-0046 MVP):** `GGUFStreamingModel` + `forward_streaming` + soft PrefetchEngine; `/model` ATA + `/model-fetch` Net→FAT→AirLLM (SESSION_127/128). Net falha L3.5/RX se RX=0. P9 mmap ≠ AirLLM.
- **Frame allocator**: Bitmap estendido para 8GB
- **FAT32 streaming**: `read_file_range()` — leitura chunked
- **RssAgent + EmailAgent**: Agentes WWW via HTTP + SMTP
- **HW Expert GPU**: 43.339 dispositivos, loss 0.097, acurácia 95.4%
- **Tag v1.0.0**: Criada e pushada

### ✅ Sprint 100 — Code Freeze v1.0.0
- `cargo clean -p neural-kernel && cargo check --release` 0 erros ✅
- QEMU UEFI boot (OVMF + TCG) — kernel init até runtime/scheduler ✅
- Bootloader v0.11: BIOS image não funciona (triple fault), UEFI funciona ✅
- Ponytail Audit: -19 arquivos, -500 LOC, -3 deps, -32 transitive crates ✅
- #PF no scheduler resolvido via heap stack switch (Pacote A: stack ≥2MB via Vec heap) ✅
- **Pacote B (boot):** `init_platform_sync` (PCI+ACPI+APIC+SMP) antes dos drivers; PlatformAgent/NetDriverAgent idempotentes; Agency SpecialistAgent Continuous→EventDriven ✅
- **🔴 Conhecido**: WHPX crasha com SMP ("Unexpected VP exit code 4") — usar TCG.
- VirtualBox boot test — manual

### ✅ Sprint 101-105 — v2.0 Fundação
- Piper TTS, STT, HDA capture, NVIDIA GPU compute ✅
- K³CHJ workspace migration (5 crates, dep chain) ✅
- Ponytail audit (600 LOC, 11 deps) ✅
- RingBufStore refactor + LEGACY snapshot ✅

### ✅ Sprint 106 — v2.0 Ecossistema de Anéis Lógicos (10/10)
- Cargo workspace estrito (k_nano, k_ai, cortex, hermes, jarbas) ✅
- Rename k_ia→k_ai, jarvis→jarbas ✅
- SOUL.md parser via VFS (4 arquivos jarbas corrigidos) ✅
- Trinity MoE router (classifica intents, não roteia hardware) ✅
- MicroPython/WASM sandbox + WASI→Skill bridge (20+ mapeamentos) ✅
- Page faults fix (allocator → events → agents) ✅
- AIOS API (aios_net, aios_fs) + SkillOpt (Python→Rust no_std) ✅
- Heap address HW real (`0x4000_0000_0000`) ✅

### ✅ Sprint 107 — Voice I/O FECHADA (PASS parcial forte+)
- Clima e2e GEN+TTS+FB, HWEXPERT, Piper neural-lite, WakeWord registrado, EventBus skinny ✅
- Backlog voz → **Sprint Sound (reaberta)** (não bloqueia ADR-42)

### Residuals conscientes (pós SESSION_152)
- Sound: soft-float/VITS ⏳ · UAC `#84` ▶️ · jarbas cutover ▶️ (pipeline Mic→Wake→STT→TTS ✅)
- ADR-0042 N1–N5 + wire ✅ CLOSED (não é pista ativa)
- ADR-0040: #417/#419/#282e–g ✅ · 282h ⏳ · **#418 peer PASS** (S3/WebDAV residual) · #420/#423 ▶️ · #422 USB AWAITING
- Onda 5 GPU: #420/#423/#454–456 ▶️ `[MHI-DMA]`/`[GDS-HW]`/`[GPU-HW]`
- Onda 6 AirLLM: ATA ✅ · Net path ✅ · PreFlight `airllm-net` PARTIAL (falta e2e) · ▶️ `[AIRLLM-DMA]`
- Onda 7 / Pós-LAN: LAN+NetFs ✅ · TLS ✅ · WiFi ath10k A3 código ✅ SESSION_161 · runtime Note AWAITING
- Trilha R soft-float: SESSION_147 ⏳
- Fora gate: SmileyOS/Cube/XDNA/SKYNET · Cross-OS · CRDT
- LAN gate = e1000 RX>0 — SLIP ≠ gate
- **Gate v2.0.0:** `por_fazer`/AWAITING defer + review + OK maintainer.

### ✅ Scheduler performance fix (Sprint 95/96 runtime)
- RTL8139 RX debug rate-limited (1/100 chamadas) — serial flood eliminado
- Scheduler skipa agentes passivos (>50 consecutive Pending → 80% skip)
- `has_event` agora depende de `ScheduleKind` real, não hardcoded `true`

## Arquivos Chave
| Arquivo | Função |
|---|---|
| `disk_agent/mod.rs` | DiskIntelligenceAgent (198 LOC) |
| `disk_agent/controller.rs` | StorageController trait + AtaCtrl + UsbMscCtrl + NvmeCtrl |
| `disk_agent/fs_probe.rs` | FilesystemProbe + 10 probes (260 LOC) |
| `disk_agent/nvme.rs` | NVMe driver (239 LOC) |
| `memory_agent.rs` | Adaptive budget + CPU calibration + dynamic tick |
| `allocator.rs` | resize_heap_to_mb() + CURRENT_HEAP_MB |
| `tpm.rs` | TPM 2.0 TIS + SHA256 embedded (279 LOC) |
| `identity.rs` | Ed25519 kernel verification |
| `agents.rs` | HermesAgent event-driven + Cortex fallback |

---

## Navegação Rápida para AI DEVs

```
📁 docs/                         → Toda a documentação
├── 📁 architecture/             → ADRs: decisões arquiteturais (40 documentos)
│   ├── 📄 INDEX.md              → Lifecycle, conflitos de ID e rastreabilidade
│   └── 📄 0039-boot-flow.md     → Boot sequence agent-centric
├── 📁 memory/                   → Estado, ideias, sessões
│   ├── 📄 STATE.md              → ⭐ COMEÇE AQUI: estado atual do kernel
│   ├── 📄 IDEA_BANK.md          → 416+ ideias catalogadas
│   ├── 📄 SESSION_INDEX.md      → Índice de sessões + lições críticas
│   └── 📄 SESSION_NNN.md        → Sessões individuais com debug e descobertas
├── 📁 archive/sprints/          → SPRINT-106 e planos concluídos
└── 📄 GOVERNANCE.md             → Ciclo IDEA→ADR→sprint→check
📄 AGENTS.md                     → ⭐ POLÍTICAS: regras de engenharia, premissas
📄 ROADMAP.md                    → Roadmap v1.0 → v2.0
📄 TODO.md                       → Checklist mestre
📄 crates/k_nano/ … jarbas/      → 5 crates K³CHJ (v2.0)
📄 crates/neural-kernel/         → Bin de integração
```

---

**Estado canônico:** v1.9.0 teste/não estável — Pós-LAN + Residuals; base v1.8.6; `v2.0.0` não declarado.
