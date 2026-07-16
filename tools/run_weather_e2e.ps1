# Orchestrates WHPX weather e2e with serial bridge via run-qemu-whpx.ps1
# Kill/wait default: 15 minutes (Sprint 107 loops). Override: -KillMinutes 18
# NOTA: NAO usar Start-Job — jobs PS podem impedir bind TCP do serial_bridge.
# GUI: passa -Window ao run-qemu-whpx.ps1 (sem -nographic / -display none).
param(
    [int]$KillMinutes = 15,
    [switch]$Window = $true,
    [int]$Smp = 2
)
$ErrorActionPreference = "Continue"
$Root = Split-Path $PSScriptRoot -Parent
Set-Location $Root

# Auto-kill deadline — BPE chat + soft_stride + max_gen; default 15m (Sprint 107).

Get-Process qemu-system-x86_64 -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
Get-CimInstance Win32_Process -Filter "Name='python.exe'" -ErrorAction SilentlyContinue | Where-Object {
    $_.CommandLine -match 'serial_bridge'
} | ForEach-Object {
    Stop-Process -Id $_.ProcessId -Force -ErrorAction SilentlyContinue
    Write-Host "killed leftover bridge pid=$($_.ProcessId)"
}
Start-Sleep -Seconds 1

$before = @(Get-ChildItem (Join-Path $Root "logs\boot_whpx_*.txt") -ErrorAction SilentlyContinue |
    Sort-Object LastWriteTime -Descending | Select-Object -First 1 -ExpandProperty FullName)
Write-Host "BEFORE_LOG=$before"

$psOut = Join-Path $Root "logs\whpx_runner_out.log"
$psErr = Join-Path $Root "logs\whpx_runner_err.log"

