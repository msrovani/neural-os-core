# HW flash — usb_hw.img (gerado 2026-09-03, HEAD e3b0075)

## Artefato
- `target/usb_hw.img` — **6271 MB** (alias `disk_hw_unified.raw`)
- Layout: MBR dados `0x0C` + ESP `0xEF`; GPT ESP + NEURAL-OS FAT32
- Pack: `PACK_LLM=falcon3` → FALCON3.V6 (~770MB) + Piper + BGE + HW Expert + firmware NVIDIA/i915/ath10k + LEGO recipes + BOOT.LOG + NSGDB
- Kernel: `fat-boot-log` ON (via crate `boot`)

## Rufus (Windows)
1. Pendrive **≥8 GB**
2. Rufus → selecionar `target\usb_hw.img` → **Imagem DD** (não ISO)
3. Gravar (apaga o stick)
4. Notebook: **Secure Boot OFF**, boot **UEFI** no USB
5. Windows deve montar volume **NEURAL-OS** com `BOOT.LOG`

## Aceite metal (fechar AWAITING / caminhar v2.0)

| Item | Log / critério no metal |
|------|-------------------------|
| SMP K23 | `online==madt-1`, `smp_online=N`, roles ∝ N; sem freeze ICR |
| Ring3 H2 | `ring3_mark_hw_gate_passed` / `register_native_ring` só se metal PASS |
| xHCI HID | mouse/teclado USB real (QEMU EnableSlot falha) |
| HDA | playback/capture real (QEMU ICW stub) |
| GPU/fw | `fw_gpu` / canary CE se NVIDIA/Intel presente; senão honesty ABSENT |
| WiFi RF | ath10k/iwlwifi scan se HW + firmware no FAT |
| UAC | `[UAC-HW]` deixa de ser AWAITING_REAL_HW |
| BOOT.LOG | flush no stick (MSC/ATA); Notepad lê sem BOM lixo |
| LLM | FALCON3.V6 load → CURRENT_MODEL / TTS greeting |
| NSGDB | ingest/persist não-RAM |
| VFS+FS | pós-fix: `AtaAgent` não re-probe; NeuralFS skip ATA no live USB |

### Fix 2026-09-03/04 — hang `BOOT: self-tests...`
Causa: `boot_ckpt` → `try_flush_ramlog` → flush falha → `heal` re-probe xHCI/MSC = hang.
Fix: live USB sem MSC não flush/heal; `storage_available` respeita SKIP_*; FB `BOOT: K33 …` por passo; trainer skip no live USB.

### Run Alienware 2026-09-04 (Core 7 240H / 16c / RTX 3050)
- Operador: chegou **desktop UI** → **freeze**.
- Stick `E:\BOOT.LOG` = **placeholder**; `E:\NSGDB.BIN` = **zeros** (sem MSC → sem persist).
- Relatório: `logs/hw_alienware_s310/REPORT.md` (se existir) / histórico s310.
- **OK (imagem pré-pull s310 / ~`8573fa0`)**: boot metal até **desktop/UI** no Alienware 240H.

### Fix 2026-09-04 — BOOT.LOG/NSGDB no pendrive (urgente)
Causa: `bringup_boot_msc` pegava só a **1ª porta CCS** (webcam/BT) → stick sem MSC.
Fix: varrer todas as portas CCS (SS>HS); SCSI fail → skip porta; SysInfo promove FileFlash/NSGDB após FAT_READY.
Bugbot follow-up: **Disable Slot** em falha; migrate RAM→File antes remount; rate-limit MSC probe (~200 ticks). Commit `22acf7a`.

### Run Alienware 2026-09-05 (pós-`22acf7a`, imagem 23:49)
- Operador: de novo **desktop UI** → **freeze**.
- Stick `E:\BOOT.LOG` = **ainda só placeholder** (167 B); `E:\NSGDB.BIN` = **ainda 8 MiB zeros**.
- Relatório: `logs/hw_alienware_s311/REPORT.md`.
- **Veredito:** UI metal = PASS parcial; **persist MSC = FAIL** — multi-porta não bastou.

