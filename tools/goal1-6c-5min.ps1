# Goal 1: QEMU 6 cores / 6GB - boot loop ate UI ativa + 5min sem erros no log serial.
# Aceite: PHASE 7 ok + BOOT SCORE + desktop_ready + T+ ticks + flush BOOT.LOG ok
#         e, durante 300s, sem PANIC/Triple fault/EXC novos e com ticks avancando.
param(
    [int]$Cores = 6,
    [int]$RamGB = 6,
    [int]$MaxAttempts = 4,
    [int]$BootTimeoutSec = 300,
    [int]$StableSec = 300,
    [string]$StatusFile = ""
)
$ErrorActionPreference = "Continue"
$Root = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
Set-Location $Root
$logDir = Join-Path $Root "logs"
New-Item -ItemType Directory -Force -Path $logDir | Out-Null
if (-not $StatusFile) { $StatusFile = Join-Path $logDir "goal1_status.txt" }

function Set-Status([string]$msg) {
    $line = ("{0} {1}" -f (Get-Date -Format "HH:mm:ss"), $msg)
    Add-Content -Path $StatusFile -Value $line
    Write-Host $line
}

function Stop-QemuAll {
    Get-Process qemu-system-x86_64 -ErrorAction SilentlyContinue |
        ForEach-Object { Stop-Process -Id $_.Id -Force -ErrorAction SilentlyContinue }
    Start-Sleep -Seconds 3
}

function Read-LogShared([string]$path) {
    if (-not (Test-Path $path)) { return "" }
    try {
        $fs = [System.IO.File]::Open($path, 'Open', 'Read', 'ReadWrite')
        try {
            $sr = New-Object System.IO.StreamReader($fs)
            return $sr.ReadToEnd()
        } finally { $sr.Close(); $fs.Close() }
    } catch { return "" }
}

function Test-BootAccept([string]$text) {
    $need = @{
        phase7  = ($text -match 'PHASE n=7 name=Runtime status=ok')
        score   = ($text -match 'BOOT SCORE')
        desktop = ($text -match 'desktop_ready')
        sched   = ($text -match 'sched_enter|SCHEDULER|51 runtime agents')
        ticks   = ($text -match '\[T\+[1-9]\d*\]')
        bootlog = ($text -match 'flush BOOT\.LOG ok=true')
        nopanic = -not ($text -match 'Triple fault|PANIC|#GP\(|#UD\(|#PF\(')
    }
    $miss = @($need.GetEnumerator() | Where-Object { -not $_.Value } | ForEach-Object { $_.Key })
    return @{ Pass = ($miss.Count -eq 0); Miss = $miss }
}

function Test-FailMarkers([string]$text) {
    # Erros duros: panic/triple fault/exception. Warnings conhecidos nao contam.
    $m = [regex]::Matches($text, 'Triple fault|PANIC|panic at|#GP\(|EXCEPTION|abort\(\)')
    return $m.Count
}

Set-Status "GOAL1 start cores=$Cores ram=${RamGB}GB attempts=$MaxAttempts stable=${StableSec}s"

$readyLog = ""
$goal = $false
for ($attempt = 1; $attempt -le $MaxAttempts; $attempt++) {
    Stop-QemuAll
    Set-Status "ATTEMPT $attempt/$MaxAttempts : launching run-qemu-whpx.ps1 -Smp $Cores -RamGB $RamGB"
    $proc = Start-Process -FilePath "powershell.exe" `
        -ArgumentList @("-NoProfile", "-ExecutionPolicy", "Bypass", "-File", (Join-Path $Root "run-qemu-whpx.ps1"), "-Smp", "$Cores", "-RamGB", "$RamGB") `
        -PassThru -WorkingDirectory $Root

    $sw = [Diagnostics.Stopwatch]::StartNew()
    $latest = $null
    $accepted = $false
    while (-not $proc.HasExited -and $sw.Elapsed.TotalSeconds -lt $BootTimeoutSec) {
        Start-Sleep -Seconds 8
        $latest = Get-ChildItem (Join-Path $logDir "boot_whpx_*.txt") -ErrorAction SilentlyContinue |
            Sort-Object LastWriteTime -Descending | Select-Object -First 1
        if (-not $latest) { continue }
        $raw = Read-LogShared $latest.FullName
        $chk = Test-BootAccept $raw
        Set-Status ("boot t={0:N0}s pass={1} miss=[{2}] bytes={3}" -f $sw.Elapsed.TotalSeconds, $chk.Pass, ($chk.Miss -join ","), $latest.Length)
        if ($chk.Pass) { $accepted = $true; break }
    }

    if (-not $accepted) {
        Set-Status "ATTEMPT $attempt FAIL (boot/aceite timeout ou qemu saiu procExited=$($proc.HasExited))"
        Stop-QemuAll
        continue
    }

    # === UI ativa: 5 min de observacao ===
    Set-Status "ACCEPTED boot log=$($latest.FullName) - observando $StableSec s por erros"
    $sw2 = [Diagnostics.Stopwatch]::StartNew()
    $clean = $true
    $failsAtStart = 0
    $lastTick = 0
    $lastTailPos = 0
    while ($sw2.Elapsed.TotalSeconds -lt $StableSec) {
        Start-Sleep -Seconds 10
        if ($proc.HasExited) { Set-Status "QEMU saiu durante observacao (t=$([int]$sw2.Elapsed.TotalSeconds)s)"; $clean = $false; break }
        $raw = Read-LogShared $latest.FullName
        $fm = Test-FailMarkers $raw
        if ($sw2.Elapsed.TotalSeconds -le 15) { $failsAtStart = $fm }
        if ($fm -gt $failsAtStart) { Set-Status "ERRO NOVO no log (fails=$fm start=$failsAtStart)"; $clean = $false; break }
        # ticks avancando?
        $tks = [regex]::Matches($raw, '\[T\+(\d+)\]')
        if ($tks.Count -gt 0) {
            $mx = 0; foreach ($t in $tks) { $v = [int]$t.Groups[1].Value; if ($v -gt $mx) { $mx = $v } }
            if ($mx -gt $lastTick) { $lastTick = $mx }
            elseif ($sw2.Elapsed.TotalSeconds -gt 60) { Set-Status "TICKS PARADOS em T+$lastTick"; $clean = $false; break }
        }
        Set-Status ("stable t={0:N0}s tick=T+{1} size={2}" -f $sw2.Elapsed.TotalSeconds, $lastTick, $raw.Length)
    }

    if ($clean -and -not $proc.HasExited) {
        $readyLog = $latest.FullName
        $goal = $true
        Set-Status "GOAL1 OK: UI ativa $StableSec s sem erros. QEMU dejando rodando pid=$($proc.Id) log=$readyLog tick=T+$lastTick"
        break
    }
    Stop-QemuAll
    Set-Status "ATTEMPT $attempt FAIL (observacao) - reiniciando loop"
}

if ($goal) { Set-Status "RESULT=GOAL1_PASS log=$readyLog"; exit 0 }
Set-Status "RESULT=GOAL1_FAIL (esgotado $MaxAttempts tentativas)"
exit 1
