# SESSION_248: Veredito de Arquitetura — HW Expert v4 NN não identifica hardware além da tabela (2026-08-04)

**Sprint:** Verificação HW Expert v4  
**Bloco:** Medição honesta NN vs pci.ids  
**Resultado:** Negativo para a reivindicação "260KB NN ≥ DB 40MB" — documentado e gateado off

---

## 1. Objetivo

Levar a identificação de hardware por rede neural (HW Expert v4) até o fim, medir contra o pci.ids em máquinas (QEMU sweep), e produzir um número honesto: o modelo de 260KB dentro do kernel identifica hardware tão bem quanto um banco de dados de 40MB?

## 2. Lanes executadas (12 lanes, ~7h compute)

| # | Lane | Objetivo | Resultado |
|---|---|---|---|
| E1-3 | explorers (3) | Recon codebase (skill, dataset, pci.ids, sweep infra) | Mapa completo |
| fix-1 | diagnosis H2 | Por que modelo shipped prediz family=16? | H2 CONFIRMADO: pesos zero (threshold 0.5 vs init ±0.088) |
| fix-2 | retrain + validate | Treinar modelo real, validar arquivo exportado | 73.32% holdout (circular — labels = heurística) |
| fix-3 | sweep QEMU | 5 boots, 60 cards, modelo @0x179000000 | **5% (3/60)** — modelo colapsa p/ realtek_eth; NN erra 0/12 na tabela |
| fix-4 | precedência | build_card: tabela-primeiro | Tabela curada SEMPRE vence o ML (commitado em 0e0f318) |
| fix-8 | class retrain | Treinar no dataset v2 (12 classes genéricas, ground-truth independente) | **60.67% ≈ majoritário 60.58%** — colapsa p/ 'other'; GATES FAIL |
| ora-1 | oracle verdict | Diagnóstico: capacidade vs treino vs alvo | Transformer é a ferramenta errada; alvo = família de driver, MLP como caminho alternativo |
| fix-9 | decisivo control | Mesmo arch fp32 (sem ternário), mesmo split | **60.58% = majoritário** — NÃO é quantização, é ARQUITETURA |
| fix-10 | vendor relabel | Dataset v3: famílias de driver dos NOMES do pci.ids | 21 famílias, canônicos 11/11, cobertura 54.7% (teto honesto) |
| fix-11 | MLP probe | MLP contínuo no dataset v3 | Overall 75.72% (inflado por 'other'), **específico 39.71%** |
| fix-12 | MLP variants | Inv-freq/sqrt/focal/concat/two-stage | **Best 58.97%**; stage-2 SEM imbalance: **63.27%** (abaixo do gate 65%) |

## 3. Veredito de arquitetura

### Controle decisivo (mesmo split 90/10 por device, seed 42, dataset v2)

| Variante | Holdout family (device-level) |
|---|---|
| Transformer ternário | 60.67% |
| **Transformer fp32 (mesmo arch)** | **60.58%** |
| Backbone ternário + head contínuo | 60.58% |
| Contínuo + vocab 256 | 60.60% |
| Linear (referência) | 63.3% |
| MLP 2-fc (referência) | 71.2% |

**Conclusão:** Remover a quantização muda ZERO — o transformer com atenção truncada (q_dim=32) + mean pool colapsa para majoritário mesmo em fp32. **A arquitetura é a vilã, não a quantização.** QAT/ADR-0084 não é o caminho de fix.

### MLP no dataset v3 — variantes

| Variante (202.5KB) | Specific acc | Overall | específico→'other' |
|---|---|---|---|
| plain CE | 39.71% | 75.72% | 700/1443 |
| inverse-freq | 58.97% | 42.10% | 86/1443 |
| sqrt-freq | 54.61% | 70.00% | 342/1443 |
| two-stage (stage-2 só específicos) | **63.27%** | — | — |

Stage-2 treinado SEM desbalanceamento (só ground-truth específico) placa em 63.27% — **abaixo do gate de 65% mesmo com stage-1 perfeito.** Teto = **SINAL**: `vid:did → família de driver específica` em devices nunca vistos ~59-63% com nomes pci.ids cobrindo 54.7%.

## 4. Número honesto final

