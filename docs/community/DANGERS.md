# Perigos — SDIO → HW Expert → Device LEGO

SDIO = inventário Windows (“existe este HWID”).  
LEGO rebelde = como o Neural OS **acorda** o silício.  
O valor é descoberta + roteamento + seeds — **não** substituir A3/BMI nem KernelPack.

## Tabela de perigos

| Perigo | Efeito | Mitigação | Pedido à comunidade |
|--------|--------|-----------|---------------------|
| Falso positivo de família | Ath9k vs ath10k → hang/#PF | Recipe assinada / BE in-tree | Corrigir labels `vid,did→family` |
| Inventar RegMap do .inf | MMIO errado | Cite Linux/datasheet | RegMap só com fonte URL |
| Ready/Connected sem RF | Usuário acha WiFi ok | Honesty; `VERDICT=` medido | Reportar falso Ready (`danger`) |
| NEEDS_FW → blob errado | Brick rádio/GPU | `blob_hash` + Deny mismatch | Tabela family↔FAT↔hash |
| Republicar .sys | Legal + HF ban | Só metadata | Checklist zero PE no JSON |
| GSP no git | Repo/licença | GSP só `target/` | Auditar PRs `firmware/` |
| Auto-bind unsigned | MMIO hostil | `verify_trusted` + HITL | Nunca aprovar sem signature |
| Deadlock UnlockDAG | Stages circulares | Tokens + timeout + Partial | Reproduzir edges |
| Combo WiFi+BT | Um Ready derruba o outro | Nós irmãos | Recipes irmãs |
| Data poisoning | Card empurra FW mau | Dual review WiFi/GPU | Review labels HF |
| `generate_register_map` guess | Mapa ≠ silício | Hint L3 only | PR “map verified” |
| QEMU ≠ Note | PASS falso | AWAITING_REAL_HW | Logs Note/dGPU |

## Texto canônico

> SDIO e o HW Expert **não acordam hardware**. Precisamos da comunidade para: (1) corrigir labels perigosos, (2) verificar RegMap/FW em HW real, (3) reportar falso Ready, (4) datasets limpos (sem `.sys`), (5) documentar edges USB↔FW↔WiFi↔GPU. Achou HWID mal classificado? Issue `danger` + serial.
