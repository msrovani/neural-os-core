# STATE - neural-os-core v1.9.99-s412 - ternary_worker SMP: tile de colunas (1,9x) + AVX2 refutado

#   PISTA ATIVA: s410m — forget cognitivo HITL (/forget) + leitura de conflitos (/conflicts)
#     via ApprovalGate Escalate (skills sgdb_forget/conflict_resolve); tombstone Superseded
#     antes do delete físico; fail-closed NSGDB down; registry pendente cap 32 FIFO;
#     s410m-b: elo AUDIT_OP_FORGET na hash-chain sys/audit/ do SGDB (audit_forget upstream
#     + audit_verify_nsgdb) — evidência do esquecimento sobrevive à memória apagada
#     s410m-c: put_kv sem dual-write — sync_write roteado: md/ no-op (domínio put_doc);
#     não-md via import_record (sem tick do clock local, indexa ART/BQ/lexical)
#     (s410l CRDT merge_remote; s410k mesh RX batch; s410j compact batch; s410i interop TKLV)
#   PISTA ANTERIOR: s406 heap auto-fracionado advisory (commit 2b650c56)
#   Não declarar v2.0.0

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
