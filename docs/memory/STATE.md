# STATE - neural-os-core v1.9.99-s432 - WHPX 6-lane hardening + triagem IA do HUB

#   PISTA ATIVA: s432 (6 lanes + hub_triage) — TALC claim do BUDGET COMPLETO (6912MB em 8GB) em VA
#     própria 0x400000080000 (fora da janela wrap do bump, demand-paged custo
#     zero). Causa-raiz do OOM da foto: TALC span fixo 512MB estourava quando o
#     bump chegava ao teto 2030MB, com RAM física 70% livre. Headroom combinado
#     bump+TALC; BUMP_BUDGET_CLAMPED separado do budget real. TALC_SPAN_END
#     ANTES do claim (size-tag no fim do span demand-pageava fora do range).
#     Validação: 13,6min, 0 OOM/heap-fail, bump cheio 2030MB e sistema vivo.
#     LIÇÃO: cargo nk não regenera uefi.img — cargo build -p boot obrigatório.
#   PISTA ANTERIOR: s431 (TALC claim) — Lab QEMU 8GB/8c: goal 5min na UI batido (rodada 10:
#     14,8min runtime, UI viva, 1 storm contido por park, OOM final honesto).
#     Fixes: logger no-op p/ crate `log` (LOGGER NULL deref cr2=0x18); storm
#     park por IP (fail-closed de core); watchdog slice 30s medido no alvo;
#     deadline no-progress; gates headroom 48→128MB; emotion alloc-free; BEI
#     guards c/ re-check bounded; escalada I5:boot_log observe-only. 0 erros.
#   PISTA ANTERIOR: s429 — VMD visão guest: offsets SHDW não-nativos NÃO abortam
#     mais. vmd.c provado: offset aplica-se a RECURSOS (bus = cpu − offset ⇒
#     janela BUS começa em host_phys), NUNCA a DMA de RAM (upstream identidade
#     no guest). Tradução real = DOWNSTREAM MMIO: BAR do filho (bus addr) →
#     cpu = bus + offset via MEMBAR mapeada UC (translate_bar puro;
#     child_bar_cpu; probe_at_mmio_va no NVMe). k-nano 240; 0 erros.
#     AWAITING_HW: nvme ok=true via=vmd guest no notebook.
#   PISTA ANTERIOR: s427 — loader-VRAM (`k_hal/src/gpu/loader_vram.rs`): .bitnet v6
#     do Falcon3-1B lido do FAT por CLUSTERS (callback, zero Vec do blob) e os
#     packed das layers (126 matrizes, ~256MB) vão DIRETO pra BAR. Lane VRAM
#     ativa ANTES do load do heap; `on_model_loaded` vira no-op quando
#     `loader_resident()`. HONESTO: `load_llm_v6` ainda copia os packed pro
#     heap no parse — liberação REAL do heap = stub no parser (residual
#     ADR-0112). k-hal 74, k-nano 237; 0 erros.
#   PISTA ANTERIOR: s426 — card `hints_card` (ID 8003, F11): telemetria H3 ao vivo
#     direto dos statics (stage/fwd/n/vram resid) com affordance do critério de
#     aceite (fwd<100µs, n crescendo); refresh 2 Hz idempotente por id; host sem
#     BAR = off + unknown (não finge). jarbas 123/123, k-hal 71; 0 erros.
#   PISTA ANTERIOR: s425 — persistência do BOOT.LOG visível na UI: linha `bootlog`
#     no HUB HEALTH (HUB_ROWS 20) via `boot_logger::hub_log_line()` — ok n<N>
#     / fail <backend> <razão> x<streak> / fail sem-backend / pre-fat / n/a.
#     Statics LAST_FAIL_KIND/LAST_TRY_BACKEND anotados nos paths de falha;
#     SysInfoAgent usa a MESMA string no slog (sem dual-truth). k-nano 237,
#     jarbas hub 7/7, hermes 257; 0 erros.
#   PISTA ANTERIOR: s424 — evidência de boot nunca mais se perde: (1) kernel
#     `overwrite_boot_log` não aborta em IoFail de UMA partição — continua p/ a
#     ESP (contrato OverwriteResult intacto); (2) build embute BOOT.LOG raiz
#     pré-alocado 256KB na ESP (mesmo mecanismo do volume de dados, zero FS
#     novo); (3) fix de corretude do write do dirent (setor real do entry).
#     Validação: ESP de teste com dirent BOOT.LOG 256KB na raiz. k-nano 237.
#   PISTA ANTERIOR: s423 — wake honesto `wake_to_d0` (`k_hal/src/gpu/gpu_power.rs`):
#     a dGPU dorme em D3 no boot (SESSION_260) e os gates do vram/nvidia matavam
#     o H3 e o lane VRAM em TODO notebook. Prova de vida ANTES do write PMCSR
#     (D3cold não recebe write às cegas), budget TSC 10ms, re-scan persistido no
#     GpuInfo; `wake_all` pós detect_all + backstop idempotente nos gates.
#     Gates: 0 erros; k-hal 71. HW lab pendente: GPUPWR woke + hints Resident.
#   PISTA ANTERIOR: s422 — Intel VMD binder (`k_nano/src/vmd.rs`): o NVMe de notebooks
#     Intel RST vive num domínio PCI SECUNDÁRIO (8086:a77f sem driver no lab; NVMe
#     invisível ao CF8/CFC). CFGBAR=ECAM dos filhos + busn_start VMCAP/VMCONFIG +
#     scan flat + enable via config MMIO; SHDW nativo ⇒ DMA físico direto;
#     `NvmeDriver::probe_at_mmio` extraído; fallback no probe storage → MESMO
#     global NVME_DRIVER (bin intocado). Gates: 0 erros, k-nano 237. HW lab
#     pendente: `nvme ok=true via=vmd` + BOOT.LOG persistindo no NVMe.
#   PISTA ANTERIOR: s421 — lane VRAM POR SEQUÊNCIA (layer*7+slot) corrige os 3 bugs de
#     corretude do s418 (SHAPE_INDEX servia layer 0 p/ todas; GEMV coluna trocada;
#     escala única p/ m>1). Upload sem dedupe + pré-checagem INTEIRA (parcial = off
#     honesto). Dispatch seq no apply_one_layer (7 matmuls) + proteção de shape.
#     HONESTIDADE: heap NÃO encolhe (bump sem free; pesos já no boot) — liberação
#     real exige loader-VRAM (residual, lab). QEMU 8G/6c: lane off honesto (sem
#     aperture), CPU ladder intacta, zero #PF, grow máx 1536MB. s420c: consumidor
#     HINTS (tint orb/dock/cards); s420b: train_hint_mlp + loader HINT.BIN;
#     s420: gate fail-closed headroom_low 128MB no slice (zero #PF em produção)
#   PISTA ANTERIOR: s418 — BAR Compute (pesos W2A8 residem na VRAM via BAR, GEMV host lê aperture,
#     lane VRAM no dispatch; StreamsW2a8/ComputeDevice = upgrade; lab GTX 1050 = residual)
#   PISTA ANTERIOR: s410m — forget cognitivo HITL (/forget) + leitura de conflitos (/conflicts)
#     via ApprovalGate Escalate (skills sgdb_forget/conflict_resolve); tombstone Superseded
#     antes do delete físico; fail-closed NSGDB down; registry pendente cap 32 FIFO;
#     s410m-b: elo AUDIT_OP_FORGET na hash-chain sys/audit/ do SGDB (audit_forget upstream
#     + audit_verify_nsgdb) — evidência do esquecimento sobrevive à memória apagada
#     s410m-c: put_kv sem dual-write — sync_write roteado: md/ no-op (domínio put_doc);
#     não-md via import_record (sem tick do clock local, indexa ART/BQ/lexical)
#     (s410l CRDT merge_remote; s410k mesh RX batch; s410j compact batch; s410i interop TKLV)
#   PISTA ANTERIOR: s406 heap auto-fracionado advisory (commit 2b650c56)
#   Não declarar v2.0.0

