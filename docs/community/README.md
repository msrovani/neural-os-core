# Neural Device LEGOs — Community Hub

Porta de entrada para contribuir com **conectores de hardware** do Neural OS (K³CHJ).

**Código:** https://github.com/msrovani/neural-os-core  
**Modelos/dados:** https://huggingface.co/aios-k2chj  
**Firmware upstream:** https://gitlab.com/kernel-firmware/linux-firmware.git  
**ADR:** [0056-neural-device-lego.md](../architecture/0056-neural-device-lego.md)  
**Specs:** [../specs/device-lego/](../specs/device-lego/)

## Mensagem

We don’t ship fake Wi‑Fi. Help us wake one real chip at a time — labels, firmware hashes, and serial logs welcome.

SDIO e o HW Expert **não acordam hardware**. Eles só sugerem *quem é* o chip. Acordar o silício exige DeviceRecipe + firmware com hash + BE medido.

## Documentos

| Doc | Para quem |
|-----|-----------|
| [CALL_FOR_CONTRIBUTORS.md](CALL_FOR_CONTRIBUTORS.md) | Convite + papéis + Adopt-a-Chip |
| [LICENSES.md](LICENSES.md) | Matriz de licenças |
| [DANGERS.md](DANGERS.md) | Perigos SDIO→modelo→LEGO |
| [MITIGATION.md](MITIGATION.md) | Empresas, RE, asm |
| [FAQ.md](FAQ.md) | Perguntas frequentes |
| [SECURITY.md](SECURITY.md) | Disclosure responsável |
| [GOOD_FIRST_ISSUES.md](GOOD_FIRST_ISSUES.md) | Tarefas pequenas |
| [../guides/sdio-extract-and-label.md](../guides/sdio-extract-and-label.md) | Extrair/rotular SDIO |
| [../guides/firmware-download-and-fat.md](../guides/firmware-download-and-fat.md) | FW → FAT |
| [../guides/huggingface-aios-k2chj.md](../guides/huggingface-aios-k2chj.md) | HF org |
| [../guides/train-hw-expert-v4.md](../guides/train-hw-expert-v4.md) | Treino |
| [../guides/wire-hw-expert-to-lego.md](../guides/wire-hw-expert-to-lego.md) | Card → recipe |
| [../guides/writing-a-device-recipe.md](../guides/writing-a-device-recipe.md) | Escrever RECIPE.md |

## Boundary

- `docs/community/` + `docs/specs/device-lego/` = **público estável**
- `docs/memory/SESSION_*` = lab interno (pode estar à frente ou atrás)

## Governança

Seguir [GOVERNANCE.md](../GOVERNANCE.md): IDEA → ADR → sprint → implementação.
