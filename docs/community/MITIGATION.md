# Mitigação — empresas, HW hacking, assembler

Não pedimos milagre às empresas nem reescrevemos o mundo em asm. Estreitamos a promessa e abrimos o canal certo.

## Empresas (IHV/OEM)

| Pedir | Não pedir |
|-------|-----------|
| Redistribuição FW (WHENCE) | Driver Windows fechado |
| Errata / chip id | Reescrita bare-metal Rust |
| Confirmar `blob_hash` | SLA comercial |
| Contato open (listas ath10k-like) | NDA que bloqueie recipe |

Outreach **depois** de spec + golden. Se ignorarem → Linux open + RE.

## HW hacking (canal principal)

Trace MMIO, diff vs Linux, serial `VERDICT=`, board quirks, USB/BT sniff → `RegMap` + stages com **cite**. Label: `help-wanted:re`. Só HW próprio / FW redistribuível.

## Assembler

Hot path / stub interno no `k_hal` quando necessário. Formato comunitário = **RECIPE.md + Rust BE**, não “LEGO em asm”.

## Ordem de ataque

1. Docs + honesty  
2. Um rebelde e2e (ath10k Note)  
3. RE só nessa família  
4. USB EP0 → BT  
5. GPU via degraus/Nouveau  
6. Empresas depois do golden  

## Não é mitigação

Esperar Qualcomm/NVIDIA “adotar” o OS; 171K recipes do SDIO; wasm+asm genérico; SoftMAC em FW-MAC.
