# Plano mestre — Fechamento neural-os-core 1.x → gate v2.0.0

**Data:** 2026-09-19  
**Base:** v1.9.99-s367 TEST · ADR-0100 §5 · TODO.md · gap audit s360+  
**Premissa:** “Fechar 1.x” ≠ implementar Layer S/HW inteiro. Gate v2.0.0 = Ondas **0–3** (mín.) + review ADR + OK humano. Ondas 4–10 e AWAITING_HW entram como **Track C (pós-gate / defer explícito)** ou fecham se o maintainer exigir “1.x completo produto”.

---

## 0. Escopo e trilhos

| Trilho | Objetivo | Sai 1.x? |
|--------|----------|----------|
| **A — Gate ADR-0100** | Onda 0✅ + 1✅ + 2 metal + 3 (A2 **ou** A3) + review + OK | **Obrigatório** para declarar caminho a v2.0.0 |
| **B — Produto 1.x** | CI verde, mesh lab estável, voz mínima honesta, desktop S5 leve, storage honesty | **Recomendado** antes do freeze tag `v1.9.99` / pré-v2 |
| **C — Layer S / pós-gate** | Ring3 register, App B/C, CapGate DMA, W2A8 GPU, NPU, WiFi RF, AirLLM DMA, VITS pleno | **Fora do gate** salvo waiver inverso (“tudo ou nada”) |

**Regra de honesty (ADR-0088/0092):** nada fecha com slog mentiroso; AWAITING_HW + defer na STATE **não bloqueia** gate (ADR-0100 §5).

**Dependência dura:**
```
Aceite metal (BOOT.LOG + sem freeze network_agent + MSC)
        │
        ▼
0100 Onda 2 (SMP K23) ──► 0102 T-052/053 (Ring3 metal) ──► 0103 Fase 2 schemes
        │
        └──► 0089/0092 aceite metal
```

---

## 1. Trilho A — Gate mínimo (fecha 1.x → permite v2.0.0)

### A0. Freeze de honestidade (1–2 dias)
| ID | Trabalho | Teste / evidência | Aceite |
|----|----------|-------------------|--------|
| A0.1 | Inventário `por_fazer` em TODO/STATE/ADR-0100 T-001–T-075 | Script: `tools/check_todo_gate.py` (novo) lista `[ ]` Onda 0–3 | Tabela STATE “gate checklist” |
| A0.2 | Alinhar TODO “6 testes host” com realidade (já CLOSED em hermes/jarbas isolado) | `cargo test -p hermes --lib`; `-p jarbas soul_*` | Doc sync |
| A0.3 | Licença MIT vs AGPL — decisão maintainer | Commit único LICENSE + TECNOLOGIAS | Sem drift |

### A1. Onda 2 — SMP metal (T-017…T-021)
| ID | Trabalho | Teste / evidência | Aceite |
|----|----------|-------------------|--------|
| A1.1 | Imagem unified fresca `PACK_LLM=2b` (ou none se só K23) | `build_image.py --hw --unified --build-boot` | `usb_hw.img` hash em SESSION |
| A1.2 | Boot Alienware (i5) — log K23 `online == madt_enabled - 1` | Serial/FB + `E:\BOOT.LOG` | T-018 PASS ou SESSION+hipótese |
| A1.3 | Segundo silício (240H) **ou** defer HITL explícito | Idem | T-019 PASS **ou** STATE defer |
| A1.4 | Confirmar ICR/x2APIC sem bits ilegais | Grep BOOT.LOG | T-020 |

### A2. Aceite metal pós-s328 (bloqueio quente)
| ID | Trabalho | Teste / evidência | Aceite |
|----|----------|-------------------|--------|
| A2.1 | Discriminador FB: heartbeat `T=` avança após tick~1370 | Foto FB + BOOT.LOG | Sem freeze `@ network_agent` |
| A2.2 | xHCI MSC: PP pós-HCRST → `CCS` + `E:\BOOT.LOG` cresce | Diff size BOOT.LOG pré/após Runtime | S1 0103 |
| A2.3 | InferQueue + UI: orb vivo + 1ª frase TTS antes de `LLM_RESPONSE` | Lab metal checklist HW_DIAG_s316 | Aceite s328 |