## BAR Compute (s418, ADR-0112) [OK — stage Mapped]

| Item | Estado |
|------|--------|
| vram_stream.rs (ring + canário golden TSC) | OK - 4 slots × 8×9216 i8; telemetria lock-free |
| bar_compute.rs (upload + GEMV via BAR) | OK - upload determinístico, SHAPE_INDEX, read_volatile + prefetcht0 |
| Lane VRAM no dispatcher (cortex::compute) | OK - antes do GPU device; N_VRAM telemetria |
| Seam upload pós-load (set_model) | OK - register_vram_upload_hook + layers_snapshot |
| Honestidade | OK - stage Mapped sem note_gpu_compute; iGPU=Off (DRAM compartilhada) |
| Compat vendor | OK - NV BAR1 / AMD BAR0=VRAM / iGPU Off / D3 recusa (s260) / QEMU VGA fail-closed |
| check bare-metal + testes | OK - 0 erros; k-hal 232, cortex 105, k-nano 232, hermes 257 (t=1) |
| Lab: round-trip GTX 1050 + decode longo | residual (RAM/estabilidade, NÃO tok/s) |
| StreamsW2a8 + ComputeDevice (CE/SDMA/BCS) | upgrade path ADR-0112 §7 |

## Federation de saúde + OOM fail-closed (s417) [OK parcial]

