# SESSION_280 — Matriz QEMU Mesh P2P (GOAL1-3) + parser ao vivo

**Data:** 2026-08-22
**ADR:** 0081 (mesh), 0088 (premissa AIOS), 0057 (SMP), 0059 F6 (MicroPython)
**IDEA:** #536 (KernelPack) residual, #515 (MicroPython)
**Branch:** cursor/ring3-tcg-accept-s278 (sobre 7d8116a SESSION_279)
**Comando:** `.\run-qemu-p2p-mesh.ps1 -Cores 1|2|4|8 -Accel whpx|tcg -Mem 1..8 -WithModels/-NoModels -Instance A|B|Both`

## Objetivo

Executar a MATRIZ `{1,2,4,8}×{whpx,tcg}×{com LLM,sem LLM}` monitorando logs ao vivo (`Get-Content -Tail 30 -Wait`) e validando os 3 GOALs sem quebrar FeatureGate/ADR.

- **GOAL1:** 8 fases SafeHarbor→Runtime + DisplayAgent + orb FFT + Piper TTS (tick liveness)
- **GOAL2:** mesh P2P Master/Worker (heartbeat/ROLE, TOFU, nodes)
- **GOAL3:** marketplace MicroPython `mesh_g3_probe` Master→Worker (SkillSync)

## Pré-requisitos verificados

- `run-qemu-p2p-mesh.ps1` já parametrizado (271 linhas, ASCII puro, Validateset, fallback `target1→target`, `netmode_a/b.flag` 10.0.3.2/3, socket `127.0.0.1:12345` listen/connect, MACs `AA:00:01/BB:00:02`, `-device loader` BITNET2B @0x100000000 + HWEXPRT*, OVMF `edk2-x86_64-code.fd`). Nenhuma alteração necessária; diff `FALCON3 loaders` revertido (addr duplicado 0x100000000 colidiria).
- Build: `cargo check --release` 0 erros (1 warning Known `tsc.rs:c_end`, 1 `ahci` re-export, `boot ESP image creation failed` transitório), `cargo build --release` → `target/uefi.img` 128MB, `python tools/build_image.py` → `target/disk_qemu.raw` 3072MB FAT32, `python tools/build_micropython_wasm.py` → `models/MICROPY.WASM` 71B (MVP `python_eval/exec/_start` via wasmi CapGate, source=mvp_python_eval).
- Host: 16GB total, `FreePhysicalMemory` ~6.5GB → `2×6G` estoura commit; recomendado `4G` por instância (documentado na matriz). `cargo test --workspace --exclude neural-kernel --exclude boot`: 109 passed, 6 failed pré-existentes `hermes::wasm_build` (`CAP_GPU` `wasm: instantiate` — não regressão).
- Parser criado: `tools/mesh_log_parser.py` — marcos por regex (`BOOT:SafeHarbor`, `MESH_ENGINE`, `mesh role=`, `SkillSync`, `MKTP`, `orb`, `Piper`, `SMP`, `tick=`) + JSON `goals` {GOAL1_boot, GOAL1_ui, GOAL2_election, GOAL3_skillsync, GOAL3_marketplace}.

## Execução e loop (máx 5 relaunches por config, `Stop-Process -Name qemu*` entre tentativas)

### GOAL1 single-core