### A3. Onda 3 — OTA/install (A2 **ou** A3)
| ID | Trabalho | Teste / evidência | Aceite |
|----|----------|-------------------|--------|
| A3.1 | **Preferido A2:** `serve_update.py` → guest Ato 1–3 | Serial + HTTP 200 em `docs/evidence/` | T-022/T-023 |
| A3.2 | **Alt A3+A7:** `NET_READY` → ModelProvisioner + Cron POST logs | QEMU lab script | T-024…T-026 |
| A3.3 | Menu Live/Install default Live (sem wipe) | Teste host máquina de estados + QEMU | T-028/T-029 |
| A3.4 | Rollback `tries` 1→3 + `last_good` | `cargo test` host FSM | T-030 |

### A4. Review formal + OK humano
| ID | Trabalho | Aceite |
|----|----------|--------|
| A4.1 | Checklist ADR-0100 §5 + ADR INDEX conflicts | Doc `docs/evidence/v2-gate-review.md` |
| A4.2 | Maintainer assina STATE: “gate v2.0.0 OK para tag” | Commit + tag pré-v2 |

**Saída Trilho A:** tag `v1.9.99-gate` (ou `v1.10.0-rc1`) — **não** ainda `v2.0.0` sem OK explícito.

---

## 2. Trilho B — Produto 1.x (features + testes antes do freeze)

### B1. CI / host tests (semana 0, paralelo a A)
| ID | Feature / fix | Testes |
|----|---------------|--------|
| B1.1 | Flaky parallel: SESSION clear + CapGate lock (já no tree) | `cargo test -p hermes --lib -- --test-threads=8` → 188/188 |
| B1.2 | Workspace verde | `cargo test --workspace --exclude neural-kernel --exclude boot --no-fail-fast` → **0 fail** |
| B1.3 | Fixture GGUF | `python tools/gen_test_gguf.py` no CI pré-test; artefato `target/test_tq2_0.gguf` |
| B1.4 | Guarda duplicação | `python tools/check_duplication.py` exit 0 |
| B1.5 | Scripts lab ASCII-only | `run-mesh3-lab.ps1`, `run-qemu-whpx.ps1` parseiam no PS 5.1 |
| B1.6 | AutoLearn/SleepCycle | Parser `measure_learn_sleep_logs.py` VERDICT OK; opcional FAT `SECURITY.BIN` → `learn_done status=ok` |

**Gate B1:** CI job “host-tests” vermelho = bloqueia merge no freeze.

### B2. Mesh lab (ADR-0081)
| ID | Feature | Testes / lab |
|----|---------|--------------|
| B2.1 | Peer B estável em 6-node | `tools/run-qemu-p2p-mesh.ps1` + parser; 3 relaunches sem hang PS/2 |
| B2.2 | Re-boot pós-s360 | Operador: relaunch mesh6; SESSION com roles Master/Worker/Memory |
| B2.3 | Mesh3 METRIC contínuo | Já: probe+sleep; estender checklist roles `nodes≥2` |
| B2.4 | Mesh → HW LAN | **Defer Track C** se sem switch dedicado; senão lab 2 notebooks + hub |

### B3. Voz (ADR-0045) — mínimo honesto 1.x
| ID | Feature | Testes |
|----|---------|--------|
| B3.1 | **V1** Playback pacing por clock (`min(free_frames, Δt×rate)`) | Host unit mixer; QEMU: `PLAY_SAMPLES_DROPPED==0` em tick 30/60/120 |
| B3.2 | **V2** Duplexidade: sem cancel inferência no eco; `STT_UNCERTAIN` → TTS/UI | Teste host state machine; lab serial |
| B3.3 | **V3** Ring SPSC `AUDIO_FRAME` + teto utterance | Testes host ring; sem leak EventBus |
| B3.4 | **V4** Wake: janela deslizante + artefato treino | `tools/train_wakeword.py` + holdout FPPH; remover claim 98% sem artefato |
| B3.5 | UAC isócrono / VITS pleno | **Track C** (AWAITING / soft-float) |

### B4. Storage / FAT
| ID | Feature | Testes |
|----|---------|--------|
| B4.1 | FAT pins/FSInfo | Já CLOSED s345; regressão QEMU 8c boot past K33 |
| B4.2 | Tickv backend file (virtio/NVMe) honesty | Host + QEMU: `backend=file` ≠ RAM sem WARN |
| B4.3 | NSGDB metal | AWAITING_OPERATOR: `NSGDB.BIN` não-zeros no stick |
| B4.4 | Órfãos ntfs/btrfs/ext2 | **Delete ou crate** + `check_duplication` (S3 0103) |
| B4.5 | NVMe/AHCI layouts | Honesty timeouts já; golden QEMU `-device nvme` smoke |

