# HW flash — usb_hw.img (SESSION_314 / ADR-0103 S1)

**Pista:** P0 BOOT.LOG metal — não perder desktop; melhorar persistência MSC (hub+route+TT).

## Artefato (gerar antes do flash)

```powershell
cargo build --release -p boot
$env:PACK_LLM = "falcon3"
python tools/build_image.py --hw --unified --size 6144 --build-boot
```

- Saída: `target/usb_hw.img` (**6271 MB** — SESSION_315b anti-black-screen)
- ESP `kernel.elf` sha256 prefix `8dbde4ea822aca58` (budget MSC 3s + boot_progress_line)
- Pack completo: FALCON3.V6 (~788MB) + AGENT/LEARNER/RERANKER/RUSTCDR3/VISION + Piper + BGE + E5 + ROUTER + HW Expert + firmware GS/AT10K/i915/NVIDIA + LEGO recipes + BOOT.LOG placeholder + NSGDB 8MB
- Kernel path: `k_hal::usb::probe_and_install` + hub budget + soft-halt UI + FB `BOOT: early USB…`

**Flash (operador):** Rufus → Imagem DD em `target\usb_hw.img` — agente não grava o stick.

## Rufus (Windows)

1. Pendrive ≥8 GB  
2. Rufus → `target\usb_hw.img` → **Imagem DD** (não ISO)  
3. Gravar (apaga o stick)  
4. Notebook: Secure Boot OFF, boot UEFI USB  
5. Pós-boot: Windows monta **NEURAL-OS** (`E:` típico)

## Aceite P0 (copiar para REPORT)

| # | Critério | PASS/FAIL |
|---|----------|-----------|
| 1 | `E:\BOOT.LOG` BOM + `[S] neural-os-core` + checkpoints | |
| 2 | Desktop animado + mouse ≥ 2 min | |
| 3 | Sem freeze no 1º frame por probe MSC no tick | |
| 4 | (Ideal) `E:\NSGDB.BIN` não todo-zero | |

### Se MSC FAIL — FB (sem COM1) + stick

No ecrã procurar:

- `--- USB ramlog (MSC fail) ---` / `(DriverInit MSC fail)`
- `hub class @ root` / `hub port … pós-reset` / `MSC atrás do hub` / `EARLY bringup OK`
- `cmd CC/TIMEOUT`, speed/tt/route

Colar foto/texto em `logs/hw_alienware_s314/REPORT.md`.

## Freeze até PASS

Não abrir Ring3 metal, 0103 S2+, Falcon3 sprint, mesh 2c como prioridade.
