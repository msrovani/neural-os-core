# watch_corruption.ps1 - s437: harness de diagnostico do stall silencioso.
#
# O GOAL "1h sem crash/#PF" esta bloqueado por uma corrupcao ESPORADICA de
# ponteiros (valores 0x6/0x16/0x13c/0x42d/0x7ee00001) que faz o kernel dar
# #PF em lock/CAS/MMIO num endereco invalido; o handler de storm parqueia o
# BSP em loop{hlt} -> freeze total. Este script acelera o diagnostico:
#
#   1. lanca o QEMU 8c/8GB com -monitor tcp (nao suportado pelo runner padrao);
#   2. espera o log estabilizar (freeze) ou surgir "[EXC] #PF storm";
#   3. extrai o IP e o CR2 do fault (o park ja persiste no serial);
#   4. dumpa o RIP de cada vCPU via monitor (info registers por CPU);
#   5. simboliza tudo contra target/limine-esp-tree/kernel.elf (llvm-nm).
#
# Uso:
#   powershell -NoProfile -ExecutionPolicy Bypass -File tools\watch_corruption.ps1
#   ... -WaitSec 900 -MonitorPort 4447
#
# ASCII-only (PS5 le UTF-8 sem BOM como CP1252; em-dash quebra strings).
param(
    [int]$WaitSec = 900,
    [int]$MonitorPort = 4447,
    [int]$RamGB = 8,
    [int]$Smp = 8
)

$ErrorActionPreference = "Stop"
$Root = Split-Path -Parent $PSScriptRoot
$qemu = "C:\Program Files\qemu\qemu-system-x86_64.exe"
$logDir = Join-Path $Root "logs"
New-Item -ItemType Directory -Force -Path $logDir | Out-Null
$stamp = Get-Date -Format "yyyyMMdd_HHmmss"
$serialLog = Join-Path $logDir "watch_$stamp.txt"
$bootLog = Join-Path $logDir "watch_mon_$stamp.txt"

if (!(Test-Path $qemu)) { Write-Host "ERRO: QEMU nao encontrado" -ForegroundColor Red; exit 1 }
foreach ($f in @("uefi.img", "disk_qemu.raw", "ovmf_code.fd", "ovmf_vars.fd")) {
    if (!(Test-Path (Join-Path $Root "target\$f"))) {
        Write-Host "ERRO: target\$f ausente (rode cargo build -p boot + build_image)" -ForegroundColor Red; exit 1
    }
}

Get-Process qemu-system-x86_64 -ErrorAction SilentlyContinue | Stop-Process -Force
Start-Sleep -Seconds 2

$qa = @(
    "-m", "${RamGB}G", "-smp", "$Smp", "-accel", "whpx", "-cpu", "Haswell",
    "-drive", "format=raw,file=$Root\target\uefi.img,if=ide,index=0",
    "-drive", "format=raw,file=$Root\target\disk_qemu.raw,if=ide,index=1",
    "-device", "loader,file=$Root\target\FALCON3.BIN,addr=0x100000000",
    "-device", "loader,file=$Root\target\bpe_vocab.bin,addr=0x13DF00000",
    "-device", "loader,file=$Root\target\netmode.flag,addr=0x13E2E4F47",
    "-drive", "if=pflash,format=raw,file=$Root\target\ovmf_code.fd,readonly=on",
    "-drive", "if=pflash,format=raw,file=$Root\target\ovmf_vars.fd",
    "-serial", "file:$serialLog", "-serial", "null",
    "-netdev", "user,id=n0", "-device", "e1000,netdev=n0",
    "-audiodev", "none,id=snd0",
    "-device", "intel-hda,id=hda0",
    "-device", "hda-duplex,id=hda-codec,bus=hda0.0,cad=0,audiodev=snd0",
    "-device", "qemu-xhci,id=xhci", "-device", "usb-tablet,bus=xhci.0", "-device", "usb-kbd,bus=xhci.0",
    "-device", "virtio-gpu-pci,id=vgpu", "-vga", "std", "-display", "none",
    "-monitor", "tcp:127.0.0.1:$MonitorPort,server,nowait"
)
Start-Process -FilePath $qemu -ArgumentList $qa -WorkingDirectory $Root -WindowStyle Hidden -RedirectStandardError $bootLog
Write-Host "QEMU lancado (monitor tcp:$MonitorPort, serial $serialLog)" -ForegroundColor Green

