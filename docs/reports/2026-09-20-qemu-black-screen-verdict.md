## QEMU diagnosis 2026-09-20 (tela preta metal)

### Resultado QEMU (WHPX, uefi.img)
1. logwriter stage-0 OK
2. chainload **FromDevicePath** `\EFI\neural\limine.efi` OK (fix apos FromBuffer sem file_path)
3. Limine 12.5.2 carrega `boot():/kernel.elf`
4. Kernel handoff + FB 1280x800 + PHASE 0+ OK
5. Serial: `logs/qemu-logwriter-boot4_clean.txt`

### Metal (pre-fix)
- Stick E:\BOOT.LOG ainda placeholder → kernel nunca selou/rebootou, OU MSC nunca escreveu
- Tela preta pos-Limine: hipotese dominante = Limine sem device path (FromBuffer) falhando `boot():` no firmware real
- Pos-fix: BOOTX64 com FromDevicePath + marcador `NEURAL\BOOT.LOG` no ESP a cada boot

### Aceite metal
1. Copiar `target\limine-esp-tree\EFI\BOOT\BOOTX64.EFI` → `D:\EFI\BOOT\BOOTX64.EFI` (ou regravar usb_hw.img)
2. Boot: se `D:\NEURAL\BOOT.LOG` aparecer com `stage-0 OK` → logwriter rodou
3. Se menu Limine aparecer e depois preto longo: USB 2.0 carregando kernel.elf (~20MB) — esperar >30s
4. Apos Runtime+reboot: E:\BOOT.LOG deve sair do placeholder (ramlog selado)
