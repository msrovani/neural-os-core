# write_usb_hd.ps1 - grava a imagem HW (usb_hw.img) direto num HD externo USB,
# bootavel como se fosse pendrive. Equivalente ao modo DD do Rufus.
#
# Uso (PowerShell COMO ADMIN):
#   .\tools\write_usb_hd.ps1                          # usa target\usb_hw.img
#   .\tools\write_usb_hd.ps1 -Image target\usb_hw.img -DiskNumber 2
#
# Requisito: gerar a imagem antes ->
#   python tools\build_image.py --hw --unified [--size 6144]
param(
    [string]$Image = "target\usb_hw.img",
    [int]$DiskNumber = -1
)

$ErrorActionPreference = "Stop"

if (-not (Test-Path -LiteralPath $Image)) {
    Write-Host "[ERRO] Imagem nao encontrada: $Image" -ForegroundColor Red
    Write-Host "Gere primeiro: python tools\build_image.py --hw --unified"
    exit 1
}

$isAdmin = ([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()
           ).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
if (-not $isAdmin) { Write-Host "[ERRO] Rode como Administrador." -ForegroundColor Red; exit 1 }

$imgPath = (Resolve-Path -LiteralPath $Image).Path
$imgSize = (Get-Item -LiteralPath $imgPath).Length

# Lista candidatos (USB ou removivel)
$disks = Get-Disk | Where-Object { $_.BusType -in @('USB','Removable') }
if (-not $disks) { Write-Host "[ERRO] Nenhum disco USB/removivel encontrado." -ForegroundColor Red; exit 1 }

$disks | Format-Table Number, FriendlyName,
    @{n='SizeGB';e={[math]::Round($_.Size/1GB,1)}},
    @{n='Sector';e={$_.LogicalSectorSize}},
    PartitionStyle -AutoSize

if ($DiskNumber -lt 0) {
    $DiskNumber = [int](Read-Host "Numero do disco ALVO (PhysicalDrive)")
}

$disk = Get-Disk -Number $DiskNumber -ErrorAction SilentlyContinue
if (-not $disk) { Write-Host "[ERRO] Disco $DiskNumber nao existe." -ForegroundColor Red; exit 1 }
if ($disk.BusType -notin @('USB','Removable')) {
    Write-Host "[ERRO] Disco $DiskNumber nao e USB/removivel (BusType=$($disk.BusType)). Recusa por seguranca." -ForegroundColor Red
    exit 1
}
if ($disk.Size -lt $imgSize) {
    Write-Host ("[ERRO] Disco ({0:N1} GB) menor que a imagem ({1:N1} GB)." -f ($disk.Size/1GB), ($imgSize/1GB)) -ForegroundColor Red
    exit 1
}
# A imagem e gerada com GPT/MBR assumindo setores de 512B. Bridges USB "4K native"
# quebram o boot silenciosamente - recusar cedo.
if ($disk.LogicalSectorSize -ne 512) {
    Write-Host "[ERRO] Setor logico = $($disk.LogicalSectorSize) bytes (4K native). A imagem assume 512B." -ForegroundColor Red
    Write-Host "       GPT/MBR nao seriam encontrados pelo firmware. Use outro enclosure/cabo."
    exit 1
}

Write-Warning ("Vai APAGAR TUDO em PhysicalDrive{0} ({1}, {2:N1} GB) e gravar {3}" -f `
    $DiskNumber, $disk.FriendlyName, ($disk.Size/1GB), $imgPath)
if ((Read-Host "Digite SIM para confirmar") -cne 'SIM') { Write-Host "Abortado."; exit 1 }

# Limpa particoes ANTES do raw write: solta os handles/volume-cache do Windows
Set-Disk -Number $DiskNumber -IsOffline $false
Clear-Disk -Number $DiskNumber -RemoveData -RemoveOEM -Confirm:$false

$src = [IO.File]::OpenRead($imgPath)
try {
    $dst = [IO.File]::Open("\\.\PhysicalDrive$DiskNumber", 'Open', 'Write', 'ReadWrite')
    try {
        $buf = New-Object byte[] 4194304   # 4 MB
        $total = [long]$src.Length
        $done = [long]0
        while (($r = $src.Read($buf, 0, $buf.Length)) -gt 0) {
            $dst.Write($buf, 0, $r)
            $done += $r
            Write-Progress -Activity "Gravando imagem" -Status "$([math]::Round($done/1MB)) / $([math]::Round($total/1MB)) MB" `
                           -PercentComplete ([int](100 * $done / $total))
        }
        $dst.Flush()
        $flush = [IO.File]::Open("\\.\PhysicalDrive$DiskNumber", 'Open', 'Read', 'ReadWrite') # sanity re-open
        $flush.Close()
    } finally { $dst.Close() }
} finally { $src.Close() }

Update-HostStorageCache
Write-Host ""
Write-Host "OK. Imagem gravada em PhysicalDrive$DiskNumber." -ForegroundColor Green
Write-Host "Boot: conecte o HD, F2/F12 -> escolha a entrada USB do HD. Secure Boot OFF."
