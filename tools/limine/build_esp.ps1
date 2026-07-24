# ADR-0065 — monta tree ESP alinhado ao ClaudioOS image-builder.
# Layout:
#   /EFI/BOOT/BOOTX64.EFI
#   /EFI/BOOT/limine.conf + limine.cfg   (UEFI procura primeiro ao lado do EFI)
#   /limine.conf + limine.cfg
#   /boot/limine.conf + limine.cfg
#   /kernel.elf
param(
    [string]$KernelElf = "",
    [string]$OutImg = "",
    [string]$LimineVersion = "12.5.2"
)

$ErrorActionPreference = "Stop"
$Root = (Resolve-Path (Join-Path $PSScriptRoot "..\..")).Path
$LimineDir = Join-Path $Root "tools\limine"
$Vendor = Join-Path $LimineDir "vendor"
$EspDir = Join-Path $LimineDir "esp"
if (-not $OutImg) { $OutImg = Join-Path $Root "target\limine-esp.img" }
if (-not $KernelElf) {
    $KernelElf = Join-Path $Root "target\limine\x86_64-unknown-none\release\neural-kernel"
}

New-Item -ItemType Directory -Force -Path $Vendor | Out-Null
$efiBin = Join-Path $Vendor "BOOTX64.EFI"

function Ensure-LimineRelease {
    if (Test-Path $efiBin) {
        Write-Host "[limine] vendor OK: $efiBin" -ForegroundColor Green
        return
    }
    $candidates = @(
        "https://github.com/limine-bootloader/limine/releases/download/v$LimineVersion/limine-binary.zip",
        "https://github.com/limine-bootloader/limine/releases/latest/download/limine-binary.zip"
    )
    $zip = Join-Path $Vendor "limine-binary.zip"
    $ok = $false
    foreach ($url in $candidates) {
        Write-Host "[limine] trying $url ..." -ForegroundColor Cyan
        try {
            Invoke-WebRequest -Uri $url -OutFile $zip -UseBasicParsing
            $ok = $true
            break
        } catch {
            Write-Host "[limine] miss: $_" -ForegroundColor DarkYellow
        }
    }
    if (-not $ok) {
        Write-Host "[limine] download falhou — coloque BOOTX64.EFI em $Vendor" -ForegroundColor Yellow
        return
    }
    $extract = Join-Path $Vendor "extract"
    if (Test-Path $extract) { Remove-Item -Recurse -Force $extract }
    Expand-Archive -Path $zip -DestinationPath $extract -Force
    $found = Get-ChildItem -Path $extract -Recurse -Filter "BOOTX64.EFI" | Select-Object -First 1
    if ($found) {
        Copy-Item -Force $found.FullName $efiBin
        Write-Host "[limine] BOOTX64.EFI instalado" -ForegroundColor Green
    }
}

Ensure-LimineRelease

if (!(Test-Path $KernelElf)) {
    Write-Host "ERRO: kernel ausente: $KernelElf" -ForegroundColor Red
    exit 1
}

# Limpa ESP tree
if (Test-Path $EspDir) { Remove-Item -Recurse -Force $EspDir }
$efiDir = Join-Path $EspDir "EFI\BOOT"
$bootDir = Join-Path $EspDir "boot"
New-Item -ItemType Directory -Force -Path $efiDir | Out-Null
New-Item -ItemType Directory -Force -Path $bootDir | Out-Null

# Kernel na raiz (ClaudioOS: /kernel.elf)
Copy-Item -Force $KernelElf (Join-Path $EspDir "kernel.elf")

$conf = Join-Path $LimineDir "limine.conf"
$cfg = Join-Path $LimineDir "limine.cfg"

# Multi-path (ClaudioOS): raiz, /boot, /EFI/BOOT
foreach ($dir in @($EspDir, $bootDir, $efiDir)) {
    Copy-Item -Force $conf (Join-Path $dir "limine.conf")
    Copy-Item -Force $cfg (Join-Path $dir "limine.cfg")
}

if (Test-Path $efiBin) {
    Copy-Item -Force $efiBin (Join-Path $efiDir "BOOTX64.EFI")
}

Write-Host "[limine] ESP tree (ClaudioOS-style): $EspDir" -ForegroundColor Green
Get-ChildItem -Recurse $EspDir | ForEach-Object { $_.FullName.Replace($EspDir, "") }
Write-Host "OK"
