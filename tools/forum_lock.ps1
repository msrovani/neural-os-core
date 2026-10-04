# Shared forum write lock - PowerShell side.
# Same lock file as tools/forum_lock.py (.forum_write.lock, next to the log), so a
# Python agent and a PowerShell agent contend on the SAME file. Both write the
# same JSON holder ({pid, who, ts, token}), so either one can read the other and
# detect an orphaned lock.
#
# Why it exists: tools/forum_loop_opkimi.ps1 appended with Add-Content and took NO
# lock at all, so it could interleave with (or reuse an id against) anyone posting
# through forum_post.py.
#
# ASCII-ONLY on purpose: PowerShell 5.1 reads .ps1 as cp1252 and non-ASCII turns
# into mojibake that breaks the script with no obvious syntax error.
#
#   . tools\forum_lock.ps1
#   $h = Enter-ForumWriteLock -Log $ForumPath -Who 'OPMUSE'
#   try { Add-Content -LiteralPath $ForumPath -Value $msg } finally { Exit-ForumWriteLock -Handle $h }

# Sem Set-StrictMode de proposito: este arquivo e dot-sourced e o strict
# mode se tornaria do CHAMADOR, quebrando o script que nos usa.

function Get-ForumLockPath {
    param([string]$Log)
    if ([string]::IsNullOrEmpty($Log)) { $Log = (Join-Path ([Environment]::GetFolderPath('Desktop')) "LOG AGENTES .txt") }
    return (Join-Path (Split-Path -Parent ([System.IO.Path]::GetFullPath($Log))) ".forum_write.lock")
}

function Get-ForumEpoch {
    return [double]((Get-Date).ToUniversalTime() - [DateTime]'1970-01-01').TotalSeconds
}

function Read-ForumLockHolder {
    param([string]$Path)
    try {
        $raw = [System.IO.File]::ReadAllText($Path)
    } catch { return $null }
    if ([string]::IsNullOrWhiteSpace($raw)) { return $null }
    try { return ($raw | ConvertFrom-Json) } catch {
        # lock no formato texto antigo (so o pid)
        $n = 0
        if ([int]::TryParse($raw.Trim(), [ref]$n)) {
            return [pscustomobject]@{ pid = $n; who = 'legacy'; ts = 0; token = '' }
        }
        return [pscustomobject]@{ pid = 0; who = 'ilegivel'; ts = 0; token = '' }
    }
}

function Test-ForumPidAlive {
    param([int]$Id)
    if ($Id -le 0) { return $false }
    try { return ($null -ne (Get-Process -Id $Id -ErrorAction SilentlyContinue)) } catch { return $true }
}

function Enter-ForumWriteLock {
    <#
      Exclusao real vem de FileMode::CreateNew, que e atomico no SO: falha se o
      arquivo ja existe. Fail-closed: se nao obter no tempo, LANCA - nunca
      escrever sem lock (um write sem lock e o que duplica id).
    #>
    param(
        [string]$Log = "",
        [string]$Who = "powershell",
        [double]$TimeoutSec = 20.0,
        [double]$StaleAfterSec = 120.0
    )
    $path = Get-ForumLockPath -Log $Log
    $dir = Split-Path -Parent $path
    if (-not (Test-Path -LiteralPath $dir)) { New-Item -ItemType Directory -Force -Path $dir | Out-Null }
    $token = [Guid]::NewGuid().ToString('N').Substring(0, 12)
    $deadline = (Get-Date).AddSeconds($TimeoutSec)
    $delayMs = 10
    while ($true) {
        try {
            # CreateNew = atomico. FileShare::None = ninguem mexe enquanto escrevo.
            $fs = [System.IO.File]::Open($path, [System.IO.FileMode]::CreateNew,
                                         [System.IO.FileAccess]::Write,
                                         [System.IO.FileShare]::None)
            try {
                $payload = @{
                    pid   = $PID
                    who   = $Who
                    ts    = Get-ForumEpoch
                    token = $token
                } | ConvertTo-Json -Compress
                $bytes = [System.Text.Encoding]::UTF8.GetBytes($payload)
                $fs.Write($bytes, 0, $bytes.Length)
                $fs.Flush($true)
            } finally { $fs.Dispose() }
            return [pscustomobject]@{ Path = $path; Token = $token; Who = $Who; Log = $Log }
        } catch [System.IO.IOException] {
            $h = Read-ForumLockHolder -Path $path
            if ($null -ne $h) {
                $age = (Get-ForumEpoch) - [double]$h.ts
                $alive = Test-ForumPidAlive -Id ([int]$h.pid)
                # Orfao (dono morreu) ou pid reciclado + lock muito velho.
                if ((-not $alive) -or ($age -gt $StaleAfterSec)) {
                    Write-Host "[f15/lock] roubou lock orfao de $($h.who) (pid=$($h.pid) ha $([int]$age)s)"
                    try { Remove-Item -Force -LiteralPath $path -ErrorAction SilentlyContinue } catch {}
                    continue
                }
            }
            if ((Get-Date) -gt $deadline) {
                throw ("nao obtive o lock do forum ($path) em {0:N0}s - NAO escrever sem lock" -f $TimeoutSec)
            }
            Start-Sleep -Milliseconds $delayMs
            $delayMs = [Math]::Min($delayMs + 10, 200)
        }
    }
}

function Exit-ForumWriteLock {
    param([Parameter(Mandatory = $true)]$Handle)
    if ($null -eq $Handle) { return }
    # So remove se AINDA for o nosso: se fomos roubados, apagar seria o bug.
    $h = Read-ForumLockHolder -Path $Handle.Path
    if ($null -eq $h -or $h.token -eq $Handle.Token) {
        try { Remove-Item -Force -LiteralPath $Handle.Path -ErrorAction SilentlyContinue } catch {}
    }
}

function Get-ForumLockStatus {
    param([string]$Log = "")
    $path = Get-ForumLockPath -Log $Log
    $h = Read-ForumLockHolder -Path $path
    if ($null -eq $h) { return [pscustomobject]@{ Locked = $false; Path = $path } }
    return [pscustomobject]@{
        Locked   = $true
        Path     = $path
        Who      = $h.who
        Pid      = $h.pid
        AgeSec   = [int]((Get-ForumEpoch) - [double]$h.ts)
        PidAlive = Test-ForumPidAlive -Id ([int]$h.pid)
    }
}