| Caminho | Acurácia | Dispositivos |
|---|---|---|
| Tabela curada | **100%** | Conhecidos (~18 pares QEMU + expansível) |
| Heurística class byte | Confiável | Hardware fornece o byte (0x08-0x0B PCI config) |
| **NN (qualquer arch)** | **≤59-63% específico** | Devices nunca vistos |

**A reivindicação "260KB NN ≥ DB 40MB" é REFUTADA pela medição.** A DB nomeia o que conhece (100% cobertura); a NN não atinge o gate de 65% em família específica para devices fora da tabela. O teto é de SINAL nos dados, não de arquitetura ou treino.

## 5. Disposição final do kernel

- `build_card`: **tabela → heurística** (NN branch removido; `ea696c3`)
- `prediction_to_card`: preservado com `#[allow(dead_code)]` + protocolo de re-habilitação
- `predict_all_pci`: no-op (predições NN erradas não entram no SGDB)
- `cargo check --release`: 0 erros

**Re-habilitação:** restaurar branch em `build_card` + provar ≥65% específico no protocolo honesto (split 90/10 por device, seed 42, sweep QEMU). A infraestrutura para isso está no repo.

## 6. Infraestrutura entregue

| Ferramenta | Função |
|---|---|
| `tools/hw_sweep/` (run_sweep.ps1 + parse_sweep.py) | Sweep QEMU multi-device, parsing de [HW-PnP], scoring |
| `tools/validate_hw_expert_v4.py` / `_class.py` | Port Rust-exato: parse, nonzero gate, predições, holdout do ARQUIVO |
| `tools/retrain_hw_expert_v4.py` / `_class.py` | Treino Rust-exato (forward espelha kernel), STE, threshold no arquivo |
| `tools/probe_continuous_arch.py` | Controle decisivo: fp32 same-arch (diagnóstico quantização vs arquitetura) |
| `tools/probe_mlp_vendor*.py` | MLP probe + variantes no alvo vendor-specific |
| `tools/relabel_hw_expert_v4_class.py` / `_vendor.py` | Relabel com ground-truth independente (v2 genérico, v3 vendor) |
| `models/hw_expert/v4/dataset_class_v{2,3}.json` | Datasets relabelados (59.6K samples cada) |

## 7. Evidência commitada

| Commit | Descrição |
|---|---|
| `79ac8e5` | Benchmark holdout inicial |
| `f493fcd` | Relabel dataset v2 (12 classes) + tooling |
| `cbaf1a5` | ADR-0084 §11.5 + política fidelidade de treino |
| `3f9dc51` | Verdict arquitetura + dataset v3 vendor + controle |
| `5d4f67c` | Verdict FINAL + MLP probes |
| `ea696c3` | Gate off NN (build_card tabela+heurística) |

## 8. Lições

1. **O artefato exportado é o contrato, não o modelo em memória** — in-mem 73% ≠ runtime 5% (SESSION_247). Validar SEMPRE o arquivo com port Rust-exato.
2. **Forward de treino deve espelhar o kernel** — 5 divergências (rms_norm, SwiGLU, heads sem bias, atenção q_dim, residual) + embed .T produziram artefato que lia lixo no kernel.
3. **Controle decisivo (fp32 same-arch) é o experimento mais barato** — em 30 min descobre se o gargalo é quantização ou arquitetura; evita retrain de ciclos errados.
4. **Teto de sinal > arquitetura > treino** — o vid:did → família de driver tem teto 59-63% com os dados do repo (nomes pci.ids cobrem 54.7%); nenhuma arquitetura ou variante de loss muda isso significativamente.
5. **Labels independentes são necessários, mas insuficientes** — a heurística circular foi refutada (runtime 5%), mas o ground-truth honesto também é limitado pelo que os dados carregam (nomes pci.ids não contêm família de driver para 45% dos devices).
6. **Tabela curada + precedência tabela-primeiro** é o caminho correto para devices conhecidos; a NN só se justifica se medidamente melhor que a heurística de class byte.
7. **O protocolo de medição é o ativo durável** — o sweep QEMU, split honesto, validator Rust-exato e controle contínuo provam ou refutam qualquer modelo futuro com ~30-90 min de compute.