function Query([string]$cmd) {
    $script:w.WriteLine($cmd)
    Start-Sleep -Milliseconds 250
    $sb = New-Object System.Text.StringBuilder
    $deadline = (Get-Date).AddMilliseconds(700)
    while ((Get-Date) -lt $deadline) {
        if ($script:s.DataAvailable) {
            $b = New-Object byte[] 8192; $n = $script:s.Read($b, 0, 8192)
            [void]$sb.Append([Text.Encoding]::ASCII.GetString($b, 0, $n))
        } else { Start-Sleep -Milliseconds 50 }
    }
    return $sb.ToString()
}

# --- 1. esperar freeze (log para de crescer) ou storm ---
$lastSize = -1; $stable = 0
for ($i = 0; $i -lt $WaitSec; $i++) {
    Start-Sleep -Seconds 1
    $q = Get-Process qemu-system-x86_64 -ErrorAction SilentlyContinue
    if (-not $q) { Write-Host "QEMU saiu" -ForegroundColor Yellow; break }
    $size = (Get-Item $serialLog).Length
    if ($size -eq $lastSize) { $stable++ } else { $stable = 0; $lastSize = $size }
    $hasStorm = (Select-String -Path $serialLog -Pattern "PF storm" -SimpleMatch -ErrorAction SilentlyContinue) -ne $null
    if ($hasStorm -or $stable -ge 30) { break }
}
Write-Host "Freeze detectado (i=$i stable=$stable storm=$hasStorm)" -ForegroundColor Yellow

# --- 2. extrair IP/CR2 do fault ---
Write-Host "`n=== FAULT (IP/CR2) ===" -ForegroundColor Cyan
Select-String -Path $serialLog -Pattern "PF_DBG\] ip=|#PF storm|#PF ip=" -ErrorAction SilentlyContinue |
    Select-Object -Last 6 | ForEach-Object { $_.Line }

# --- 3. dump RIP por vCPU ---
Write-Host "`n=== vCPU RIPs (monitor) ===" -ForegroundColor Cyan
$ips = @()
try {
    $c = New-Object System.Net.Sockets.TcpClient("127.0.0.1", $MonitorPort)
    $script:s = $c.GetStream()
    $script:w = New-Object System.IO.StreamWriter($script:s); $script:w.AutoFlush = $true
    Start-Sleep -Milliseconds 400
    for ($cpu = 0; $cpu -lt $Smp; $cpu++) {
        $null = Query "cpu $cpu"
        $out = Query "info registers"
        $rip = [regex]::Match($out, "RIP=([0-9a-fA-F]+)")
        if ($rip.Success) {
            $addr = $rip.Groups[1].Value.PadLeft(16, '0')
            Write-Host ("CPU {0} RIP=0x{1}" -f $cpu, $rip.Groups[1].Value)
            $ips += $addr
        }
    }
    $c.Close()
} catch { Write-Host "monitor err: $_" -ForegroundColor Red }

# --- 4. simbolizar contra kernel.elf ---
$elf = Join-Path $Root "target\limine-esp-tree\kernel.elf"
$nmTool = Get-ChildItem "$env:USERPROFILE\.rustup\toolchains" -Recurse -Filter "llvm-nm.exe" -ErrorAction SilentlyContinue | Select-Object -First 1
if ((Test-Path $elf) -and $nmTool) {
    Write-Host "`n=== Simbolos (llvm-nm) ===" -ForegroundColor Cyan
    $nmOut = Join-Path $logDir "nm_watch_$stamp.txt"
    & $nmTool.FullName -C --numeric-sort $elf 2>$null | Out-File -Encoding utf8 $nmOut
    $lines = Get-Content $nmOut | Where-Object { $_ -match '^[0-9a-f]{16} ' }
    foreach ($t in $ips) {
        $prev = $lines | Where-Object { ($_.Substring(0, 16)) -le $t } | Select-Object -Last 1
        Write-Host ("0x{0} -> {1}" -f $t, $prev)
    }
} else {
    Write-Host "kernel.elf/llvm-nm ausente; pule simbolizacao" -ForegroundColor Yellow
}

Write-Host "`nProximo passo (watchpoint do corruptor):" -ForegroundColor Green
Write-Host "  Identifique a ESTRUTURA-va do CR2 (offset conhecido) e reinicie com:"
Write-Host "  -S -gdb tcp::1234 ; no gdb: watch -l *(unsigned long*)(BASE+OFF); continue"
Write-Host "  O writer para no 1o store -- vira fix pontual."
Write-Host "`nSerial: $serialLog"
