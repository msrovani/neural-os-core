# Security — Device LEGOs

## Reportar

Bugs que concedem MMIO/Cap indevido, bypass de assinatura, ou recipe que executa bring-up sem trust:

1. Abra issue com label `security` (ou email privado se o maintainer publicar canal).
2. Inclua: VID/DID, recipe id, Cap envolvida, serial sanitizado (sem secrets).
3. Não publique PoC de brick em massa antes de mitigação.

## Não é vuln

- `VERDICT=PARTIAL` honesto  
- QEMU sem rádio  
- FW ausente no FAT  

## Trust

Assinatura Ed25519 + `blob_hash`: ver [TRUST.md](../specs/device-lego/TRUST.md) e ADR-0052/0053.
