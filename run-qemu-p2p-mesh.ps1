<#
.SYNOPSIS
  Lanca instancias QEMU com link socket ponto-a-ponto para teste P2P Mesh (ADR-0081).
  Instancia A (Cloverleaf) escuta em 127.0.0.1:12345; Instancia B (Hal9000) conecta.
  Logs: logs/boot_mesh_a.txt e logs/boot_mesh_b.txt.
  IPs estaticos: A=10.0.3.2, B=10.0.3.3 (via netmode flag).
.DESCRIPTION
  SESSION_233: multicast 230.0.0.1 NAO funciona no Windows. Usar socket listen/connect.
  OVMF pflash obrigatorio: uefi.img e UEFI-only.
  ASCII puro (PS 5.1).
.PARAMETER Cores
  -smp 1|2|4|8 (default 2).
.PARAMETER Accel
  whpx|tcg (default tcg). WHPX falha -> cai para TCG.
.PARAMETER Mem
  -Mem RAM em GB, ate 8 (default 4). Teto T-047 / SESSION_280: 2x6G estoura host; 4G.
.PARAMETER WithModels
  Liga -device loader (BITNET2B + HWEXPRT*). Default: ligado se nem -NoModels nem -NoDisk.
.PARAMETER NoModels
  Desliga loaders MoE (boot rapido, estilo -NoDisk para LLM).
.PARAMETER Instance
  A|B|Both (default Both). A=listen, B=connect.
.PARAMETER NoDisk
  Omite segundo drive FAT32 (boot rapido mesh).
#>

param(
    [ValidateSet(1,2,4,8)]
    [int]$Cores = 2,

    [ValidateSet("whpx","tcg")]
    [string]$Accel = "tcg",

    [ValidateRange(1,8)]
    [int]$Mem = 4,

    [switch]$WithModels,
    [switch]$NoModels,
    [switch]$NoDisk,

    [ValidateSet("A","B","Both")]
    [string]$Instance = "Both"
)

$Root = $PSScriptRoot
$imgDir = Join-Path $Root "target1"
if (-not (Test-Path (Join-Path $imgDir "uefi.img"))) { $imgDir = Join-Path $Root "target" }
$target = Join-Path $Root "target"
$logDir = Join-Path $Root "logs"

New-Item -ItemType Directory -Path $logDir -Force | Out-Null
New-Item -ItemType Directory -Path $target -Force | Out-Null

$uefi = Join-Path $imgDir "uefi.img"
$disk = Join-Path $imgDir "disk_qemu.raw"

if (-not (Test-Path $uefi)) {
    Write-Host "[ERRO] uefi.img nao encontrado em $imgDir" -ForegroundColor Red
    Write-Host "       Rode 'cargo build --release' primeiro" -ForegroundColor Yellow
    exit 1
}

if ($NoDisk) {
    Write-Host "[AVISO] -NoDisk: segundo drive omitido (boot rapido, sem FAT32)" -ForegroundColor Yellow
    $disk = ""
} elseif (-not (Test-Path $disk)) {
    Write-Host "[AVISO] disk_qemu.raw nao encontrado - segundo drive sem dados" -ForegroundColor Yellow
    $disk = ""
}

# Model policy: -NoModels wins; -WithModels forces on; else default ON unless -NoDisk
$useModels = $true
if ($NoModels) { $useModels = $false }
elseif ($WithModels) { $useModels = $true }
elseif ($NoDisk) { $useModels = $false }

$qemu = "C:\Program Files\qemu\qemu-system-x86_64.exe"
if (-not (Test-Path $qemu)) {
    $alt = Get-Command "qemu-system-x86_64" -ErrorAction SilentlyContinue
    if ($alt) { $qemu = $alt.Source }
    else {
        Write-Host "[ERRO] QEMU nao encontrado. Instale em C:\Program Files\qemu\ ou PATH" -ForegroundColor Red
        exit 1
    }
}

$ovmf = "C:\PROGRA~1\qemu\share\edk2-x86_64-code.fd"
if (-not (Test-Path $ovmf)) {
    $ovmfAlt = Join-Path (Split-Path $qemu -Parent) "share\edk2-x86_64-code.fd"
    if (Test-Path $ovmfAlt) { $ovmf = $ovmfAlt }
    else {
        Write-Host "[ERRO] OVMF nao encontrado em $ovmf" -ForegroundColor Red
        exit 1
    }
}

