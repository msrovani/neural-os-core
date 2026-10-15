# Boot evidence runner - ASCII only (PowerShell 5.1 reads cp1252)
param(
  [string]$Name = "boot",
  [string]$Loader1 = "",
  [string]$Loader2 = "",
  [int]$Seconds = 420,
  [string]$Accel = "whpx",
  [string]$Cpu = "Haswell"
)
$ErrorActionPreference = "Stop"
$qemu = "C:\Program Files\qemu\qemu-system-x86_64.exe"
if (-not (Test-Path "logs")) { New-Item -ItemType Directory -Path "logs" | Out-Null }
$ts = Get-Date -Format "yyyyMMdd_HHmmss"
$log = "logs\${Name}_${ts}.txt"
Copy-Item "C:\Program Files\qemu\share\edk2-i386-vars.fd" "target\ovmf_vars.fd" -Force
$qargs = @(
  "-m","6G","-smp","8","-accel",$Accel,"-cpu",$Cpu,
  "-drive","if=pflash,format=raw,file=target/ovmf_code.fd,readonly=on",
  "-drive","if=pflash,format=raw,file=target/ovmf_vars.fd",
  "-drive","format=raw,file=target/uefi.img,if=ide,index=0",
  "-display","none",
  "-serial","file:$log"
)
if ($Loader1 -ne "") { $qargs += @("-device","loader,file=$Loader1,addr=0x100000000") }
if ($Loader2 -ne "") { $qargs += @("-device","loader,file=$Loader2,addr=0x13E000000") }
Set-Content -Path "logs\${Name}_${ts}.args.txt" -Value ($qargs -join " ")
$p = Start-Process -FilePath $qemu -ArgumentList $qargs -PassThru -WindowStyle Hidden
Start-Sleep -Seconds 5
if ($p.HasExited) {
  Write-Output "QEMU_EXITED_EARLY code=$($p.ExitCode)"
} else {
  Write-Output "QEMU_RUNNING pid=$($p.Id)"
}
Start-Sleep -Seconds ($Seconds - 5)
if (-not $p.HasExited) {
  Stop-Process -Id $p.Id -Force
  Write-Output "QEMU_STOPPED after ${Seconds}s"
} else {
  Write-Output "QEMU_EXITED_DURING_WAIT code=$($p.ExitCode)"
}
Start-Sleep -Seconds 2
$sz = (Get-Item $log).Length
Write-Output "LOG=$log BYTES=$sz"
