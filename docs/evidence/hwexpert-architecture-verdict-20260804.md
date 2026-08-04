# HW Expert v4 — Veredito de Arquitetura (controle decisivo, 2026-08-04)

## Contexto

O modelo de 260KB (ternário, HW Expert v4, transformer 128h/6L com atenção truncada q_dim=32)
identifica hardware dentro do kernel a partir de (vid,did). Após corrigir labels circulares
(relabel com ground-truth independente, dataset v2 de 12 classes genéricas), o holdout family
ficou em 60.67% ≈ baseline majoritário 60.58% — o modelo previa a classe majoritária para tudo.
Sweep QEMU (runtime) com os labels antigos: 3/60 = 5%.

## Controle decisivo (tools/probe_continuous_arch.py — mesmo split 90/10 por device, seed 42, dataset_class_v2.json)

| Variante | Holdout family (device-level) |
|---|---|
| Ternário (retrain original, t=0.05) | 60.67% |
| **Contínuo mesmo-arch (fp32, sem quantização)** | **60.58%** |
| Backbone ternário + head contínuo (fp32) | 60.58% |
| Contínuo + vocab 256 (tokenizer 8 bits) | 60.60% |
| Linear one-hot (referência, mesmos tokens) | 63.3% |
| MLP contínuo embed+2×fc (referência, arquitetura diferente) | 71.2% |

- Loss cai (5.81→4.49) com acc presa em 60.58% mesmo em fp32 → o forward colapsa para majoritário.
- Remover a quantização muda NADA; head contínuo NÃO destrava; vocab 256 ganha 0.02pt.
- Os labels TÊM sinal (linear 63.3%, MLP 71.2%) — o transformer (atenção truncada q_dim=32 +
  mean pool) é a ferramenta errada para esta tarefa.
- **QAT/ADR-0084 NÃO é o caminho de fix para este modelo** (o gargalo não é a quantização).

## Relabel vendor-specific (tools/relabel_hw_expert_v4_vendor.py → dataset_class_v3.json)

21 famílias de driver (intel_eth, realtek_eth, broadcom_eth, virtio, intel_wifi, realtek_wifi,
atheros_wifi, broadcom_wifi, nvidia_gpu, amd_gpu, intel_gpu, audio_hda, usb_host, storage,
bridge, qemu_vga, chelsio_eth, mellanox_eth, marvell_eth, other) derivadas dos NOMES oficiais
do pci.ids/usb.ids + OVERRIDES + fallback WDM; fw/agent/caps/next re-derivados via
prediction_to_card. fw/agent/caps/next idênticos ao vocab v2 (verificado byte-a-byte).

- Canônicos QEMU: 11/11 corretos (8086:100e→intel_eth, 10ec:8139→realtek_eth, 1af4:1000→virtio,
  8086:2723→intel_wifi, 168c:003e→atheros_wifi, 1234:1111→qemu_vga, 8086:1237/7000/7113→bridge,
  8086:7010→storage, 1b36:000d→usb_host).
- Cobertura específica: 54.7% (12.479/22.806 devices com nome pci.ids); teto honesto — os 45%
  restantes são subsistemas de processador (Xeon 1.229), DAQ/teste/FPGA (318), serial (304),
  modem (267), GPU legado (225), net de vendor desconhecido (884) — sem família de driver
  no nome; roteiam para a heurística de class byte do kernel (que o hardware fornece).

## Conclusão

1. O transformer ternário NÃO identifica hardware — nem com quantização removida. O número
   honesto é 60.6% ≈ majoritário (inútil como identificador).
2. A tarefa é aprendível por um MLP contínuo pequeno (71.2% no mesmo holdout; na taxa de
   vendor-family com 54.7% de cobertura, o teto MLP é ~65-75% em devices NUNCA vistos).
3. Caminho honesto: **tabela packed exata para devices conhecidos (100%) + MLP contínuo pequeno
   (~130-260KB, f32, matmul plain) para devices nunca vistos**, fusão com o class byte quando
   presente. A reivindicação "260KB vs 40MB DB" só é defensável no enquadramento de cobertura:
   "onde a DB não tem entrada, o modelo ainda decide família de driver com ~65-75%".
4. Artefato 60.6% NÃO deve ser shipped (revertido — modelo 886EFA64 restaurado).

Evidência completa: tools/target/hwexp_continuous_control.md, tools/target/hw_expert_v4_vendor_relabel_report.md,
docs/evidence/hw_expert_v4_holdout.md, docs/evidence/hw-expert-v4-runtime-20260803.txt.