$accelChosen = $Accel
if ($accelChosen -eq "whpx") {
    Write-Host "[INFO] Tentando WHPX; se falhar no launch, cai para TCG" -ForegroundColor Yellow
}

$memStr = "${Mem}G"
Write-Host "[INFO] Mem=$memStr Cores=$Cores Accel=$accelChosen Models=$useModels Instance=$Instance imgDir=$imgDir" -ForegroundColor Cyan

$baseArgs = @(
    "-m", $memStr,
    "-smp", "$Cores",
    "-cpu", "max",
    "-accel", $accelChosen,
    "-drive", "if=pflash,format=raw,file=$ovmf,readonly=on",
    "-drive", "format=raw,file=$uefi,if=ide,index=0",
    "-vga", "std",
    "-display", "gtk",
    "-no-reboot"
)

$netmodeA = Join-Path $target "netmode_a.flag"
$netmodeB = Join-Path $target "netmode_b.flag"
[System.IO.File]::WriteAllBytes($netmodeA, [byte[]]@([byte][char]'S', 10, 0, 3, 2))
[System.IO.File]::WriteAllBytes($netmodeB, [byte[]]@([byte][char]'S', 10, 0, 3, 3))

$modelDir = Join-Path $Root "target"
$modelLoaders = @()
$modelEndAddr = 0x100000000
if ($useModels) {
    $loaders = @(
        @{ file = "BITNET2B.BIN"; addr = 0x100000000 },
        @{ file = "HWEXPRT.BIN"; addr = 0x129200000 },
        @{ file = "HWEXPRT4.BIN"; addr = 0x129400000 },
        @{ file = "hw_expert_v4.bitnet"; addr = 0x129600000 }
    )
    foreach ($L in $loaders) {
        $fp = Join-Path $modelDir $L.file
        if (Test-Path $fp) {
            $modelLoaders += @("-device", "loader,file=$fp,addr=0x$('{0:X}' -f $L.addr)")
            Write-Host "MoE loader: $($L.file) @0x$('{0:X}' -f $L.addr)" -ForegroundColor Green
            if ($L.addr -ge $modelEndAddr) { $modelEndAddr = $L.addr + 0x200000 }
        } else {
            Write-Host "[AVISO] modelo ausente (skip): $fp" -ForegroundColor Yellow
        }
    }
    if ($modelLoaders.Count -eq 0) {
        Write-Host "[AVISO] Nenhum modelo loader encontrado; seguindo sem LLM loaders" -ForegroundColor Yellow
        $useModels = $false
        $modelEndAddr = 0x100000000
    }
} else {
    Write-Host "[INFO] -NoModels/-NoDisk: sem -device loader MoE (boot rapido)" -ForegroundColor Yellow
}

$modelGap = 0x100000
if (-not $useModels) { $modelEndAddr = 0x100000000 }
$netmodeAddr = [math]::Ceiling(($modelEndAddr + $modelGap) / $modelGap) * $modelGap
$netmodeAddrHex = "0x{0:X}" -f [int64]$netmodeAddr

function Start-MeshInstance {
    param(
        [string]$Name,
        [string[]]$QemuArgs,
        [string]$LogPath
    )
    if (Test-Path $LogPath) { Remove-Item $LogPath -Force -ErrorAction SilentlyContinue }
    try {
        $p = Start-Process -FilePath $qemu -ArgumentList $QemuArgs -NoNewWindow -PassThru -ErrorAction Stop
        Start-Sleep -Seconds 2
        if ($p.HasExited -and $accelChosen -eq "whpx") {
            Write-Host "[AVISO] WHPX falhou no launch de $Name (exit=$($p.ExitCode)); caindo para TCG" -ForegroundColor Yellow
            $script:accelChosen = "tcg"
            $fixed = @()
            for ($i = 0; $i -lt $QemuArgs.Count; $i++) {
                if ($QemuArgs[$i] -eq "-accel" -and ($i + 1) -lt $QemuArgs.Count) {
                    $fixed += "-accel"; $fixed += "tcg"; $i++
                } else {
                    $fixed += $QemuArgs[$i]
                }
            }
            $p = Start-Process -FilePath $qemu -ArgumentList $fixed -NoNewWindow -PassThru
            return $p
        }
        return $p
    } catch {
        if ($accelChosen -eq "whpx") {
            Write-Host "[AVISO] WHPX exception: $($_.Exception.Message) - retry TCG" -ForegroundColor Yellow
            $script:accelChosen = "tcg"
            $fixed = @()
            for ($i = 0; $i -lt $QemuArgs.Count; $i++) {
                if ($QemuArgs[$i] -eq "-accel" -and ($i + 1) -lt $QemuArgs.Count) {
                    $fixed += "-accel"; $fixed += "tcg"; $i++
                } else {
                    $fixed += $QemuArgs[$i]
                }
            }
            return (Start-Process -FilePath $qemu -ArgumentList $fixed -NoNewWindow -PassThru)
        }
        throw
    }
}