### B5. GPU / Infer (código 1.x, golden HW = C)
| ID | Feature | Testes |
|----|---------|--------|
| B5.1 | InferQueue + Prefill layer-yield | Host `infer_queue` 5/5; lab `lab-llm-response.ps1` decode≠prefill-only |
| B5.2 | Métricas TSC budget | Contadores slog `ok`; teste host budget |
| B5.3 | W2A8 CPU path | ADR-0105 B0–B3 já; golden GPU **Track C** |
| B5.4 | Falcon3 tok/s | Lab WHPX documentado; metal T+800 **AWAITING** (não bloqueia se defer) |

### B6. Rede / TLS / WiFi
| ID | Feature | Testes |
|----|---------|--------|
| B6.1 | TLS smoke path + `tls_not_ready` honesty | Host/QEMU; nunca strip https→80 |
| B6.2 | BEI spam throttle | Contagem linhas serial A &lt; limiar; teste slog rate |
| B6.3 | WiFi RF | **Track C** AWAITING_HW (sem SSID fabricado) |

### B7. UI / Jarbas
| ID | Feature | Testes |
|----|---------|--------|
| B7.1 | Desktop S1–S4 | Já ✅; regressão QEMU 4c UI |
| B7.2 | S5: **um** widget + tema mínimo (Onda 9 T-070) | Host card render + QEMU card demo |
| B7.3 | Mouse PS/2 soft-F4 | Já s360; USB HID **Track C** |
| B7.4 | soul_* / emotion | Host 45+ testes jarbas verdes |

### B8. Segurança (honesty 1.x, promote = C)
| ID | Feature | Testes |
|----|---------|--------|
| B8.1 | Demos Ring3 QEMU + `/ring3 status` | Lab 4c `ring3_can_iretq=true` |
| B8.2 | CapGate wasmi + DENY PoC sev=`ok` | Host CapGate + hermes wasmi self-test |
| B8.3 | `register_native` / B/C | **GATED** — só metal HITL Track C |
| B8.4 | CapGate DMA/FB pleno | **Track C** escada ADR-0041 |

### B9. SMP / scheduler (código ON, aceite = A1)
| ID | Feature | Testes |
|----|---------|--------|
| B9.1 | `smp-runqueue` feature ON | Host steal/queue testes ADR-0089 |
| B9.2 | `ap_pollable` pós-IDT | Aceite metal Onda 2; QEMU 4c smoke |
| B9.3 | Hybrid P/E | Telemetria; não bloqueia gate |

### B10. Meta / MHI
| ID | Feature | Testes |
|----|---------|--------|
| B10.1 | AutoLearn efficacy com FAT knowledge | Lab + `SECURITY.BIN` → `status=ok` |
| B10.2 | SleepCycle 5 fases METRIC | Já 3–4 fases; completar PRUNE/REFLECT no parser |
| B10.3 | MHI DRAM↔HDD copy | Já parcial; NVMe/VRAM DMA **Track C** |

---

## 3. Trilho C — Pós-gate / Layer S (fora do freeze 1.x)

Explicitamente **não** bloqueiam tag gate (ADR-0100 §5), listados para não “sumirem”:

1. Ring3 T-052/053 metal + App Factory B/C + Cranelift wire  
2. CapGate IOMMU / MAP_FB / PIN_DMA reais  
3. W2A8 GPU KernelPack + AMD compute + NPU driver  
4. Prefill AirLLM DMA e2e (Onda 10)  
5. WiFi iwlwifi/ath10k RF  
6. TLS produção soft-float pleno  
7. Mesh cluster LAN real multi-host  
8. UAC isócrono + Piper VITS pleno  
9. Jarbas Tier 3–4  
10. schemes CPL=3 (0103 Fase 2)

Cada item: STATE defer + IDEA_BANK destino + aceite AWAITING_HW.

---

## 4. Cronograma sugerido (8–10 semanas calendar, metal-dependente)

