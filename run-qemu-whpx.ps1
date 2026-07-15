# neural-os-core - QEMU UEFI com WHPX (fallback TCG via -Tcg) — v1.6.0
# Fluxo: cargo build --release -> python tools\build_image.py -> .\run-qemu-whpx.ps1 [-Window]
param(
    [switch]$Window,
    [switch]$BuildDisk,
    [switch]$Tcg,
    [switch]$Bridge,
    [int]$RamGB = 6,
    [int]$Smp = 4
)

$ErrorActionPreference = "Stop"
$Root = $PSScriptRoot
$timestamp = Get-Date -Format "yyyyMMdd_HHmmss"
$logDir = Join-Path $Root "logs"
New-Item -ItemType Directory -Force -Path $logDir | Out-Null
$logfile = Join-Path $logDir "boot_whpx_$timestamp.txt"

$uefi = Join-Path $Root "target\uefi.img"
$ovmf = Join-Path $Root "target\ovmf.fd"
$qemu = "C:\Program Files\qemu\qemu-system-x86_64.exe"

if (!(Test-Path $uefi)) { Write-Host "ERRO: target\uefi.img ausente. cargo build --release"; exit 1 }
if (!(Test-Path $ovmf)) { Write-Host "ERRO: target\ovmf.fd ausente"; exit 1 }
if (!(Test-Path $qemu)) { Write-Host "ERRO: QEMU nao encontrado"; exit 1 }

function Resolve-FatDisk {
    $targetDisk = Join-Path $Root "target\disk_qemu.raw"
    $toolsDisk  = Join-Path $Root "tools\disk_qemu.raw"
    if (Test-Path $targetDisk) { return $targetDisk }
    if (Test-Path $toolsDisk) {
        New-Item -ItemType Directory -Force -Path (Join-Path $Root "target") | Out-Null
        Copy-Item -Force $toolsDisk $targetDisk
        Write-Host "Disco copiado: tools\ -> target\disk_qemu.raw" -ForegroundColor Yellow
        return $targetDisk
    }
    return $null
}

$disk = Resolve-FatDisk
if (-not $disk -and $BuildDisk) {
    python (Join-Path $Root "tools\build_image.py")
    $disk = Resolve-FatDisk
}

$accel = "tcg"
$cpu = "max"
if (-not $Tcg) {
    $accel = "whpx"
    # host + APX/MPX em QEMU 11 → OVMF #GP no PlatformPei; Haswell estável + AVX2
    $cpu = "Haswell"
}

$smpTry = @($Smp)
if ($Smp -ne 2) { $smpTry += 2 }

$netMode = "user"
$netArgs = @("-netdev", "user,id=n0,hostfwd=tcp::4445-:4445,hostfwd=tcp::4446-:4446", "-device", "e1000,netdev=n0")
if ($Bridge) {
    Write-Host "Tentando -netdev bridge,id=n0 ..." -ForegroundColor Cyan
    $netMode = "bridge"
    $netArgs = @("-netdev", "bridge,id=n0,br=bridge", "-device", "e1000,netdev=n0")
} else {
    Write-Host "WARN: netdev=user (bridge nao pedido / Windows host). hostfwd :4445/:4446" -ForegroundColor Yellow
}

Write-Host "=== NEURAL-OS-CORE (UEFI + $accel) ===" -ForegroundColor Cyan
Write-Host "RAM: ${RamGB}G | CPU: $cpu smp-try=$($smpTry -join ',') | NIC: e1000 ($netMode)"
if ($disk) { Write-Host "FAT32: $disk (IDE index=1)" -ForegroundColor Green }
else { Write-Host "AVISO: sem disk_qemu.raw - python tools\build_image.py" -ForegroundColor Yellow }
Write-Host "Log: $logfile"
Write-Host "Nota: se WHPX falhar (VP exit), rode: .\run-qemu-whpx.ps1 -Tcg -Window" -ForegroundColor Gray

function Build-QemuArgs {
    param([string]$Acc, [string]$Cpu, [int]$SmpN)
    $a = @(
        "-m", "${RamGB}G", "-smp", "$SmpN",
        "-accel", $Acc, "-cpu", $Cpu,
        "-drive", "format=raw,file=$uefi,if=ide,index=0"
    )
    if ($disk) { $a += @("-drive", "format=raw,file=$disk,if=ide,index=1") }
    # N3: BitNet 2B via QEMU loader @4GB (evita PIO FAT ~200MB no TCG)
    $bitnet2b = Join-Path $Root "target\bitnet_2B.bitnet"
    if (Test-Path $bitnet2b) {
        $a += @("-device", "loader,file=$bitnet2b,addr=0x100000000")
        Write-Host "BitNet2B loader: $bitnet2b @0x100000000" -ForegroundColor Green
    }
    $a += @(
        "-drive", "if=pflash,format=raw,file=$ovmf,readonly=on",
        "-serial", "file:$logfile",
        "-serial", "tcp:127.0.0.1:4444,server=on,wait=off"
    )
    $a += $netArgs
    if ($Window) { $a += @("-vga", "std") }
    else { $a += @("-vga", "none", "-display", "none", "-nographic") }
    return $a
}

$ok = $false
foreach ($smpN in $smpTry) {
    $qemuArgs = Build-QemuArgs -Acc $accel -Cpu $cpu -SmpN $smpN
    Write-Host "QEMU: -smp $smpN accel=$accel net=$netMode" -ForegroundColor Gray
    try {
        & $qemu @qemuArgs
        $ok = $true
        break
    } catch {
        Write-Host "WARN: falhou smp=$smpN accel=$accel : $_" -ForegroundColor Yellow
        if ($accel -eq "whpx") {
            Write-Host "WHPX falhou - tentando TCG smp=$smpN ..." -ForegroundColor Yellow
            $accel = "tcg"
            $cpu = "max"
            try {
                $qemuArgs = Build-QemuArgs -Acc $accel -Cpu $cpu -SmpN $smpN
                & $qemu @qemuArgs
                $ok = $true
                break
            } catch {
                Write-Host "WARN: TCG tambem falhou: $_" -ForegroundColor Yellow
            }
        }
        if ($netMode -eq "bridge") {
            Write-Host "WARN: bridge -> user+hostfwd" -ForegroundColor Yellow
            $netMode = "user"
            $netArgs = @("-netdev", "user,id=n0,hostfwd=tcp::4445-:4445,hostfwd=tcp::4446-:4446", "-device", "e1000,netdev=n0")
        }
    }
}
if (-not $ok) { Write-Host "ERRO: QEMU nao iniciou"; exit 1 }
Write-Host ""
Write-Host "Serial log: $logfile"
