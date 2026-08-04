# HW Expert v4 — Honest Held-Out Benchmark

_Gerado por `tools/eval_hw_expert_v4.py` em 2026-08-03 01:45:41_

## Método

- **Split por dispositivo**: 90% dos dispositivos únicos (vid,did) para treino, 10% hold-out (seed 42). Nenhum dispositivo aparece nos dois lados.
- **Dataset**: 59905 amostras; 43594 dispositivos únicos — 39235 treino, 4359 hold-out. Hold-out: 5288 amostras.
- **Modelo**: BitNetLMv4 (ternário) — hidden=128, layers=6, heads=4 (q_dim=32), ff=256, batch=4096, lr=3e-4, weight_decay=1e-5, clip=1.0, AdamW.
- **Treino**: 2 epochs em CPU (cpu). Mesma arquitetura/loop de `train_hw_expert_v4.py`.
- **Rótulos**: derivados de `classify_by_vendor()` na geração do dataset (meta.family), que espelha a heurística do kernel.
- **Eval**: amostras hold-out (peso por duplicata) E dispositivos hold-out (1 voto por dispositivo). Cabeçalho = nível dispositivo.

## Headline — Hold-out (dispositivos NUNCA vistos no treino)

| Head | Acurácia (amostras) | Acurácia (dispositivos) |
|------|--------------------|-------------------------|
| family | 75.19% | 70.61% |
| fw_id | 94.95% | 94.49% |
| agent_id | 75.28% | 70.73% |
| caps_bits | 65.66% | 59.37% |
| next_action | 75.47% | 70.86% |

> Nota: quick_eval do treino (primeiras 1024 amostras DO TREINO) era o número antigo (~95%). A acurácia honesta em dispositivos nunca vistos é o quadro acima.

## Baselines no mesmo hold-out

| Baseline | Cobertura/Acurácia | Nota |
|----------|-------------------|------|
| (a) kernel `table_lookup` (~18 pares exatos) | 0.02% (1/4359 devices) | Tabela cobre só dispositivos conhecidos; em hold-out aleatório ~0% |
| (b) heurística `classify_by_vendor` (geradora dos rótulos) | 99.66% | **CIRCULAR**: os rótulos do dataset saíram desta função. Alto acerto é tautológico. |
| (b2) `heuristic_card` do kernel (só vid/did) | 11.03% | Aproximação: kernel despacha por PCI class byte (0x02/0x0D/0x03/0x04/0x0C/0x01/0x06), que o dataset não tem. Portamos as regras de vendor/máscara. |
| (c) pci_ids.json lookup exato | 51.34% (2238/4359 devices) | DB nomeia IDs conhecidos; não infere nada para desconhecidos. |

### Circularidade (explícito)

Os rótulos `y.family` do dataset.json foram gerados por `classify_by_vendor()` (mesma lógica de vendor/máscara do kernel). Comparar a heurística contra o rótulo mede fidelidade de reprodução, NÃO capacidade de classificar hardware real. O baseline (b) está ~100% por construção — ele É o gerador dos rótulos. O baseline (b2) é a versão honesta da heurística do kernel (sem class byte) e está abaixo do NN em family.

## Métrica-chave de generalização: NN vs heurística que desiste

| Heurística genérica | Devices genéricos | NN atribui família específica | % | Conf. média | Acordo c/ kernel |
|---------------------|-------------------|------------------------------|---|-------------|------------------|
| label_generator(pci_bridge) | 1202 | 648 | 53.9% | 0.645 | 0.0% |
| kernel(unknown) | 0 | 0 | 0.0% | 0.000 | 0.0% |

Interpretação: o NN infere uma família específica a partir de vid:did sozinho onde a heurística rotulou como genérico/desconhecido. Isso é a vantagem única do NN — e é **inverificável contra estes rótulos** (o rótulo diz genérico; o hardware real é a verdade que não temos). O NN treinado aprendeu os padrões de vendor/máscara dos 90% de dispositivos conhecidos e os aplica aos desconhecidos.

## Comparação de tamanho

- Modelo `.bitnet` v5 (5 heads): **266,321 bytes** (260 KB)
- `pci_ids.json`: **2,568,045 bytes**
- `usb_ids.json`: **2,044,387 bytes**
- DB combinada: 4,612,432 bytes → modelo = 5.77% do tamanho
- Corpus bruto raw (pci.ids + usb.ids + SDIO DriverPacks + WDM HWIDs): ~40 MB — a DB nomeia o que já conhece, o modelo de 260 KB infere para o que nunca viu.

## Caveats

1. **Rótulos são heurística, não ground-truth de hardware real.** O NN aprende a reproduzir o `classify_by_vendor` dos dispositivos vistos e a generalizar o padrão para os não vistos. Acurácia vs hardware real pode divergir dos números acima.
2. **Sem PCI class byte no dataset**: o kernel real despacha por class byte; o NN e o baseline (b2) operam só com vid:did.
3. **Bus inferido por fonte**: o dataset atual não guarda bus no meta; samples USB foram inferidos por 'usb' no source. Ambiguidade em source=sdio pode reduzir o acerto da heurística circular.
4. **Dispositivos com rótulos mistos**: 35 dispositivos têm mais de um family label entre amostras (fontes diferentes).
5. **Caps = 10 bits exatos**: comparar o vetor completo, não por-bit.
6. **Config do benchmark**: 2 epochs. Se o tempo fosse um problema (>1h), reduzir layers para 4 ou subsample — o split honesto nunca muda.

## Recomendação

1. Publicar o número hold-out (dispositivo) como o número oficial — o NN generaliza para dispositivos não vistos onde a heurística desiste.
2. NÃO comparar NN vs heurística pelos rótulos (circular). Comparar por (i) cobertura da tabela ~0% fora dos pares conhecidos, (ii) % de inferência específica em genéricos, (iii) avaliação em HW real com class byte.
3. Gerar um dataset de verdade com class byte (PCI config space) para o baseline (b2) completo e para medir acurácia vs hardware real.
4. Manter a ordem do kernel (NN → tabela → heurística): o NN é o único que cobre dispositivos fora da tabela.
