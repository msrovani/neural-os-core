# ADR addenda — NixAIOS disposition

Addenda curtas por cluster. Corpos históricos bare-metal **não** são reescritos.

| Cluster | ADRs | Disposicao NixAIOS |
|---------|------|-------------------|
| Premissa | 0088, 0113, 0106 | PORT doutrina; evidência = journald/ISO |
| Cortex / modelos | 0078, 0101, 0112, 0057 | PORT hub + ADAPT HotSwap/GPU |
| Hermes / WASM | 0059, 0052, 0041 P3 | PORT Factory A + CapGate |
| BEI | 0060 | PORT tick fino; não substitui LLM |
| Mesh | 0081 | PORT protocolo UDP std |
| Memory | 0091 | PORT neural-sgdb FileStorage |
| UI | 0058, 0090 | ADAPT Slint + Cage |
| Install | 0086 | SUPERSEDE_NIX (ISO + rebuild) |
| Ring3 | 0077, 0102 | SUPERSEDE_NIX (namespaces/seccomp) |