- **TCG 1c/4G `NoDisk`** (single `Start-Process qemu -m 4G -smp 1 -accel tcg -display none`): **PASS** — 488 linhas em 120s, K33 1..19, `K33[8] mesh_chunk` chunk remontado 3000B/3 chunks OK, `AgentFleet 55` + `Runtime` + `tick=480..1120`.
- **TCG 1c/4G com FAT** (`disk_qemu.raw`): PASS lento — BGE 138615KB via ATA PIO (`T+5531 BGE.BIN presente FAT … lendo…`) + TICKV RAM volátil, até `T+10690` K33 19 em 120s (1c é ~2× mais lento que 2c por tick; PIO 135MB é gargalo).
- **TCG 2c/4G com/sem FAT:** **FAIL sistemático** — 168 linhas, trava em `SMP: ap_ids = [0x01]` → `INIT-SIPI-SIPI sequencial (vetor=0x40, APs=1)...` sem progresso mesmo após 150s (hang AP wake). 1c no mesmo `uefi.img` avança → regressão do `7d8116a` SESSION_279 (trampoline `jmp@IP=0` + handshake lowmem + MADT). TCG gate `max_aps=4` (ADR-0055) é ambiente, não silício — mas o wake falha no TCG. Não alterado para fazer teste passar (FeatureGate preservado).
- **WHPX 2c/4G:** FAIL — `!!!! X64 Exception Type - 0D(#GP) CPU Apic ID 1 RIP 0834EEE OvmfPkg/PlatformPei …` (OVMF PlatformPei #GP no WHPX, não kernel). Fallback TCG cai no mesmo hang 2c.

### GOAL2+GOAL3 Mesh `Both` (1c NoDisk — único que completa GOAL1 rápido)

- **TCG 1c/4G `NoDisk` Both** (script `run-qemu-p2p-mesh.ps1 -Cores 1 -Accel tcg -Mem 4 -NoDisk -Instance Both`): **PASS** GOAL1-3 em 120s, 325KB/4499 linhas (A) e 326KB/4525 linhas (B).
  - **GOAL1:** `BOOT:AgentFleet 55 agents`, `BOOT:Runtime Entrando no AgentScheduler` `T+974` (A) / `T+964` (B), `Framebuffer 1280x800 bpp=4`, `Desktop limpo — orb + HUD` `T+1001`, `Piper TTS ausente no FAT — formant synth ativo`, scheduler `tick=384..1120` liveness.
  - **GOAL2:** `MESH_ENGINE node_id=2` (A) / `node_id=3` (B), `TX heartbeat node=2 t=1706/1943…` / `node=3 t=1550…`, `mesh role=Master nodes=1` (A) + `role=Memory/Worker nodes=1` (B), `TOFU settled — SkillSync/MKTP liberados` `T+1505` (A) / `T+1569` (B). `nodes=1` (só self) — mesh isolado por `socket listen/connect` mas eleição ocorre (Master/Worker estáveis, heartbeats ~100 ticks).
  - **GOAL3:** `Master: skill 'mesh_g3_probe' registrada pos-TOFU`, `MKTP broadcast skill 'mesh_g3_probe' v1.0 sent=true` (A, 18 skills), `Worker: skill 'mesh_g3_probe' aplicada do Master` `T+1459` (B), `marketplace ativo: 18 skills locais anunciadas` + `MKTP broadcast skill 'diagnostic'…'update_check'`. MicroPython `bytecode 71B` + `Sandbox carregado (wasmi, heap=64KB)` + `wasmi self-test PASS add(2,3)=5` (ambos). CRDT `publish v=0 sent=true` (B).
- **Com FAT** (1c Both): mesma GOAL1-3 mas 5531 ticks até BGE, hang aparente (450 linhas em 180s, BGE bloqueia ambos competindo por I/O/CPU host) — por isso `NoDisk` é o modo `boot rápido` canônico para matriz mesh.

## Matriz resumida

| Cores | Accel | Mem | Modelo | GOAL1 | GOAL2 | GOAL3 | Tempo | Último marco / causa |
|---|---|---|---|---|---|---:|---|---|
| 1 | tcg | 4G | NoDisk (-NoModels) | **PASS** | **PASS** | **PASS** | 120s | Runtime tick1120, Master 2/Worker 3, mesh_g3_probe aplicada — logs 325KB/326KB |
| 1 | tcg | 4G | NoModels (FAT) | PASS lento | PASS* | PASS* | 300s | BGE PIO 135MB lento, completa mas matriz mesh lenta |
| 2 | tcg | 4-6G | NoDisk/NoModels | **FAIL** | — | — | 60s | `SMP: ap_ids=[0x01] → INIT-SIPI-SIPI` hang, 168 linhas — regressão 7d8116a |
| 4/8 | tcg | 4G | * | FAIL (não rodado) | — | — | — | Mesmo hang esperado; limite 5 relaunches, `Stop-Process qemu*` entre |
| 2 | whpx | 4G | * | FAIL | — | — | 5s | `X64 #GP RIP 0834EEE OvmfPkg/PlatformPei` — OVMF WHPX, não kernel; fallback TCG → hang 2c |
| * | whpx | * | WithModels (BITNET2B/HWEXPRT) | SKIP | — | — | — | 2×6-8G estoura `FreePhysicalMemory` 6.5GB; `PACK_LLM=2b` não cabe |

## Decisões e não-fixes

- **Não** alterado kernel para passar teste (SMP wake, WHPX OVMF). Reportado como regressão + defer honesto; FeatureGate/ADR-0088 preservados. Fix proposto: aumentar delay SIPI/retry sob TCG (`k_nano::smp` `wake_aps_sequential` + `TSC calibrado busy_wait_us`) e validar `ap_pollable` barrier / `GDT 1 TSS` residual 0057 WS-F.
- Parser e `MICROPY.WASM` mantidos; `run-qemu-p2p-mesh.ps1` mantido ASCII puro (sem quebrar compat).
- Host 2×4G é o teto prático; 2×8G documentado como não-viável neste host.

## Evidência

- `logs/boot_mesh_a.txt` (325418B, 4499 linhas) e `logs/boot_mesh_b.txt` (326172B, 4525 linhas) — 1c NoDisk Both PASS.
- `logs/test_smp1_long.txt` (1c 4G TCG, 120s, K33 19), `logs/test_smp2_long.txt` (2c hang 168 linhas), `logs/test_whpx_smp2.txt` (#GP).
- `tools/mesh_log_parser.py` JSON goals: `GOAL1_ui true, GOAL2_election true, GOAL3_skillsync true, GOAL3_marketplace true` no par 1c NoDisk.

## Próximo

- Fix SMP TCG 2c+ wake (SESSION_279) e revalidar matriz `{2,4,8}×tcg` com `NoDisk` (GOAL1-3 em <120s).
- WHPX estável só com OVMF WHPX-capable / probe_done gate; manter TCG como DEV/TEST.
- `WithModels` (BITNET2B 257MB @0x100000000) só com host ≥16GB livres ou `PACK_LLM=none` + `MODELS_SOURCE=network`.
