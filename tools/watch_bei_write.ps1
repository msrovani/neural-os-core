# watch_bei_write.ps1 - s452: captura o WRITER do stray heap write que corrompe
# o campo `affect_regulator` (Arc) do BeiState leakado (cr2=0x11 -> #PF storm).
#
# Metodo (licao s438): watchpoint de HW no CAMPO, nao no CR2 (o #PF e vitima).
#   - `incorporate_affect` faz `addq $0x38, %rdi` -> affect_regulator @ self+0x38.
#   - BEI_STATE (link addr 0xffffffff810f4790) guarda o *BeiState; bei_tick em
#     0xffffffff8015eb20. Break em bei_tick -> le BEI_STATE -> watch (BEI+0x38).
#   - O 1o store no campo DEPOIS do init = o corruptor; gdb para nele.
#
# ASCII-only (PS5 le UTF-8 sem BOM como CP1252).
param(
    [int]$RamGB = 6,
    [int]$Smp = 4,
    [int]$GdbPort = 1234,
    [int]$BootWaitSec = 60
)
$ErrorActionPreference = "Stop"
$Root = Split-Path -Parent $PSScriptRoot
$qemu = "C:\Program Files\qemu\qemu-system-x86_64.exe"
$gdb  = "C:/Users/msrov/AppData/Local/Microsoft/WinGet/Packages/BrechtSanders.WinLibs.POSIX.UCRT_Microsoft.Winget.Source_8wekyb3d8bbwe/mingw64/bin/gdb.exe"
$elf  = Join-Path $Root "target\limine-esp-tree\kernel.elf"
$logDir = Join-Path $Root "logs"
New-Item -ItemType Directory -Force -Path $logDir | Out-Null
$stamp = Get-Date -Format "yyyyMMdd_HHmmss"
$serialLog = Join-Path $logDir "bei_$stamp.txt"
$gdbLog = Join-Path $logDir "bei_gdb_$stamp.txt"
$gdbScript = Join-Path $Root "target\bei_watch.gdb"
$ovmfVarsRun = Join-Path $Root "target\ovmf_vars_bei.fd"
Copy-Item -Force "C:\Program Files\qemu\share\edk2-i386-vars.fd" $ovmfVarsRun

# Link-time addrs (kernel linkado em 0xffffffff80000000, fixo):
#   BEI_STATE = 0xffffffff810f4790 ; bei_tick = 0xffffffff8015eb20 ; offset = 0x38
$gdbBody = @"
set pagination off
set confirm off
target remote 127.0.0.1:$GdbPort
break *0xffffffff8015eb20
continue
set `$bei = *(unsigned long*)0xffffffff810f4790
printf "BEI_STATE=0x%lx\n", `$bei
watch *(unsigned long*)(`$bei+0x38)
continue
printf "=== WRITER HIT ===\n"
info registers rip rdi rsi rdx rax rcx
x/6i `$rip-10
bt
"@
$gdbBody | Out-File -Encoding ascii $gdbScript

Get-Process qemu-system-x86_64 -ErrorAction SilentlyContinue | Stop-Process -Force
Start-Sleep -Seconds 2
$qa = @(
    "-m","${RamGB}G","-smp","$Smp","-accel","whpx","-cpu","Haswell",
    "-drive","format=raw,file=$Root\target\uefi.img,if=ide,index=0",
    "-drive","format=raw,file=$Root\target\disk_qemu.raw,if=ide,index=1",
    "-device","loader,file=$Root\target\FALCON3.BIN,addr=0x100000000",
    "-device","loader,file=$Root\target\netmode.flag,addr=0x13E2E4F47",
    "-drive","if=pflash,format=raw,file=$Root\target\ovmf_code.fd,readonly=on",
    "-drive","if=pflash,format=raw,file=$ovmfVarsRun",
    "-serial","file:$serialLog","-serial","null",
    "-netdev","user,id=n0","-device","e1000,netdev=n0",
    "-audiodev","none,id=snd0","-device","intel-hda,id=hda0",
    "-device","hda-duplex,id=hda-codec,bus=hda0.0,cad=0,audiodev=snd0",
    "-device","qemu-xhci,id=xhci","-device","usb-tablet,bus=xhci.0","-device","usb-kbd,bus=xhci.0",
    "-device","virtio-gpu-pci,id=vgpu","-vga","std","-display","none",
    "-s"
)
Start-Process -FilePath $qemu -ArgumentList $qa -WorkingDirectory $Root -WindowStyle Hidden
Write-Host "QEMU lancado (gdb tcp:$GdbPort, serial $serialLog); aguardando $BootWaitSec s..."
Start-Sleep -Seconds $BootWaitSec
# gdb escreve warnings no stderr; sem "Continue" o pipeline morre no 1o aviso.
$ErrorActionPreference = "Continue"
& $gdb -batch -x $gdbScript $elf *>&1 | Out-File -FilePath $gdbLog -Encoding utf8
$ErrorActionPreference = "Stop"
Get-Process qemu-system-x86_64 -ErrorAction SilentlyContinue | Stop-Process -Force
Write-Host "`nserial=$serialLog"
Write-Host "gdb=$gdbLog"