### Fix 2026-09-05 — raiz persist metal (SESSION_311)
**Raciocínio:** Desktop não precisa de MSC (Limine já leu ESP). BOOT.LOG/NSGDB precisam de `USB_MSC` vivo. Multi-porta só varre o **HC já bound**.

Causas (código):
1. `init_xhci` pegava o **1º PCI 0x0C/0x03** sem `prog_if==0x30` → pode ser **EHCI**; stick no 2º xHCI invisível.
2. Bulk MSC **EP1 hardcoded** + doorbell DCI 2/3 → SCSI falha se o stick usa EP2/EP3 (SESSION_170 residual).
3. `live_usb_no_msc` bloqueia ATA/AHCI (anti-hang correto) → sem MSC = zero persist.

Fix em `k_nano`:
- Filtrar/`fallback` xHCI `prog_if=0x30`; **`init_xhci_select(i)`** + probe MSC em **todos** os HCs.
- Parse Configuration Descriptor (class 08 BOT) → Configure EP + `BulkEndpoint.dci` real.
- `overwrite_boot_log` prioriza partição dados `0x0C` antes de ESP `0xEF`.

**Reteste:** nova `usb_hw.img` → Alienware → `E:\BOOT.LOG` deve ter `[T+]`/`Knn` (não placeholder).

### Run Alienware 2026-09-05 ~00:48 (pós-`494b965`)
- Operador: **desktop UI** → **freeze** (de novo).
- Stick: `BOOT.LOG` **ainda placeholder**; `NSGDB` **ainda zeros**.
- `D:\kernel.elf` 21139656 B (imagem nova confirmada no stick).
- Relatório: `logs/hw_alienware_s312/REPORT.md`.
- **Veredito:** multi-xHCI+EP parse **não bastou**; a auditoria seguinte encontrou
  violações determinísticas no Event Ring e no Normal TRB (SESSION_313).

### Fix 2026-09-05 — desktop trava, timer 00:00 (SESSION_312)
Dois bugs no mesmo sintoma:

1. **Scheduler:** SysInfo (`urgency` 160, `PollEvery` last_poll=0) rodava **no tick 1**, no mesmo ciclo do 1º frame. `UsbMassStorage::probe` fazia `init_xhci_select` em **todos** os HCs (reset) + `clear_msc_port_skips` + EnableSlot em webcam/BT → tick não volta → orb congelado.
2. **Clock:** `present_frame` dirty-rect pintava HUD/orb mas **não o dock**. O `{:02}:{:02}` era HH:MM; mesmo com timer vivo ficava `00:00` no 1º minuto.

Fix inicial: `mark_ui_live()` após o 1º render; SysInfo skip `tick<64`; dock
`swap_rect` + relógio `mm:ss` (hz clamp 8..=256).

### Fix 2026-09-05 — xHCI metal + liveness (SESSION_313)

Auditoria contra xHCI 1.2 encontrou as causas de raiz:

1. Interrupter Set 0 começa em **`RTSOFF+0x20`**, mas ERST/ERDP eram escritos
   em `RTSOFF` reservado; comandos não produziam eventos válidos no metal.
2. Normal TRB colocava **IOC no DWORD 2**, corrompendo length em +32; IOC é
   DWORD 3 bit 5.
3. Completion `Success=1` era rejeitado; o código aceitava `0`.
4. Faltavam Bus Master, UEFI ownership handoff, CSZ 64B, scratchpads, WPR
   SuperSpeed, PAGESIZE/CNR e ERDP.EHB.
5. Probe MSC pós-desktop foi bloqueado integralmente (não apenas rebind):
   enumeração síncrona não pertence ao scheduler cooperativo.
6. `smp-runqueue` de ticks fica gated até lock por-agent; AP compute continua.
   Orb/cursor usam tick do scheduler, não dependem do LAPIC/PIT.

**Reteste s313:** aguardar 2 minutos no desktop; depois verificar
`E:\BOOT.LOG` não-placeholder e `E:\NSGDB.BIN` não-zero.

**Não fecha só com stick:** WiFi RF, GPU canary, HDA, UAC, Ring3 H2, SMP K23, TLS, gate v2.0.0.

## Não usar
- Esta imagem como disco único no QEMU (continuar `uefi.img` + `disk_qemu.raw`)
- Rufus modo ISO / “escrever em partições”
