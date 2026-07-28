# SESSION 228: Hardware Boot Real + ESP GPT + Mouse PS/2 Fix

**Data:** 2026-07-28
**Context:** Pós-SESSION_227 — primeiro boot em HW real (notebook) via pendrive unificado.

---

## Sumário

1. **Boot HW real bem-sucedido** — Pendrive unified (GPT/ESP + dados FAT32) bootou Limine UEFI no notebook até a interface Jarbas. Primeira vez que o sistema roda em hardware real.
2. **mk_esp_fat.py: MBR → GPT** — O `uefi.img` (Limine boot) era MBR-only, mas o `build_usb_unified.py` exige GPT. Convertido para GPT completo (protective MBR 0xEE + EFI PART + entries + backup).
3. **Mouse Agent: 8042 probe** — Em notebook moderno sem controlador PS/2, `enable_ps2_mouse()` causava 100K-loop timeouts. Adicionado `ps2_check_exists()` com self-test curto.

---

## Problemas e Diagnósticos

### Pendrive não bootava
- **Causa**: `uefi.img` era MBR-only (Limine antes era bootloader 0.11 GPT).
- **Fix**: `mk_esp_fat.py` → GPT com protective MBR + GPT header + partition entries + backup.
- **Validação**: `usb_hw.img` gerado com `build_image.py --hw --unified`, pendrive via Rufus DD.

### Sistema lentíssimo + sem mouse
- **Causa raiz**: `MouseAgent::tick()` chamava `enable_ps2_mouse()` sem verificar existência do 8042. Em hardware sem PS/2:
  - `ps2_wait_write()`: 100K loops em port 0x64
  - `ps2_wait_read()`: 100K loops em port 0x64  
  - Múltiplas operações (~12) × 3-4 waits = milhões de iterações desperdiçadas
  - Resultado: sistema congela, cursor não aparece
- **Fix**: `ps2_check_exists()` — lê status 0x64 (rejeita 0xFF), self-test 0xAA→0x55 com timeout 5K loops (vs 100K). Se ausente, só USB HID.

### SMP no hardware real
- `allow_smp=true` para hypervisor=None (bare-metal)
- MADT scan → BOOT_APIC_IDS → fallback sequencial se MADT sem entries
- Trampoline < 1MB, retry 3×, timeout 250ms/AP
- Código já trata bare-metal moderno adequadamente

### BOOT.LOG não escrito
- `persist_now()` já tenta USB→ATA→AHCI→NVMe (linha 249 da boot_logger.rs)
- USB-MSC tenta primeiro, com sync_cache
- Possível causa: xHCI enumeração não completou a tempo, ou partição não montada

### Desafios abertos do bare-metal
- Trackpad I2C-HID (Synaptics/ELAN) — sem driver I2C no kernel
- Sem ATA PIO — USB boot não tem PATA controller
- xHCI depende de enumeração bem-sucedida
- GPU: framebuffer funcionou (Jarbas apareceu)

---

## Arquivos Modificados

| Arquivo | Mudança |
|---------|---------|
| `tools/limine/mk_esp_fat.py` | MBR-only → GPT (protective 0xEE + EFI PART + entries + backup) |
| `crates/neural-kernel/src/agents/mouse_agent.rs` | `ps2_check_exists()` before `enable_ps2_mouse()` |

## Lições Aprendidas

- **Sempre sondar hardware antes de init**: 8042 não existe em notebook moderno. PS/2 probe deve ser rápido e não presuntivo.
- **HW real ≠ QEMU**: 8042 garantido em QEMU (PIIX4/ICH9 emula), não em HW real. xHCI pode falhar sem driver I2C-HID para trackpad.
- **GPT obrigatório para UEFI real**: Limine bootloader exige GPT/ESP, não MBR.
- **Timeout curto para probe de hardware ausente**: 5K loops (vs 100K) para self-test 8042 evita travamentos.
