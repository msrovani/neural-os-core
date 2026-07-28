# Licenças & Proteção — Neural OS K³CHJ

**Versão:** 2026-07-27

---

## Estratégia de Licenciamento

Neural OS adota o modelo **AGPL-3.0 + Commercial Exception**:
- **Código aberto:** AGPL-3.0 protege o trabalho da comunidade contra fechamento
- **Licença comercial:** Empresas que não querem abrir modificações pagam por uma licença proprietária
- **Patentes defensivas:** Publicação antecipada (prior art) + pedidos provisórios USPTO

Este modelo é usado por MongoDB (SSPL), Elastic (ELv2), MariaDB (BSL) e dezenas de outros projetos bem-sucedidos.

Para licenciamento comercial: `licensing@neural-os.io`

---

## Matriz de Licenças

| Camada | Licença típica | Pode | Proibido |
|--------|----------------|------|----------|
| **Código Neural** (`crates/`, `tools/`) | **AGPL-3.0** | Fork/PR sob AGPL | Relicenciar como MIT sem acordo |
| **DeviceRecipe / specs** | **AGPL-3.0** (mesmo repo) | PR de recipes | Claim `signed=true` falso |
| **Datasets HF** (HWID/ids/metadata) | **MIT** (cards HF) | Redistribuir JSON sanitizado | Republicar `.sys` / DriverPack |
| **Blobs firmware/ lab** | **Por arquivo via WHENCE** | Subset redistribuível | GSP no git; FW sem licença |
| **linux-firmware upstream** | Misto (WHENCE) | Clone GitLab | Assumir "tudo GPL" |
| **Modelo HW Expert no HF** | Pesos MIT/Apache; código AGPL | Download lab/FAT | Remover atribuição |
| **pci.ids / usb.ids** | Licenças upstream | Via dataset HF sanitizado | Ignore NOTICE |
| **Chaves Ed25519** | N/A | Só PK pública | Commit de secret |
| **Patentes** (futuras) | **Licenciáveis** | Uso sob AGPL + comercial | Uso por concorrentes sem licença |

---

## Inovações Protegidas

Veja [PATENTS.md](PATENTS.md) para o catálogo completo de invenções elegíveis a patente:

| # | Invenção | Status |
|---|----------|--------|
| 1 | BitNet Ternary LLM em kernel bare-metal | Prior art ✓ · Provisória pendente |
| 2 | Neural AutoInstaller (ADR-0079) | Prior art ✓ · Provisória pendente |
| 3 | HW Expert v3 + SDIO MoE | Prior art ✓ · Provisória pendente |
| 4 | Memory Hierarchy Index (MHI) | Prior art ✓ · Provisória pendente |
| 5 | K³CHJ Capability Rings (ADR-0041) | Prior art ✓ · Provisória pendente |
| 6 | Trinity MoE on-device (ADR-0060) | Prior art ✓ · Provisória pendente |
| 7 | Self-Healing Firmware Pipeline | Prior art ✓ |
| 8 | Generative Card Desktop (ADR-0058) | Prior art ✓ |

---

## Proteção Legal — Passos Seguintes

| Ação | Custo | Prazo | Responsável |
|------|-------|-------|-------------|
| Publicação arXiv (prior art formal) | $0 | 1 semana | Mantenedor |
| Pedido provisório USPTO (#1-6) | ~$70 cada | Este trimestre | Advogado de patentes |
| Registro de marca "Neural OS" + "K³CHJ" | ~R$300 (INPI) | 3 meses | Mantenedor |
| NDA para discussões comerciais | $0 | Template pronto | Mantenedor |
| PCT internacional (se tração comercial) | ~$5k | 12 meses após provisória | Advogado |

### Prior Art — O que já temos

Publicar código no GitHub **já constitui prior art** (data de commit). Para maior robustez legal:
1. **ArXiv** — publicação revisada por pares, data certa, amplamente reconhecida
2. **Registro de Software (INPI/Brasil)** — ~R$70, prova de autoria

---

## Regras Práticas

1. Toda PR de dataset/FW cita **fonte + licença** (WHENCE excerpt ou card HF)
2. Publish HF: campo `license:` obrigatório no model/dataset card
3. Recipe: `license:` recomendado no frontmatter
4. Metadata HWID ≠ driver Windows — não redistribuir binários proprietários
5. Agradecer: GitLab kernel-firmware, pci-ids.ucw.cz, linux-usb.org
6. Contribuidores assinam cessão de direitos (veja [CONTRIBUTING.md](../../CONTRIBUTING.md))
7. **Nunca** commit secrets, chaves privadas, ou dados de terceiros sem licença

---

## Third-party

Bootloader vendor: MIT/Apache. Dependências Rust: ver crates individuais. Lista expandida em `NOTICE` na raiz.

---

**Contato:** `licensing@neural-os.io` · `patents@neural-os.io`
