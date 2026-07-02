# Setup Hyper-V VM for Neural OS Hermes
# Execute como ADMINISTRADOR (PowerShell as Administrator)

$VMPath = "C:\dev\neural-os-core"
$VHDPath = "$VMPath\disk.vhd"

# 1. Criar VM
Write-Host "[1/4] Criando VM NeuralOS..."
New-VM -Name "NeuralOS" -MemoryStartupBytes 4GB -BootDevice VHD -VHDPath $VHDPath

# 2. Configurar CPU (4 cores + expor virtualizacao)
Write-Host "[2/4] Configurando CPU..."
Set-VMProcessor -VMName "NeuralOS" -Count 4 -ExposeVirtualizationExtensions $true

# 3. Habilitar serial COM1 para debug
Write-Host "[3/4] Habilitando serial..."
Set-VMComPort -VMName "NeuralOS" -Number 1 -Path "\\.\pipe\NeuralOSSerial"

# 4. Configurar rede (Default Switch com DHCP)
Write-Host "[4/4] Configurando rede..."
Remove-VMNetworkAdapter -VMName "NeuralOS" -ErrorAction SilentlyContinue
Add-VMNetworkAdapter -VMName "NeuralOS" -SwitchName "Default Switch"
Set-VMNetworkAdapter -VMName "NeuralOS" -Name "Network Adapter" -DeviceNaming "Intel E1000"

Write-Host ""
Write-Host "VM 'NeuralOS' criada!"
Write-Host ""
Write-Host "Para iniciar:"
Write-Host "  Start-VM -Name 'NeuralOS'"
Write-Host ""
Write-Host "Para ver serial (em outro terminal como admin):"
Write-Host "  powershell -Command { cat \\.\pipe\NeuralOSSerial }"
Write-Host ""
Write-Host "Para parar:"
Write-Host "  Stop-VM -Name 'NeuralOS' -Force"
