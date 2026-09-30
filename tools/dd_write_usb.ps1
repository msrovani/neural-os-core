# s421-lab: grava target/usb_hw.img no pendrive (modo DD) — rodar como ADMIN
# Uso: powershell -NoProfile -ExecutionPolicy Bypass -File tools/dd_write_usb.ps1
# Pendrive alvo: SanDisk Ultra USB 3.0 28.6GB = \\.\PHYSICALDRIVE1 (confirmar!)

$ErrorActionPreference = "Stop"
$IMG = "C:\DEV\neural-os-core-latest\target\usb_hw.img"
$PD  = "\\.\PHYSICALDRIVE1"

if (-NOT ([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
    Write-Host "ERRO: rode como Administrator (raw write em disco)" -ForegroundColor Red
    exit 1
}

# Confirmação dupla — DD apaga TUDO no disco alvo
Write-Host "Alvo:  $PD" -ForegroundColor Yellow
Write-Host "Imagem: $IMG ($([math]::Round((Get-Item $IMG).Length/1GB,2)) GB)" -ForegroundColor Yellow
Get-CimInstance Win32_DiskDrive | Select-Object Index,Model | Format-Table
$ans = Read-Host "CONFIRMA gravar e APAGAR o disco 1 (SanDisk)? digite SIM"
if ($ans -ne "SIM") { Write-Host "Abortado."; exit 0 }

$stream = [System.IO.File]::Open($IMG, 'Open', 'Read', 'None')
try {
    $disk = [System.IO.File]::Open($PD, 'Open', 'Write', 'None')
    try {
        $buf = New-Object byte[] (4MB)
        $sw = [Diagnostics.Stopwatch]::StartNew()
        $total = 0L
        $size = $stream.Length
        while (($n = $stream.Read($buf, 0, $buf.Length)) -gt 0) {
            $disk.Write($buf, 0, $n)
            $total += $n
            if (($total % (256MB)) -lt $buf.Length) {
                "{0}MB ({1}%)" -f ($total/1MB), ($total*100/$size) | Write-Host
            }
        }
        $disk.Flush()
        "GRAVADO $total bytes em $([int]$sw.Elapsed.TotalSeconds)s" | Write-Host -ForegroundColor Green
    } finally { $disk.Close() }
} finally { $stream.Close() }