| Item | Estado |
|------|--------|
| Diagnóstico freeze/#PF | OK - 1 OOM (bump sem free → NULL → #PF → hlt no AP); carimbo ≠ culpado; crash site migra com fix |
| Fail-closed infer_queue | OK - claim recusado sob headroom crítico (64MB) + job em curso → Finishing honesto (10/10) |
| Fail-closed MoE lifecycle | OK - gate `heap_headroom_critical()` em bei_tick + flush_merges/splits/births |
| Freeze UI (tick lock) | OK - `try_with_agent_tick_lock_ms(2)` display first+boost; SCHED gap 10761→3351 |
| Hybrid allocator | PARCIAL - bump-first+TALC overflow; TALC-first stallou boot; diagnóstico `ALLOC null` pronto |
| Modelo puro máquina/frota | OK - worst-of NO_GO>UNKNOWN>GO, machine_verdict_json, parse no_std, fleet_worst (10/10) |
| Consolidação hermes | OK - MACHINE_HEALTH 1Hz + machine_prompt único + TX MCH\0 (cooldown 10s) |
| Fleet no Master | OK - fleet_health RX MCH\0 (array 16 slots, evicção), FLEET_HEALTH + escala 1×/incidente (4/4) |
| QEMU validação | OK - MCH TX 25 pubs, zero PF_DBG, a2_proof completo; boot passou do stall anterior |
| Stall silencioso pós-teto | OK - SmpMmGuard (s419) + gate headroom_low 128MB no slice (s420); validado QEMU: a2_proof completo, grow máx = teto, zero #PF |
| Federation e2e dual-node | ⏳ - RX de frota com MCH alheio real + FLEET_HEALTH no HUD |

## KV-INT8/paginado (s414) [OK]

| Item | Estado |
|------|--------|
| `KvCache` INT8 (bloco 64, escala f32) | OK - `k`/`v` -> `Vec<Vec<i8>>`; `k_all`/`v_all` dequantizam (assinatura + guard SESSION_351 intactos) |
| Ganho de memoria | OK - **3,76x** (nao 4x: a escala f32 por 64 valores conta) |
| Storage paginado | OK - `KV_PAGE=4096` i8; paginas nunca realocam (evita memcpy de ate ~152 MB) |
| Evidencia (P0) | OK - KV do 1B ctx 4096 = **603.979.776 B > modelo (544 MB)**; atencao ~2% do prefill |
| Runtime (P3) | OK - log `KV kv_mem ... int8_used=KB int8_alloc=KB f32_would_be=KB` no fim do prefill |
| Medicao no lab (`attn` por ctx 1K/2K/4K) | residual |
| Testes | OK - `cargo test -p cortex --lib kv` 6/6; `cargo check --release` 0 erros |