# -Window: QEMU GUI on screen (screenshots). Normal style so console runner is visible too.
# Nao usar -NoSerialBridge — bridge sobe antes do QEMU e morre no finally do script.
# SMP=2: WHPX mais estavel no soft-float 2B (SMP=4 ja saiu cedo no FWD).
$qemuArgs = @("-NoProfile", "-ExecutionPolicy", "Bypass", "-File", (Join-Path $Root "run-qemu-whpx.ps1"), "-Smp", "$Smp")
if ($Window) { $qemuArgs += "-Window" }
$runner = Start-Process -FilePath "powershell.exe" `
    -ArgumentList $qemuArgs `
    -WorkingDirectory $Root `
    -PassThru -WindowStyle Normal

Write-Host "runner pid=$($runner.Id) kill_timeout=${KillMinutes}m Window=$Window Smp=$Smp"
Write-Host "runner_out_hint=$psOut (unused; use boot_whpx + bridge logs)"

function Test-ReadableGenerate {
    param([string]$LogPath)
    $line = Select-String -Path $LogPath -Pattern "decoded_len=(\d+)\s+text='([^']*)'" -ErrorAction SilentlyContinue |
        Select-Object -Last 1
    if (-not $line -or $line.Matches.Count -lt 1) { return $false }
    $n = [int]$line.Matches[0].Groups[1].Value
    $text = $line.Matches[0].Groups[2].Value
    if ($n -lt 8) { return $false }
    if ([string]::IsNullOrWhiteSpace($text)) { return $false }
    # Tem letras ASCII (PT/EN)
    if ($text -notmatch '[A-Za-z]') { return $false }
    # Nao e spam do mesmo caractere (ex: 6666, aaaa)
    $chars = $text.ToCharArray() | Where-Object { $_ -ne ' ' }
    if ($chars.Count -ge 4) {
        $uniq = ($chars | Select-Object -Unique).Count
        if ($uniq -le 1) { return $false }
    }
    # Nao so digitos
    if ($text -match '^\d+$') { return $false }
    return $true
}

function Test-WeatherishGenerate {
    param([string]$LogPath)
    $line = Select-String -Path $LogPath -Pattern "decoded_len=(\d+)\s+text='([^']*)'" -ErrorAction SilentlyContinue |
        Select-Object -Last 1
    if (-not $line -or $line.Matches.Count -lt 1) { return $false }
    $text = $line.Matches[0].Groups[2].Value
    # Lexico clima: exige ≥2 hits (anti-"tempoLie maze")
    return [bool]($text -match '(?i)(tempo|clima|weather|sol|sunny|rain|chuva|hoje|nubl|cloud|frio|quent|dia|Celsius|claro|climate|bom|esta).*(tempo|clima|weather|sol|sunny|rain|chuva|hoje|nubl|cloud|frio|quent|dia|Celsius|claro|climate|bom|esta)|(?i)(hoje|tempo).{0,24}(bom|sol|claro|sunny|rain|dia)')
}

$deadline = (Get-Date).AddMinutes($KillMinutes)
$got = $false
$gotWeather = $false
while ((Get-Date) -lt $deadline) {
    Start-Sleep -Seconds 10
    if ($runner.HasExited -and -not (Get-Process qemu-system-x86_64 -ErrorAction SilentlyContinue)) {
        Write-Host "RUNNER_DONE exit=$($runner.ExitCode)"
        break
    }
    $latest = Get-ChildItem (Join-Path $Root "logs\boot_whpx_*.txt") -ErrorAction SilentlyContinue |
        Where-Object { $_.FullName -ne $before } |
        Sort-Object LastWriteTime -Descending | Select-Object -First 1
    if ($null -eq $latest) {
        $latest = Get-ChildItem (Join-Path $Root "logs\boot_whpx_*.txt") -ErrorAction SilentlyContinue |
            Sort-Object LastWriteTime -Descending | Select-Object -First 1
    }
    if ($null -eq $latest) {
        Write-Host "waiting... no log yet"
        continue
    }
    $hit = Select-String -Path $latest.FullName -Pattern "JARBAS-TTS|FAILED empty|decoded_len=|pcm_samples=|JARBAS-TTS-FB|BPE\] BPB1|soft_stride|chat_frame" -ErrorAction SilentlyContinue
    $decOk = Test-ReadableGenerate -LogPath $latest.FullName
    $wxOk = Test-WeatherishGenerate -LogPath $latest.FullName
    $pcmOk = [bool](Select-String -Path $latest.FullName -Pattern "pcm_samples=([1-9]\d*)" -ErrorAction SilentlyContinue |
        Select-Object -First 1)
    $fbOk = [bool](Select-String -Path $latest.FullName -Pattern "JARBAS-TTS-FB\] painted" -ErrorAction SilentlyContinue |
        Select-Object -First 1)
    # Log HIT but do NOT early-exit — keep GUI up until KillMinutes for screenshots.
    if ($hit -and $decOk -and $pcmOk -and -not $got) {
        Write-Host "HIT readable-gen+TTS in $($latest.Name) letters ok pcm ok fb=$fbOk weatherish=$wxOk (continuing until ${KillMinutes}m kill)"
        $hit | ForEach-Object { Write-Host $_.Line }
        $got = $true
        $gotWeather = $wxOk
    }
    Write-Host "waiting... $($latest.Name) size=$($latest.Length) qemu=$((Get-Process qemu-system-x86_64 -ErrorAction SilentlyContinue) -ne $null) readable=$decOk weatherish=$wxOk"
}

Get-Process qemu-system-x86_64 -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
Start-Sleep -Seconds 3
if (-not $runner.HasExited) {
    Stop-Process -Id $runner.Id -Force -ErrorAction SilentlyContinue
}
Start-Sleep -Seconds 2
Write-Host "=== CLEANUP leftover bridge (if any) ==="
Get-CimInstance Win32_Process -Filter "Name='python.exe'" -ErrorAction SilentlyContinue | Where-Object {
    $_.CommandLine -match 'serial_bridge'
} | ForEach-Object {
    Stop-Process -Id $_.ProcessId -Force -ErrorAction SilentlyContinue
    Write-Host "killed leftover bridge pid=$($_.ProcessId)"
}

$latest = Get-ChildItem (Join-Path $Root "logs\boot_whpx_*.txt") -ErrorAction SilentlyContinue |
    Sort-Object LastWriteTime -Descending | Select-Object -First 1
Write-Host "FINAL_LOG=$($latest.FullName)"
Write-Host "GOT_READABLE_TTS=$got"
Write-Host "GOT_WEATHERISH=$gotWeather"
if ($latest) {
    Write-Host "=== KEY LINES ==="
    Select-String -Path $latest.FullName -Pattern "BRIDGE|STATUS|LLM LOADED|JARBAS|HERMES|\[GEN\]|\[FWD\] soft|PIPER|TTS|FAILED|pcm_samples|decoded|BGE|TTS-FB|BPE\]|HWEXPERT|RUSTCODER|\[STT\]" |
        ForEach-Object { Write-Host $_.Line }
    Write-Host "=== EXPERT LOADERS ==="
    Select-String -Path $latest.FullName -Pattern "HWEXPERT|RUSTCODER|BGE\]|STT\] CTC" |
        ForEach-Object { Write-Host $_.Line }
}
