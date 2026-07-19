# Call for Contributors — Neural Device LEGOs

**EN (short):** We don’t ship fake Wi‑Fi. Help us wake one real chip at a time — labels, firmware hashes, and serial logs welcome. Repo: https://github.com/msrovani/neural-os-core · Data: https://huggingface.co/aios-k2chj

**pt-BR:** Não pedimos o driver Windows. Pedimos HWID limpo, labels tipados, firmware redistribuível com hash, e recipes honestas.

## O que é um LEGO HW

| Camada | Papel |
|--------|--------|
| L0 Bus | MMIO/IRQ/DMA — só mantenedores no bin |
| L1 Port | HalOffer (`Net`/`Wifi`/`Gpu`/…) — HW bem-comportado (VirtIO…) |
| L2 DeviceRecipe | Bind VID/DID + FW + stages UnlockDAG — comunidade |

Backend nativo R1 (ath10k BMI, NVIDIA ACR) **permanece no bin**. Community entrega **recipes + labels + hashes + serial**.

## Papéis

| Papel | Entrega | Aceite |
|-------|---------|--------|
| Extractor | JSON HWID (sem `.sys`) | PR + counts |
| Firmware curator | Blobs WHENCE-ok + `blob_hash` + short names | PR `firmware/` (sem GSP) |
| Dataset contributor | Labels/datasets HF ou PR | Sanitize + license |
| HF publisher | Upload org `aios-k2chj` (trusted) | Card + link GitHub |
| Labeler | `vid,did → family,fw,caps,next_action` | Schema v4 |
| Trainer | `.bitnet` + metrics | Review antes de Auto |
| Recipe author | `RECIPE.md` golden | Schema; unsigned = draft Escalate |
| HW tester | Serial `VERDICT=` | Issue com log |
| AI/Cursor helper | Skill + sem MMIO inventado | Checklist Pre-Flight |

## Adopt-a-Chip

Adote **uma** família e leve até serial medido:

| Chip | Status alvo | Contato |
|------|-------------|---------|
| QCA6174 (`168C:003E`) Note 1050 | ath10k A3 `fw_ready` | Abra issue `adopt:qca6174` |
| VirtIO-net | L1 gold behaved | `adopt:virtio-net` |
| NVIDIA gp108 (Pascal lab) | Degraus ACR→canário | `adopt:gp108` |
| xHCI host | U0–U2 tipados | `adopt:xhci` |

Nome do adotante → [Hall of Serial](#hall-of-serial).

## Hall of Serial

Crédito por logs honestos (`PASS` **ou** `PARTIAL` com reason):

| Contributor | Chip | Marker | Data |
|-------------|------|--------|------|
| *(seu nome)* | | | |

## Badges de honesty

Orgulho, não vergonha: `AWAITING_REAL_HW`, `fw_ready`, `Partial`, `VERDICT=PARTIAL reason=…`.

## Credit Linux / WHENCE

Toda recipe deve citar driver upstream (ex.: `ath10k`, nouveau) e licença WHENCE do blob. Somos **bare-metal consumers** do open firmware — não competimos com o Linux.

## Danger reports

HWID mal classificado ou recipe que mentiu PASS → issue com label `danger` + serial. Ver [DANGERS.md](DANGERS.md).

## Supporters (media)

Cobertura ≠ codeveloper. Crédito sob **Supporters** se publicarem (ex.: Tom's Hardware, Phoronix, Hackaday) — após docs + serial reproduzível. Não listar como Contributor de código.

## Links

- Specs: [../specs/device-lego/](../specs/device-lego/)
- Good first: [GOOD_FIRST_ISSUES.md](GOOD_FIRST_ISSUES.md)
- Licenças: [LICENSES.md](LICENSES.md)
- HOWTO: [../../HOWTO.md](../../HOWTO.md)
