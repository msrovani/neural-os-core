# STATE — neural-os-core v1.9.99-s410 — Mesh 6 OOM bughunt + sev fecho

#   PISTA ATIVA: s410 — mesh 6 lab: dedup fnv1a64 TX/RX (RX MEM 648→2), workers 2G estáveis T+122k+;
#     Master anti-bloat (loop I4 cortado + caps obs/req/mkt); I3 note_trust_entries; sev s409+s410 mapeadas
#   PISTA ANTERIOR: s406 heap auto-fracionado advisory (commit 2b650c56)
#   Não declarar v2.0.0

## Lab vivo (s410)

| Item | Estado |
|------|--------|
| GOAL1 6c/5min (`tools/goal1-6c-5min.ps1`) | ✅ PASS (5115 ok / 236 warn / 2 fail conhecidos) |
| Mesh 6 workers c–f (2G) | ✅ estáveis T+122k+ pós-fix (RX MEM 648→2) |
| Mesh 6 Master (a) pós-fix | 🟡 re-test pendente (rodada anterior usou imagem pré-fix; OOM T+141k pré-fix) |
| KVM+VFIO GTX 1050 (`tools/run-qemu-kvm-vfio.ps1`) | 🟡 script pronto (canários ADR-0105 preservados); AWAITING host Linux IOMMU |
| GPU compute NVIDIA (ADR-0105 B1.3/B2.4) | ▶️ AWAITING_HW (CpuOnly default; VFIO lab s410c) |
| sev audit Sev::from_sub | ✅ s409+s410 fecho (23 subs → Ok, dbg→Trace; 2 emissores corrigidos) |
| KVM+VFIO GTX 1050 (`tools/run-qemu-kvm-vfio.ps1`) | 🟡 script pronto (canários ADR-0105 preservados); AWAITING host Linux IOMMU |
| I3 fantasma | ✅ fix note_trust_entries (push pattern hermes→k_ai) |

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