$logA = Join-Path $logDir "boot_mesh_a.txt"
$logB = Join-Path $logDir "boot_mesh_b.txt"

$argsA = $baseArgs + $modelLoaders + @(
    "-name", "mesh-cloverleaf",
    "-netdev", "socket,listen=127.0.0.1:12345,id=n0",
    "-device", "e1000,netdev=n0,mac=52:54:00:AA:00:01",
    "-device", "loader,file=$netmodeA,addr=$netmodeAddrHex",
    "-serial", "file:$logA"
)
if ($disk) { $argsA += @("-drive", "format=raw,file=$disk,if=ide,index=1") }

$argsB = $baseArgs + $modelLoaders + @(
    "-name", "mesh-hal9000",
    "-netdev", "socket,connect=127.0.0.1:12345,id=n0",
    "-device", "e1000,netdev=n0,mac=52:54:00:BB:00:02",
    "-device", "loader,file=$netmodeB,addr=$netmodeAddrHex",
    "-serial", "file:$logB"
)
if ($disk) { $argsB += @("-drive", "format=raw,file=$disk,if=ide,index=1") }

Write-Host "========================================" -ForegroundColor Cyan
Write-Host "  P2P MESH TEST - ADR-0081" -ForegroundColor Cyan
Write-Host "========================================" -ForegroundColor Cyan
Write-Host "  Accel=$accelChosen smp=$Cores mem=$memStr models=$useModels instance=$Instance" -ForegroundColor Yellow
Write-Host "  A=10.0.3.2 listen  B=10.0.3.3 connect  UDP 42069" -ForegroundColor Green
Write-Host ""

$procA = $null
$procB = $null

if ($Instance -eq "A" -or $Instance -eq "Both") {
    Write-Host "[LANCANDO] Instancia A (Cloverleaf)..." -ForegroundColor Green
    $procA = Start-MeshInstance -Name "A" -QemuArgs $argsA -LogPath $logA
    Write-Host "           PID: $($procA.Id) Log: $logA" -ForegroundColor Gray
    Start-Sleep -Seconds 3
}

if ($Instance -eq "B" -or $Instance -eq "Both") {
    # Rebuild B args if accel fell back during A
    if ($accelChosen -eq "tcg") {
        $argsB = @()
        foreach ($a in ($baseArgs + $modelLoaders + @(
            "-name", "mesh-hal9000",
            "-netdev", "socket,connect=127.0.0.1:12345,id=n0",
            "-device", "e1000,netdev=n0,mac=52:54:00:BB:00:02",
            "-device", "loader,file=$netmodeB,addr=$netmodeAddrHex",
            "-serial", "file:$logB"
        ))) { $argsB += $a }
        for ($i = 0; $i -lt $argsB.Count; $i++) {
            if ($argsB[$i] -eq "-accel" -and ($i + 1) -lt $argsB.Count) { $argsB[$i+1] = "tcg" }
        }
        if ($disk) { $argsB += @("-drive", "format=raw,file=$disk,if=ide,index=1") }
    }
    Write-Host "[LANCANDO] Instancia B (Hal9000)..." -ForegroundColor Green
    $procB = Start-MeshInstance -Name "B" -QemuArgs $argsB -LogPath $logB
    Write-Host "           PID: $($procB.Id) Log: $logB" -ForegroundColor Gray
}

Write-Host ""
Write-Host "Rodando. Monitor: Get-Content -Tail 30 -Wait logs\boot_mesh_a.txt" -ForegroundColor Cyan
Write-Host "Encerrar: Stop-Process -Name qemu* -Force" -ForegroundColor Gray
Write-Host ""

# Keep alive
while ($true) {
    $alive = $false
    if ($procA -and -not $procA.HasExited) { $alive = $true }
    if ($procB -and -not $procB.HasExited) { $alive = $true }
    if (-not $alive) { break }
    Start-Sleep -Seconds 5
}
Write-Host "[FIM] Instancias encerradas. Accel final=$accelChosen" -ForegroundColor Yellow
