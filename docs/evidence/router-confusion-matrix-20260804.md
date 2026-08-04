# Router Confusion Matrix — Trinity MoE (ADR-0083 P1)

- Date: 2026-08-04
- Expert index order: generator, hw_control, hw_identify, rust_coder, disk_diag, security, speech_synth
- CURATED utterances: 111 total; TRAIN templates: 1863
- Split: TEST 31 (stratified holdout, never seen in training), VAL 25, TRAIN 55 curated + templates (1910 samples)
- Training: 114 epochs run, best at epoch 54 (early stop patience 60); val acc 0.840, val CE 0.6655
- Overall accuracy (pure argmax, no threshold): **0.935** (29/31)

## Confusion matrix (true x pred)

| true \ pred | generator | hw_control | hw_identify | rust_coder | disk_diag | security | speech_synth | row |
|---|---|---|---|---|---|---|---|
| **generator** | 5 | 0 | 0 | 0 | 0 | 0 | 0 | 5 |
| **hw_control** | 0 | 4 | 0 | 0 | 1 | 0 | 0 | 5 |
| **hw_identify** | 0 | 0 | 4 | 0 | 0 | 0 | 0 | 4 |
| **rust_coder** | 0 | 0 | 0 | 4 | 0 | 0 | 0 | 4 |
| **disk_diag** | 0 | 0 | 1 | 0 | 4 | 0 | 0 | 5 |
| **security** | 0 | 0 | 0 | 0 | 0 | 4 | 0 | 4 |
| **speech_synth** | 0 | 0 | 0 | 0 | 0 | 0 | 4 | 4 |
| **col** | 5 | 4 | 5 | 4 | 5 | 4 | 4 | |

## Per-class metrics

| expert | precision | recall | F1 | support |
|---|---|---|---|---|
| generator | 1.000 | 1.000 | 1.000 | 5 |
| hw_control | 1.000 | 0.800 | 0.889 | 5 |
| hw_identify | 0.800 | 1.000 | 0.889 | 4 |
| rust_coder | 1.000 | 1.000 | 1.000 | 4 |
| disk_diag | 0.800 | 0.800 | 0.800 | 5 |
| security | 1.000 | 1.000 | 1.000 | 4 |
| speech_synth | 1.000 | 1.000 | 1.000 | 4 |

Overall accuracy: **0.935**

## Mismatch highlights

- `set brightness and speak` — true **hw_control**, pred **disk_diag** (top-2: disk_diag=0.385, speech_synth=0.358)
- `identifique o cve do disco` — true **disk_diag**, pred **hw_identify** (top-2: hw_identify=0.611, security=0.266)
