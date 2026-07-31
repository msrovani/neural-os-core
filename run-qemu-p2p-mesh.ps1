<#
.SYNOPSIS
  Lanca 2 instancias QEMU com link socket ponto-a-ponto para teste P2P Mesh (ADR-0081).
  Instancia A (Cloverleaf) escuta em 127.0.0.1:12345; Instancia B (Hal9000) conecta.
  Logs separados: logs/boot_mesh_a.txt e logs/boot_mesh_b.txt.
  IPs estaticos: A=10.0.3.2, B=10.0.3.3 (via netmode flag em NETMODE_LOADER_PHYS).
.DESCRIPTION
  SESSION_233: multicast 230.0.0.1 NAO funciona no Windows ("can't bind ip=230.0.0.1").
  Usar socket,listen / socket,connect (mesmo dominio L2, broadcast UDP funciona).
  -m 8G obrigatorio: 2G estoura o heap bump, 4G da #PF no scan do QEMU-loader.
  OVMF pflash obrigatorio: uefi.img e UEFI-only (firmware built-in nao boota).
  IMPORTANTE: este arquivo deve ser ASCII puro (PS 5.1 le sem BOM como ANSI;
  caracteres multibyte UTF-8 quebram o parse em strings/comentarios).
#>

param(
    [switch]$NoDisk
)

# Script esta na raiz do repo — $PSScriptRoot e o proprio diretorio do projeto.
# (Split-Path -Parent pegaria o pai, ex.: C:\DEV em vez de C:\DEV\neural-os-core.)
$Root = $PSScriptRoot
$target = Join-Path $Root "target"
$logDir = Join-Path $Root "logs"

# Ensure log directory exists
New-Item -ItemType Directory -Path $logDir -Force | Out-Null

