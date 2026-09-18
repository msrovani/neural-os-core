# Sync AIOS ↔ neural-sgdb: garante junction crates/neural-sgdb → repo comunitário.
# Uso: powershell -NoProfile -File tools\sync-neural-sgdb.ps1
#      powershell -NoProfile -File tools\sync-neural-sgdb.ps1 -Pull
# crates/neural-sgdb/ está no .gitignore — nunca é commitado; deve ser junction.

param(
    [string]$Source = "C:\DEV\neural-sgdb",
    [switch]$Pull
)

$ErrorActionPreference = "Stop"
$Root = Split-Path -Parent $PSScriptRoot
$Dest = Join-Path $Root "crates\neural-sgdb"

if (-not (Test-Path $Source)) {
    Write-Error "Repo neural-sgdb nao encontrado em $Source. Clone: git clone https://github.com/msrovani/neural-sgdb.git $Source"
}

if ($Pull) {
    Write-Host "git pull em $Source ..."
    git -C $Source pull --ff-only
    if ($LASTEXITCODE -ne 0) { Write-Error "git pull falhou" }
}

$ver = (Select-String -Path (Join-Path $Source "Cargo.toml") -Pattern '^version\s*=\s*"([^"]+)"').Matches.Groups[1].Value
$rev = (git -C $Source rev-parse --short HEAD)

if (Test-Path $Dest) {
    $item = Get-Item $Dest -Force
    $target = @($item.Target)[0]
    if ($item.Attributes -band [IO.FileAttributes]::ReparsePoint) {
        if ($target -and ((Resolve-Path $target).Path -eq (Resolve-Path $Source).Path)) {
            Write-Host "OK junction: $Dest -> $Source"
            Write-Host "neural-sgdb v$ver @ $rev"
            exit 0
        }
        Write-Host "Removendo junction antiga (target=$target)..."
        cmd /c "rmdir `"$Dest`"" | Out-Null
    } else {
        Write-Host "Removendo copia desatualizada em $Dest ..."
        Remove-Item -Recurse -Force $Dest
    }
}

cmd /c "mklink /J `"$Dest`" `"$Source`"" | Out-Null
if (-not (Test-Path $Dest)) { Write-Error "mklink /J falhou" }

Write-Host "OK junction criada: $Dest -> $Source"
Write-Host "neural-sgdb v$ver @ $rev (AIOS acompanha tip do repo)"
Write-Host "Validar: cargo check -p k_ai --target-dir target/check-nsgdb"
