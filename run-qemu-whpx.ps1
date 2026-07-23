# neural-os-core - QEMU UEFI com WHPX (fallback TCG via -Tcg) - v1.8.0
# Fluxo: cargo build --release -> python tools\build_image.py -> .\run-qemu-whpx.ps1 [-Window]
#
# === Sprint Net gate (CANONICO) ===
# Path de internet = e1000 PCI (NAO SLIP/COM2).
#   Default: -netdev user + e1000 (QEMU slirp). Guest static 10.0.2.15/24.
#   Recomendado internet real via WiFi host: -Bridge (TAP + ICS/bridge Windows).
#     Guest: DHCP (nao forca 10.0.2.15). Requer adaptador TAP (OpenVPN TAP-Windows).
#
# === SLIP FREEZE (nao e path do gate Net) ===
# tools\serial_bridge.py + COM2 = peer de debug legado. Codigo permanece, default OFF.
#   Opt-in legado: -SerialBridge
#   Alias explicito skip: -NoSerialBridge (default ja e skip)
#
# === Audio host bridge ===
# Default: -audiodev none (HDA presente, sem mic/speakers do Windows).
#   Opt-in: -AudioBridge -> dsound duplex (in+out) no hda-duplex.
#   QEMU Win 11.x desta maquina: none/dsound/sdl/jack/spice/wav (sem wasapi).
#   Windows: permitir microfone para QEMU em Privacidade.
#
# VirtIO-net: -VirtioNet (alternativa ao e1000 no mesmo netdev)
# NOTA: ficheiro ASCII-only (PS5 le UTF-8 sem BOM como CP1252; em-dash partia strings).
param(
    [switch]$Window,
    [switch]$BuildDisk,
    [switch]$Tcg,
    [switch]$Bridge,
    [switch]$NoSerialBridge,   # default behavior; kept for scripts
    [switch]$SerialBridge,     # opt-in: start tools\serial_bridge.py (FROZEN for Net gate)
    [switch]$VirtioNet,
    [switch]$AudioBridge,      # opt-in: dsound duplex (mic + speakers) via intel-hda
    [string]$TapName = "",     # TAP adapter name for -Bridge (auto-detect if empty)
    [int]$SerialBridgePort = 4444,
    [int]$RamGB = 6,
    [int]$Smp = 4
)

$ErrorActionPreference = "Stop"
$Root = $PSScriptRoot
$timestamp = Get-Date -Format "yyyyMMdd_HHmmss"
$logDir = Join-Path $Root "logs"
New-Item -ItemType Directory -Force -Path $logDir | Out-Null
$logfile = Join-Path $logDir "boot_whpx_$timestamp.txt"
$bridgeLog = Join-Path $logDir "bridge_$timestamp.log"
$bridgeErr = Join-Path $logDir "bridge_$timestamp.err.log"

$uefi = Join-Path $Root "target\uefi.img"
$ovmf = Join-Path $Root "target\ovmf.fd"
$qemu = "C:\Program Files\qemu\qemu-system-x86_64.exe"
$bridgeScript = Join-Path $Root "tools\serial_bridge.py"
$netmodeFile = Join-Path $Root "target\netmode.flag"

if (!(Test-Path $uefi)) { Write-Host "ERRO: target\uefi.img ausente. cargo build --release"; exit 1 }
if (!(Test-Path $ovmf)) { Write-Host "ERRO: target\ovmf.fd ausente"; exit 1 }
if (!(Test-Path $qemu)) { Write-Host "ERRO: QEMU nao encontrado"; exit 1 }

$script:bridgeProc = $null

function Stop-SerialBridge {
    if ($null -eq $script:bridgeProc) { return }
    $bridgePid = $script:bridgeProc.Id
    try {
        if (-not $script:bridgeProc.HasExited) {
            Stop-Process -Id $bridgePid -Force -ErrorAction SilentlyContinue
            Get-CimInstance Win32_Process -Filter "ParentProcessId=$bridgePid" -ErrorAction SilentlyContinue |
                ForEach-Object { Stop-Process -Id $_.ProcessId -Force -ErrorAction SilentlyContinue }
            Write-Host "[SLIP] killed pid=$bridgePid" -ForegroundColor Yellow
        } else {
            Write-Host "[SLIP] ja encerrado pid=$bridgePid" -ForegroundColor Gray
        }
    } catch {
        Write-Host "[SLIP] kill falhou pid=$bridgePid : $_" -ForegroundColor Yellow
    }
    $script:bridgeProc = $null
}

function Test-PortListening {
    param([int]$Port)
    try {
        $c = @(Get-NetTCPConnection -LocalPort $Port -State Listen -ErrorAction SilentlyContinue)
        if ($c.Count -gt 0) { return $true }
    } catch { }
    $out = netstat -an | Select-String -Pattern ":$Port\s+.*LISTEN"
    return $null -ne $out
}