## Perf SMP (s413) [OK]

| Item | Estado |
|------|--------|
| T1 sync do SMP | REFUTADO - o "4,2 ms" era o **log de entrada**; dispatch real **~60 us** (TSC depois do log) |
| T2 threshold tiny f32 | NEUTRO (< variancia do host); revertido; economia real ~0,5% do prefill |
| T5 fusao SwiGLU in-place (`cortex.rs`) | inconclusivo (confundido pelo host); mantido por ser simplificacao (menos alloc/copia) |
| Bancada de perf | variancia run-to-run 7-8% -> efeitos <10% precisam host controlado ou n repeticoes |
| ADR-0111 KV-INT8/paginado | PROPOSED (IDEA #613) - KV do 1B (604 MB) **> modelo** (544 MB); atencao ~2% do prefill -> ganho e memoria/ctx longo; destrava #617 |
| T3/T4/T6/T7 (worker) | bloqueados na bancada (efeito esperado <10%) |

## Perf SMP - tile do worker ternario (s412) [OK]

| Item | Estado |
|------|--------|
| Tile de colunas (formula de tile de LINHAS) | OK - `clamp(8,256)` -> `clamp(32,256)`; 8 col = 2 B/linha de cache (64 B) |
| Worker k=2048 n=8192 (Falcon3-1B, m=8) | OK - 204 -> **105 ms** (1,9x); prefill 20,9 -> **12,7 s** (1,65x) |
| Sweep do piso | OK - 8/16/32/64/128 -> 204/154/105/130/127 ms (32 = fit L1: 16 KB strip + 8 KB x) |
| AVX2 unpack 2-bit | REFUTADO - **12x PIOR** no alvo soft-float (nao emite AVX2; f32 vira libcall); deletado |
| Sync SMP ~4,2 ms/dispatch | conhecido - custo FIXO (ternario E f32); bypass no ternario medio = regressao (~4x worker) |
| Threshold nos tiny f32 (k=8 n=256, 14x ~4,2 ms) | residual (nao implementado - decisao ponytail) |

## A2 proof - marco zero (s411) [OK]

| Item | Estado |
|------|--------|
| FALCON3-3B 1 token real (QEMU 6G/4c WHPX) | OK - `a2_proof done id=2 toks=1 prefill_us=69042824` (BOOT.LOG em disco) |
| Hang do prefill (travava em layer 1/22) | OK - lost wakeup do AP idle (`ui_yield` + `hlt` sem IPI); bounded retry + TOCTOU (`ap_work.rs`) |
| Mesh 2 nos | OK - converge (node_id 2/3, Master/Memory, peers=1); netmode `0x16400000` |
| Token legivel (BPE) | OK - BPB1 Falcon3 131072 @0x150000000 -> `JARBAS: quad` (era `)`) |
| Barrier SMP sem deadline | OK - deadline 60 s + fallback single-core honesto |
| posture FAIL de 1 amostra | OK - `POSTURE_MIN_SAMPLES=4` -> `n/a` |
| 2o token (forward de decode) | residual (max_gen>1) |
| prefill 3B ~7 s/layer (SMP) | alavanca: W2A8/GPU/AP-IDT |

## Lab vivo (s410)

| Item | Estado |
|------|--------|
| GOAL1 6c/5min (`tools/goal1-6c-5min.ps1`) | ✅ PASS (5115 ok / 236 warn / 2 fail conhecidos) |
| Mesh 6 workers c–f (2G) | ✅ estáveis T+122k+ pós-fix (RX MEM 648→2) |
| Mesh 6 Master (a) pós-fix | 🟡 re-test pendente (rodada anterior usou imagem pré-fix; OOM T+141k pré-fix) |
| KVM+VFIO GTX 1050 (`tools/run-qemu-kvm-vfio.ps1`) | 🟡 script pronto (canários ADR-0105 preservados); AWAITING host Linux IOMMU |
| GPU compute NVIDIA (ADR-0105 B1.3/B2.4) | ▶️ AWAITING_HW (CpuOnly default; VFIO lab s410c) |
| sev audit Sev::from_sub | ✅ s409+s410 fecho (23 subs → Ok, dbg→Trace; 2 emissores corrigidos) |
| I3 fantasma | ✅ fix note_trust_entries (push pattern hermes→k_ai) |
| Runtime hygiene (`docs/architecture/runtime-hygiene-checklist.md`) | ✅ s410d: 7 estruturas corrigidas + ~19 auditadas com cap |
| Motor único SGDB (sem dual-truth engine) | ✅ s410h: AiosDatabaseEngine deletado; layers/store/e2e via NSGDB externo |

## Gate ADR-0100 (Trilho A) — checklist vivo

| Onda | Item | Estado |
|------|------|--------|
| 0 | Honesty BOOT_AI T-001–T-006 | ✅ |
| 1 | I/O + HardwareInfo (mín. T-011) | ✅ |
| 2 | SMP metal K23 `online==madt-1` | ▶️ AWAITING_HW / operador |
| 3 | OTA A2 **ou** A3 | `[ ]` lab A2 próximo |
| Review | ADR formal + OK humano | `[ ]` |

## Aceite metal quente (bloqueia A)

| Item | Estado |
|------|--------|
| `E:\BOOT.LOG` real | ▶️ AWAITING_OPERATOR (s389b: fat-boot-log ON default) |
| Freeze `@ network_agent` | ABERTO |
| xHCI MSC PP pós-HCRST | 🟡 wired; validar metal |

## Trilho B (produto 1.x)

| ID | Item | Estado |
|----|------|--------|
| s391 | Desktop/UI/Orb theme+hover+dock+mesh honesty | ✅ |
| s390b | SelfHeal residual KERNEL_ERROR/Safety/checkpoint/SKILL_CREATE | ✅ |
| s390 | SelfHeal honesty I3/budget/LLM/silent | ✅ |
| s389b | Logging follow-up SCORE/phase/fat-boot-log | ✅ |
| s389 | Logging QEMU+HW ADR-0092 honesty | ✅ |
| s388 | ModelHub + Trinity CapGate/mmap honesty | ✅ |
| s387 | Falcon3 LLM header/RoPE/hub honesty | ✅ |
| s386 | W2A8/KernelPack honesty High→Low | ✅ |
| s385 | SGDB/Tickv/NSGDB bughunt High→Low | ✅ |
| s384 | LazyLock + smoltcp 0.14 + wasmi 2.0 | ✅ |
| IDEA #544 | Falcon3-3B lab ADR-0101 | 🟡 Onda 0 parcial (header Observe ✅) |
| IDEA #549 | GGUF tokenizer/KV 32K | ⏳ residual |
| IDEA #485 | F4 CPU W2A8 ladder | ✅ s386 (device = ADR-0105 AWAITING) |
| IDEA #550 | NKP B0–B3 | 🟡 B1.2/B4 AWAITING_HW |
| #558b/#559b/#561b | aceite voz metal | residual paralelo |
| B2 | Mesh peer B | AWAITING operador |
| #607 | Efeito Matrix mmap expert residual | ⏳ |
| #608 | slog info restante → canónico | 🟡 parcial s391 (UI slog) |
| s410 | Mesh 6 OOM bughunt (worker+Master anti-bloat) + sev fecho + GOAL1 6c/5min | ✅ SESSION_410 (Master re-test pendente) |
| s409 | Docs/Governança estudo ternário+GPU HAL; sev s409 (reg/query/select/state/revoke) | ✅ SESSION_409 |
| s392 | Boot/Limine bughunt H1–H5/M1–M5/L1–L3 + canvas | ✅ SESSION_392 |
| s393 | MHI bughunt docs-only (fix-1/2/3 + canvas + pins talc/x86_64) | ✅ SESSION_393 + IDEA #609; código = outros lanes |