```
Semana 0–1   B1 CI verde + A0 honesty + sync TODO
Semana 1–2   B3.1–B3.2 voz mínima + B2 mesh peer B lab
Semana 2–3   B4 storage orphans + Tickv honesty + B6 BEI/TLS
Semana 3–4   A2 metal boot #1 (BOOT.LOG + freeze bisector)
Semana 4–5   A1 Onda 2 SMP K23 (1–2 notebooks)
Semana 5–6   A3 Onda 3 OTA A2 lab + evidência
Semana 6–7   B5 InferQueue lab decode + B7 S5 um widget
Semana 7–8   B10 learn FAT + mesh relaunch operador
Semana 8–9   A4 review ADR + freeze tag v1.9.99-gate
Semana 9+    Declaração v2.0.0 (OK humano) OU mais Track B
Track C      paralelo pós-tag, nunca no caminho crítico do gate
```

**Risco #1:** operador metal atrasa → Trilho A escorrega; código Track B continua.

---

## 5. Matriz de testes (obrigatória no freeze)

### 5.1 Host (a cada PR freeze)
```powershell
python tools/gen_test_gguf.py
$env:CARGO_TARGET_DIR="target/check-host"
cargo test --workspace --exclude neural-kernel --exclude boot --no-fail-fast
python tools/check_duplication.py
```
**Aceite:** 0 failed.

### 5.2 QEMU labs (nightly / pré-tag)
| Lab | Script | Critério |
|-----|--------|----------|
| Boot 4c UI | `run-qemu-whpx.ps1 -Smp 4 -RamGB 4` | PHASE 7 + desktop_ready |
| Mesh3 learn/sleep | `run-mesh3-lab.ps1 -Seconds 100` | VERDICT METRIC + `nodes≥1` (ideal ≥2) |
| Mesh6 | launcher 6-node | 3 roles sem hang B |
| LLM response | `lab-llm-response.ps1` | decode/done ≠ prefill-only |
| Ring3 demos | 4c | `ring3_can_iretq=true` |
| OTA A2 | `serve_update.py` + guest | T-022 evidence |
| Voz pacing | QEMU HDA | `PLAY_SAMPLES_DROPPED==0` |

### 5.3 Metal (bloqueio A)
Checklist: `docs/memory/HW_DIAG_s316.md` + BOOT.LOG + K23 + Infer/UI.

### 5.4 Novos testes a escrever (mínimo)
| Módulo | Testes novos |
|--------|--------------|
| `jarbas` audio mixer | pacing clock / drop counter |
| `hermes` voice duplex | barge-in não cancela infer |
| `hermes` wasm_build | já verdes; lock se flaky |
| `k_nano` fat32 | FSInfo/pins regressão |
| OTA FSM | tries/last_good host |
| `measure_*` parsers | CI on sample logs |

---

## 6. Definition of Done — “1.x fechada”

- [ ] Trilho A completo (Onda 2+3 + review + OK) **ou** defer metal documentado + maintainer waiver escrito  
- [ ] `cargo test --workspace --exclude neural-kernel --exclude boot --no-fail-fast` = 0 fail  
- [ ] Mesh3 VERDICT METRIC + mesh6 sem hang B (lab)  
- [ ] Metal: `E:\BOOT.LOG` real **ou** AWAITING_OPERATOR explícito no gate review  
- [ ] Voz: V1+V2 merged; V3–V4 artefatos ou defer IDEA  
- [ ] Track C listado em STATE com defer (não “esquecido”)  
- [ ] Tag git + CHANGELOG “1.x freeze / pré-v2”  
- [ ] **Só então** conversa `v2.0.0` (não automática)

---

## 7. Anti-padrões (proibido neste plano)

- Declarar v2.0.0 sem OK humano  
- Ligar App Factory B/C ou `register_native` em QEMU para “fechar” Ring3  
- Fabricar SSID WiFi / claim wake 98% sem artefato  
- Engordar `neural-kernel` com lógica nova (emagreçer)  
- Fechar gap com sev `fail` em sucesso ou `warn` em deny esperado  
- Tratar AWAITING_HW como fail de código sem defer STATE  

---

## 8. Próximo passo operacional imediato

1. ~~**B1.2** — workspace host 0 fail~~ ✅ s368  
2. ~~**A0.1** — checklist gate Onda 0–3 em STATE~~ ✅ s368  
3. Agendar **A2.1–A2.2** boot Alienware (operador).  
4. ~~**B3.1** playback pacing~~ ✅ código s368; lab QEMU drops=0 residual  
5. **B3.2** duplex / STT_UNCERTAIN + **A3** OTA lab A2  

---

*Fonte de verdade viva:* `TODO.md` + `docs/architecture/0100-k3chj-backlog-custo-anel.md`. Este plano é o mapa de execução; status por SESSION/STATE.
