# run-qemu-whpx.ps1 — neural-os-core v1.0
# QEMU WHPX + VirtIO optimizado para Windows
# Uso: .\run-qemu-whpx.ps1 [-debug] [-smp N] [-mem 4G] [-log]

param(
    [switch]$debug = $false,
    [int]$smp = 4,
    [string]$mem = "4G",
    [switch]$log = $true
)

$QEMU = "qemu-system-x86_64.exe"
$KERNEL = "target\x86_64-neural_os\debug\bootimage-neural-os-core.bin"
$DISK = "disk.raw"

# Verifica se os arquivos existem
if (-not (Test-Path $KERNEL)) {
    Write-Error "Kernel nao encontrado: $KERNEL`nRode 'cargo build' primeiro."
    exit 1
}
if (-not (Test-Path $DISK)) {
    Write-Warning "Disco secundario nao encontrado: $DISK (opcional, ignorando)"
    Write-Warning "Crie com:   python build_image.py"
    $DISK = $null
}

# Cria pasta de logs com timestamp
if ($log) {
    $timestamp = Get-Date -Format "yyyyMMdd-HHmmss"
    $null = New-Item -ItemType Directory -Path "logs" -Force
    $logFile = "logs\qemu-$timestamp.log"
    # Redireciona todo output do PowerShell para o log
    Start-Transcript -Path $logFile -Append | Out-Null
    Write-Host "[LOG] Sessao registrada em $logFile"
}

$args = @(
    "-accel", "whpx",
    "-cpu", "host",
    "-m", $mem,
    "-smp", $smp,
    "-drive", "format=raw,file=$KERNEL,if=ide"
)

if ($DISK) {
    $args += "-drive", "format=raw,file=$DISK,if=ide"
}

$args += @(
    # VirtIO-net (prioridade maxima no kernel)
    "-netdev", "user,id=net0,hostfwd=tcp::5555-:5555",
    "-device", "virtio-net-pci,netdev=net0",
    # VirtIO-GPU (framebuffer moderno)
    "-vga", "virtio",
    "-display", "default",
    # COM1: console debug
    "-serial", "mon:stdio",
    # COM2: tunnel SLIP para rede (serial_bridge escuta esta porta)
    "-serial", "tcp:127.0.0.1:4444,server,nowait",
    # Sem NIC default (senao QEMU cria RTL8139 alem da virtio-net)
    "-nic", "none"
)

if ($debug) {
    # Modo debug: QEMU espera GDB conectar
    $args += @("-s", "-S")
    Write-Host "[QEMU] Modo DEBUG - aguardando GDB em localhost:1234"
}

Write-Host @"

[QEMU] neural-os-core v1.0 — WHPX + VirtIO
[QEMU] CPU: host ($smp cores)  RAM: $mem  ACCEL: WHPX
[QEMU] GPU: VirtIO  NET: virtio-net-pci  DISK: $(if ($DISK) {"IDE+AHCI"} else {"IDE only"})
[QEMU] Serial: COM1=stdio (console)  COM2=tcp:4444 (tunnel SLIP)
[QEMU] Acesse a janela grafica do QEMU para ver o framebuffer
[QEMU] Para rede: 'python serial_bridge.py' (outro terminal)
[QEMU] Log: $(if ($log) {$logFile} else {"desabilitado"})
"@

& $QEMU $args

if ($LASTEXITCODE -ne 0) {
    Write-Error "[QEMU] Falhou com codigo $LASTEXITCODE"
    if ($log) { Stop-Transcript | Out-Null }
    exit $LASTEXITCODE
}

if ($log) {
    Stop-Transcript | Out-Null
    Write-Host "[LOG] Sessao salva em $logFile"
}