function Start-SerialBridge {
    param([int]$Port)
    if (!(Test-Path $bridgeScript)) {
        Write-Host "[SLIP] ERRO: $bridgeScript ausente" -ForegroundColor Red
        return $false
    }
    if (Test-PortListening -Port $Port) {
        Write-Host "[SLIP] porta $Port ja em LISTEN - reutilizando" -ForegroundColor Yellow
        return $true
    }
    $py = Get-Command python -ErrorAction SilentlyContinue
    if (-not $py) {
        Write-Host "[SLIP] ERRO: python nao encontrado no PATH" -ForegroundColor Red
        return $false
    }
    $script:bridgeProc = Start-Process -FilePath $py.Source `
        -ArgumentList @("`"$bridgeScript`"", "--port", "$Port", "--watchdog", "0") `
        -WorkingDirectory $Root `
        -PassThru -WindowStyle Hidden `
        -RedirectStandardOutput $bridgeLog `
        -RedirectStandardError $bridgeErr
    $ok = $false
    for ($i = 0; $i -lt 34; $i++) {
        Start-Sleep -Milliseconds 150
        if ($script:bridgeProc.HasExited) { break }
        if (Test-PortListening -Port $Port) { $ok = $true; break }
    }
    if ($ok) {
        Write-Host "[SLIP] started pid=$($script:bridgeProc.Id) listen=127.0.0.1:$Port (FROZEN path; not Net gate)" -ForegroundColor Yellow
        return $true
    }
    Write-Host "[SLIP] FALHA ao escutar :$Port" -ForegroundColor Red
    if (Test-Path $bridgeErr) {
        Get-Content $bridgeErr -ErrorAction SilentlyContinue | Select-Object -Last 8 | ForEach-Object {
            Write-Host "  $_" -ForegroundColor DarkRed
        }
    }
    Stop-SerialBridge
    return $false
}

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

function Find-TapAdapter {
    param([string]$Preferred)
    if ($Preferred -and $Preferred.Length -gt 0) { return $Preferred }
    # Prefer OpenVPN TAP / tap-windows names
    try {
        $adapters = Get-NetAdapter -ErrorAction SilentlyContinue |
            Where-Object { $_.Status -ne "Not Present" -and (
                $_.InterfaceDescription -match "TAP|tap-windows|OpenVPN|Wintun" -or
                $_.Name -match "TAP|tap"
            ) }
        if ($adapters) {
            $a = $adapters | Select-Object -First 1
            return $a.Name
        }
    } catch { }
    return $null
}

function Write-NetModeFlag {
    param([string]$Mode) # 'U' = user/slirp, 'B' = bridge/TAP
    New-Item -ItemType Directory -Force -Path (Join-Path $Root "target") | Out-Null
    [System.IO.File]::WriteAllBytes($netmodeFile, [byte[]][char]$Mode)
}

try {
    $disk = Resolve-FatDisk
    if (-not $disk -and $BuildDisk) {
        python (Join-Path $Root "tools\build_image.py")
        $disk = Resolve-FatDisk
    }

    $accel = "tcg"
    $cpu = "max"
    if (-not $Tcg) {
        $accel = "whpx"
        # host + APX/MPX em QEMU 11 -> OVMF #GP no PlatformPei; Haswell estavel + AVX2
        $cpu = "Haswell"
    }

    $smpTry = @($Smp)
    if ($Smp -ne 2) { $smpTry += 2 }

    $netMode = "user"
    $nicDev = if ($VirtioNet) { "virtio-net-pci,netdev=n0" } else { "e1000,netdev=n0" }
    $netArgs = @("-netdev", "user,id=n0,hostfwd=tcp::4445-:4445,hostfwd=tcp::4446-:4446", "-device", $nicDev)
    Write-NetModeFlag -Mode "U"

    if ($Bridge) {
        Write-Host "=== NET: -Bridge (RECOMENDADO para internet real via WiFi host) ===" -ForegroundColor Cyan
        $tap = Find-TapAdapter -Preferred $TapName
        if (-not $tap) {
            Write-Host "ERRO: nenhum adaptador TAP encontrado." -ForegroundColor Red
            Write-Host "  1) Instale OpenVPN (TAP-Windows) ou tap-windows6" -ForegroundColor Yellow
            Write-Host "  2) Em 'Adaptadores de Rede', renomeie o TAP (ex: tap0)" -ForegroundColor Yellow
            Write-Host "  3) Opcao A: Bridge o TAP com o WiFi (botao direito > Bridge Connections)" -ForegroundColor Yellow
            Write-Host "     Opcao B: WiFi Properties > Sharing > ICS para o TAP" -ForegroundColor Yellow
            Write-Host "  4) Rode: .\run-qemu-whpx.ps1 -Bridge -TapName tap0 -Window" -ForegroundColor Yellow
            Write-Host "Fallback: sem -Bridge usa user/slirp (static 10.0.2.15)." -ForegroundColor Gray
            exit 1
        }
        $netMode = "bridge"
        Write-NetModeFlag -Mode "B"
        # Windows: tap backend (NOT linux -netdev bridge helper)
        $netArgs = @(
            "-netdev", "tap,id=n0,ifname=$tap,script=no,downscript=no",
            "-device", $nicDev
        )
        Write-Host "NET: TAP ifname='$tap' + $nicDev (guest DHCP; host WiFi via ICS/bridge)" -ForegroundColor Green
        Write-Host "Guest NAO usa 10.0.2.15 neste modo (isso e so slirp)." -ForegroundColor Gray
    } else {
        Write-Host "NET: user/slirp + $nicDev (guest static 10.0.2.15 -> host NAT)" -ForegroundColor Green
        Write-Host "Dica gate Net / WiFi real: .\run-qemu-whpx.ps1 -Bridge -Window" -ForegroundColor Cyan
    }

    # SLIP FREEZE: default OFF. Only -SerialBridge starts peer.
    $wantSlip = $SerialBridge -and (-not $NoSerialBridge)
    if ($wantSlip) {
        Write-Host "[SLIP] opt-in (-SerialBridge) - FROZEN; not Sprint Net gate" -ForegroundColor Yellow
        if (-not (Start-SerialBridge -Port $SerialBridgePort)) {
            Write-Host "[SLIP] abortando: peer serial falhou" -ForegroundColor Red
            exit 1
        }
    } else {
        Write-Host "[SLIP] skip (frozen default; use -SerialBridge to opt-in)" -ForegroundColor Gray
    }

    Write-Host "=== NEURAL-OS-CORE (UEFI + $accel) ===" -ForegroundColor Cyan
    Write-Host "RAM: ${RamGB}G | CPU: $cpu smp-try=$($smpTry -join ',') | NIC: e1000 ($netMode)"
    if ($disk) { Write-Host "FAT32: $disk (IDE index=1)" -ForegroundColor Green }
    else { Write-Host "AVISO: sem disk_qemu.raw - python tools\build_image.py" -ForegroundColor Yellow }
    Write-Host "Log: $logfile"
    Write-Host "Serial: COM1=file log | COM2=tcp client -> 127.0.0.1:$SerialBridgePort (SLIP peer opcional)" -ForegroundColor Gray
    Write-Host "Nota: se WHPX falhar (VP exit), rode: .\run-qemu-whpx.ps1 -Tcg -Window" -ForegroundColor Gray

    function Build-QemuArgs {
        param([string]$Acc, [string]$Cpu, [int]$SmpN)
        $a = @(
            "-m", "${RamGB}G", "-smp", "$SmpN",
            "-accel", $Acc, "-cpu", $Cpu,
            "-drive", "format=raw,file=$uefi,if=ide,index=0"
        )
        if ($disk) { $a += @("-drive", "format=raw,file=$disk,if=ide,index=1") }
        # ─── MoE Model Loaders — AUTO-SCAN da pasta target/ ───
        # Descobre TODOS os arquivos .bitnet / .BIN / .bin na pasta target/
        # e os carrega em endereços sequenciais (gap de 1MB entre modelos).
        # O usuário só precisa colocar o arquivo na pasta — o script descobre.
        # Prioridade: arquivos maiores primeiro (LLM base), depois os pequenos (experts).
        # Endereço base: 0x100000000 (4GB), sobe em passos de 0x100000 (1MB) + tamanho.
        $modelDir = Join-Path $Root "target"
        $modelFiles = @(Get-ChildItem -Path $modelDir -Filter "*.bitnet" -ErrorAction SilentlyContinue) +
                      @(Get-ChildItem -Path $modelDir -Filter "*.BIN" -ErrorAction SilentlyContinue) +
                      @(Get-ChildItem -Path $modelDir -Filter "*.bin" -ErrorAction SilentlyContinue)
        # Remove duplicatas (mesmo nome, extensões diferentes) e ordena por tamanho decrescente
        $seen = @{}
        $unique = @()
        foreach ($f in $modelFiles) {
            $base = [System.IO.Path]::GetFileNameWithoutExtension($f.Name).ToUpper()
            if (-not $seen.ContainsKey($base)) {
                $seen[$base] = $true
                $unique += $f
            }
        }
        $unique = $unique | Sort-Object -Property Length -Descending
        # Só filtra placeholders vazios (<10KB). Modelos grandes carregam todos;
        # kernel ajusta heap dinamicamente (fix: total_needed = file + estimated).
        $unique = @($unique | Where-Object { $_.Length -gt 10240 })
        $modelAddr = 0x100000000  # 4GB base
        $modelGap = 0x100000      # 1MB gap entre modelos
        $modelEndAddr = $modelAddr
        $loaded = 0
        foreach ($f in $unique) {
            $sizeMB = [math]::Round($f.Length / 1MB, 1)
            $hexAddr = [Convert]::ToString($modelAddr, 16).ToUpper()
            $a += @("-device", "loader,file=$($f.FullName),addr=$modelAddr")
            Write-Host "MoE loader: $($f.Name) ($sizeMB MB) @0x$hexAddr" -ForegroundColor Green
            $loaded++
            $thisEnd = $modelAddr + $f.Length
            if ($thisEnd -gt $modelEndAddr) { $modelEndAddr = $thisEnd }
            $modelAddr += [math]::Ceiling($f.Length / $modelGap) * $modelGap + $modelGap
        }
        if ($loaded -eq 0) {
            Write-Host "MoE: nenhum modelo .bitnet/.BIN encontrado em target/" -ForegroundColor Yellow
        } else {
            Write-Host "MoE: $loaded modelo(s) carregados via QEMU loader" -ForegroundColor Cyan
        }
        if (Test-Path $netmodeFile) {
            $netmodeAddr = $modelEndAddr + $modelGap
            $a += @("-device", "loader,file=$netmodeFile,addr=$netmodeAddr")
            $hexNetmode = [Convert]::ToString($netmodeAddr, 16).ToUpper()
            Write-Host "NetMode loader: $netmodeFile @0x$hexNetmode ($netMode)" -ForegroundColor Green
        }
        # COM1 = log file; COM2 = SLIP only if peer started (-SerialBridge).
        # Without peer, tcp client aborts QEMU - use null when SLIP frozen.
        $a += @(
            "-drive", "if=pflash,format=raw,file=$ovmf,readonly=on",
            "-serial", "file:$logfile"
        )
        if ($wantSlip) {
            $a += @("-serial", "tcp:127.0.0.1:${SerialBridgePort}")
        } else {
            $a += @("-serial", "null")
        }
        $a += $netArgs
        # HW simulado (QEMU device emulation docs):
        #   intel-hda + hda-duplex (ICH HDA; i440fx-safe — ich9 precisa q35)
        #   qemu-xhci + usb-tablet/kbd (input)
        #   virtio-gpu-pci (DeviceTree / k-hal VirtIO path; UEFI GOP continua em -vga)
        # Ref: https://qemu.readthedocs.io/en/latest/system/device-emulation.html
        # Audio: default none; -AudioBridge = dsound duplex (mic+speakers do Windows).
        # QEMU 11 Win build: wasapi indisponivel — usar dsound.
        if ($AudioBridge) {
            $audioDev = "dsound,id=snd0,out.mixing-engine=on,in.mixing-engine=on"
            Write-Host "AudioBridge: dsound duplex (mic+speakers) -> hda-duplex" -ForegroundColor Green
            Write-Host "  Windows: Privacidade > Microfone > permitir QEMU se captura falhar" -ForegroundColor Gray
        } else {
            $audioDev = "none,id=snd0"
            Write-Host "Audio: audiodev=none (use -AudioBridge para mic/speakers do host)" -ForegroundColor Gray
        }
        $a += @(
            "-audiodev", $audioDev,
            "-device", "intel-hda,id=hda0",
            "-device", "hda-duplex,id=hda-codec,bus=hda0.0,cad=0,audiodev=snd0",
            "-device", "qemu-xhci,id=xhci",
            "-device", "usb-tablet,bus=xhci.0",
            "-device", "usb-kbd,bus=xhci.0",
            "-device", "virtio-gpu-pci,id=vgpu"
        )
        if ($Window) {
            $a += @("-vga", "std", "-display", "gtk")
        } else {
            $a += @("-vga", "std", "-display", "none")
        }
        return $a
    }

    $ok = $false
    foreach ($smpN in $smpTry) {
        $qemuArgs = Build-QemuArgs -Acc $accel -Cpu $cpu -SmpN $smpN
        Write-Host "QEMU: -smp $smpN accel=$accel net=$netMode audio=$(if ($AudioBridge) {'dsound'} else {'none'}) serial-tcp=client:$SerialBridgePort" -ForegroundColor Gray
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
                Write-Host "WARN: bridge/TAP falhou -> fallback user/slirp + e1000" -ForegroundColor Yellow
                $netMode = "user"
                Write-NetModeFlag -Mode "U"
                $netArgs = @("-netdev", "user,id=n0,hostfwd=tcp::4445-:4445,hostfwd=tcp::4446-:4446", "-device", "e1000,netdev=n0")
            }
        }
    }
    if (-not $ok) { Write-Host "ERRO: QEMU nao iniciou"; exit 1 }
    Write-Host ""
    Write-Host "Serial log: $logfile"
}
finally {
    Stop-SerialBridge
}