# Find the UEFI image and disk image
$uefi = Join-Path $target "uefi.img"
$disk = Join-Path $target "disk_qemu.raw"
# -NoDisk: teste P2P e rede pura — a leitura FAT32 dos modelos (202MB) via
# ATA PIO sob TCG atrasa/trava o boot. Sem o segundo drive, boot vai direto
# ao runtime. (Usar -NoDisk para validar mesh/skills.)
if (-not (Test-Path $uefi)) {
    Write-Host "[ERRO] uefi.img nao encontrado em $target" -ForegroundColor Red
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

# QEMU path
$qemu = "C:\Program Files\qemu\qemu-system-x86_64.exe"
if (-not (Test-Path $qemu)) {
    # Try common alternatives
    $alt = Get-Command "qemu-system-x86_64" -ErrorAction SilentlyContinue
    if ($alt) { $qemu = $alt.Source }
    else {
        Write-Host "[ERRO] QEMU nao encontrado. Instale em C:\Program Files\qemu\ ou adicione ao PATH" -ForegroundColor Red
        exit 1
    }
}

# OVMF (UEFI) - uefi.img e UEFI-only.
# Usar caminho curto 8.3 (PROGRA~1) — caminho longo com espaco quebra no
# -drive via Start-Process -ArgumentList (split no "C:\Program").
$ovmf = "C:\PROGRA~1\qemu\share\edk2-x86_64-code.fd"
if (-not (Test-Path $ovmf)) {
    Write-Host "[ERRO] OVMF nao encontrado em $ovmf" -ForegroundColor Red
    exit 1
}

# Common base config (SESSION_233: 8G RAM, TCG, OVMF pflash)
# -smp 2: MTTCG (multi-threaded TCG) e ~4x mais rapido que -smp 1 (single-thread).
# Wake de AP no TCG e flaky (ADR-0057) mas raro; se A travar em INIT-SIPI-SIPI,
# relancar (retry resolve). -smp 1 deixa o boot TCG lento demais (~4min p/ T+1000).
$baseArgs = @(
    "-m", "8G",
    "-smp", "2",
    "-cpu", "max",
    "-accel", "tcg",
    "-drive", "if=pflash,format=raw,file=$ovmf,readonly=on",
    "-drive", "format=raw,file=$uefi,if=ide,index=0",
    "-vga", "std",
    "-display", "gtk",
    "-no-reboot"
)

# NetMode flags: IP customizado para cada instancia.
# SESSION_233: kernel espera NETMODE_LOADER_PHYS = 0x164000000 (5.56GB, dentro de 8GB).
$netmodeAddr = 0x164000000  # NETMODE_LOADER_PHYS (corrigido)
$netmodeA = Join-Path $target "netmode_a.flag"
$netmodeB = Join-Path $target "netmode_b.flag"

# Instance A: 10.0.3.2 -> 'S' + [10, 0, 3, 2]
[System.IO.File]::WriteAllBytes($netmodeA, [byte[]]@([byte][char]'S', 10, 0, 3, 2))
# Instance B: 10.0.3.3 -> 'S' + [10, 0, 3, 3]
[System.IO.File]::WriteAllBytes($netmodeB, [byte[]]@([byte][char]'S', 10, 0, 3, 3))

# Instance A: Cloverleaf (10.0.3.2) - LISTEN primeiro
$logA = Join-Path $logDir "boot_mesh_a.txt"
$argsA = $baseArgs + @(
    "-name", "mesh-cloverleaf",
    "-netdev", "socket,listen=127.0.0.1:12345,id=n0",
    "-device", "e1000,netdev=n0,mac=52:54:00:AA:00:01",
    "-device", "loader,file=$netmodeA,addr=$netmodeAddr",
    "-serial", "file:$logA"
)
if ($disk) { $argsA += @("-drive", "format=raw,file=$disk,if=ide,index=1") }

# Instance B: Hal9000 (10.0.3.3) - CONNECT
$logB = Join-Path $logDir "boot_mesh_b.txt"
$argsB = $baseArgs + @(
    "-name", "mesh-hal9000",
    "-netdev", "socket,connect=127.0.0.1:12345,id=n0",
    "-device", "e1000,netdev=n0,mac=52:54:00:BB:00:02",
    "-device", "loader,file=$netmodeB,addr=$netmodeAddr",
    "-serial", "file:$logB"
)
if ($disk) { $argsB += @("-drive", "format=raw,file=$disk,if=ide,index=1") }

# Launch
Write-Host "========================================" -ForegroundColor Cyan
Write-Host "  P2P MESH TEST - ADR-0081" -ForegroundColor Cyan
Write-Host "========================================" -ForegroundColor Cyan
Write-Host ""
Write-Host "  A (Cloverleaf): 10.0.3.2 (listen)" -ForegroundColor Green
Write-Host "    Log: $logA" -ForegroundColor Gray
Write-Host "    MAC: 52:54:00:AA:00:01" -ForegroundColor Gray
Write-Host ""
Write-Host "  B (Hal9000):    10.0.3.3 (connect)" -ForegroundColor Green
Write-Host "    Log: $logB" -ForegroundColor Gray
Write-Host "    MAC: 52:54:00:BB:00:02" -ForegroundColor Gray
Write-Host ""
Write-Host "  Rede: socket 127.0.0.1:12345 (listen/connect)" -ForegroundColor Yellow
Write-Host "  TCG (sem WHPX) - mais lento, sem falha VP exit" -ForegroundColor Yellow
Write-Host ""
Write-Host "  O que observar:" -ForegroundColor Cyan
Write-Host "  1. Ambas sobem ate NetAgent" -ForegroundColor Cyan
Write-Host "  2. UDP broadcast na porta 42069" -ForegroundColor Cyan
Write-Host "  3. MESH_ENGINE descobre o outro no" -ForegroundColor Cyan
Write-Host "  4. Eleicao: um vira Master, outro Worker" -ForegroundColor Cyan
Write-Host "  5. SkillSync: Master push skills -> Worker aplica" -ForegroundColor Cyan
Write-Host "  6. Marketplace: skills anunciadas entre nos" -ForegroundColor Cyan
Write-Host ""
Write-Host "  Logs:" -ForegroundColor Gray
Write-Host "    tail -f logs/boot_mesh_a.txt" -ForegroundColor Gray
Write-Host "    tail -f logs/boot_mesh_b.txt" -ForegroundColor Gray
Write-Host ""

# Launch A in background (listen primeiro)
Write-Host "[LANCANDO] Instancia A (Cloverleaf)..." -ForegroundColor Green
$procA = Start-Process -FilePath $qemu -ArgumentList $argsA -NoNewWindow -PassThru
Write-Host "           PID: $($procA.Id)" -ForegroundColor Gray
Start-Sleep -Seconds 3

# Launch B in background (connect depois)
Write-Host "[LANCANDO] Instancia B (Hal9000)..." -ForegroundColor Green
$procB = Start-Process -FilePath $qemu -ArgumentList $argsB -NoNewWindow -PassThru
Write-Host "           PID: $($procB.Id)" -ForegroundColor Gray

Write-Host ""
Write-Host "Ambas as instancias estao rodando." -ForegroundColor Green
Write-Host "Acompanhe os logs para ver a descoberta P2P!" -ForegroundColor Cyan
Write-Host ""
Write-Host "Para encerrar: Ctrl+C ou:" -ForegroundColor Gray
Write-Host "  Stop-Process -Id $($procA.Id) -Force" -ForegroundColor Gray
Write-Host "  Stop-Process -Id $($procB.Id) -Force" -ForegroundColor Gray
Write-Host ""

# Keep alive enquanto as instancias rodam
while (-not $procA.HasExited -and -not $procB.HasExited) {
    Start-Sleep -Seconds 5
}
Write-Host "[FIM] Instancias encerradas." -ForegroundColor Yellow
