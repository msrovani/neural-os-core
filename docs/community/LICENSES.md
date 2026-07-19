# Licenças — Neural Device LEGOs

O código do repositório é **AGPL-3.0** ([LICENSE](../../LICENSE)). Firmware, datasets e recipes **não** herdam a mesma regra automaticamente.

## Matriz

| Camada | Licença típica | Pode | Proibido |
|--------|----------------|------|----------|
| Código Neural (`crates/`, `tools/`) | **AGPL-3.0** | Fork/PR sob AGPL | Relicenciar como MIT sem acordo |
| DeviceRecipe / specs no tree | **AGPL-3.0** (mesmo repo) | PR de recipes | Claim `signed=true` falso |
| Datasets HF (HWID/ids/metadata) | **MIT** (cards HF) | Redistribuir JSON sanitizado | Republicar `.sys` / DriverPack |
| Blobs `firmware/` lab | **Por arquivo via WHENCE** | Subset redistribuível | GSP no git; FW sem licença |
| linux-firmware upstream | Misto (WHENCE) | Clone GitLab | Assumir “tudo GPL” |
| Modelo HW Expert no HF | Declarar no model card (pesos MIT/Apache sugerido; código AGPL) | Download lab/FAT | Remover atribuição |
| pci.ids / usb.ids | Licenças upstream | Via dataset HF sanitizado | Ignore NOTICE |
| Chaves Ed25519 | N/A | Só PK pública | Commit de secret |

## Regras práticas

1. Toda PR de dataset/FW cita **fonte + licença** (WHENCE excerpt ou card HF).
2. Publish HF: campo `license:` obrigatório no card.
3. Recipe: `license:` recomendado no frontmatter.
4. Metadata HWID ≠ driver Windows.
5. Agradecer: GitLab kernel-firmware, pci-ids.ucw.cz, linux-usb.org.

## Third-party (resumo)

Bootloader vendor: MIT/Apache. Dependências Rust: ver crates individuais. Lista expandida: manter alinhada a `NOTICE` se presente na raiz.
