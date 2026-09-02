# SESSION_305 — QEMU 4c loop + P6 iretq PASS + Jarbas greeting (TCG)

**Data:** 2026-09-02 | **Sprint:** v1.9.99-s305 TEST | **Status:** ✅ PASS parcial forte+ (QEMU TCG 4c)

---

## Objetivo

Rodar loop QEMU 4 cores (TCG) até **frontend visível + saudação TTS Jarbas**; desbloquear hangs de boot; corrigir demo P6 `ring3_can_iretq`.

## Causas raiz (hangs resolvidos)

1. **BOOT.LOG persist** re-probe xHCI/ATA PIO no `uefi.img` após PS2.
2. **NeuralFS / FileFlash** format/probe no disco ATA de boot no TCG.
3. **Labor smokes + TransformerTrainer::self_test** (~12 min hang em 4c).
4. **Logs Jarbas/P6 em `sub=info`** → invisíveis na serial (parecia hang pós-greeting).

## Fix P6 Ring3 (iretq + int 0x90)

| Sintoma | Causa | Fix |
|---|---|---|
| `#GP ip=0x700000300019` | `int 0x90` com gate IDT DPL=0 | `idt[0x90]` DPL=3 em `k_nano::interrupts` + `patch_idt()` antes dos demos P6 |
| `marker Ring3 nao escrito` | Marker em offset +32 = campo `result` da mailbox; `syscall_finish_ok(0)` zera | Marker demo em **+48** (após `SyscallMailbox` 48B) |
| Mailbox nr zerada no handler | `syscall_stage_from_mailbox` sobrescrevia stage | Preservar stage quando `m.nr==0 && m.cap==0` |

## Evidência QEMU (4c TCG, 4G, virtio-blk + loader FALCON3)

```text
tools/run-qemu-4c-loop.ps1 -Cores 4  → exit 0 (~40s goal)
logs/qemu4c_clean.txt              → 197 linhas

[Jarbas] saudacao suit-boot @register K44
[Jarbas] TTS boot greeting 168160 frames
[P6] Ring3 user-mode demo OK
[P6] ring3_can_iretq=true
PHASE AgentFleet / Runtime / PostRuntime OK
[MOUSE] desktop_ready
FileFlash NSGDB virtio-blk backend=file
```

Demos fault-containment e SSE ainda emitem `[P6] WARN fault abort` + `#PF/#UD` — **esperado** (contenção non-fatal no TCG).

## Arquivos tocados

- `crates/k_nano`: `interrupts.rs`, `paging.rs`, `boot_logger.rs`, `storage_bus.rs`, `storage/flash.rs`, `neural_fs/neural_fs_agent.rs`
- `crates/neural-kernel/src/main.rs`: gates `tcg_lite`, skip trainer, P6 logs visíveis, BGE log honesto
- `crates/jarbas/src/audio/jarvis.rs`: greeting/TTS → `sub=ok`
- `tools/run-qemu-4c-loop.ps1`: loop 4c + markers goal

## Validação

```powershell
cargo build --release
powershell -File tools\run-qemu-4c-loop.ps1 -Cores 4 -TimeoutSec 900
# goal: saudacao + P6 ring3_can_iretq=true + Runtime + desktop_ready
```

```text
cargo check --release -p neural-kernel  → 0 erros
```

## Lições

1. **`int 0x90` de CPL3 exige gate DPL=3 na IDT** — `patch_idt` no bin não basta se falhar; instalar também em `k_nano::init_idt`.
2. **Marker P6 ≠ mailbox `result`** — N4 mailbox ocupa 48B; demo marker em `USER_MAILBOX_VA+48`.
3. **Magic BitNet 0xBE11BE11 ≠ BGE** — log honesto evita falso alarme no scan loader @0x100000000.
4. **Loop script para cedo no goal** — saudação dispara ~30s; P6/Runtime podem precisar de boot completo (~180s) para validar Ring3 isolado.

## Próximo

- Aceite HW real T-052/053 (`ring3_mark_hw_gate_passed` + pendrive unified).
- Piper/LLM no `disk_qemu.raw` para TTS com voz real (hoje formant drain).
