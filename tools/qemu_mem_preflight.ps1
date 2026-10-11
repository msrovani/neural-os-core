# qemu_mem_preflight.ps1 - host free-RAM preflight + WHPX probe for QEMU launchers
# PS 5.1, dot-sourceable, no external modules, ASCII-only (PS 5.1 reads .ps1 as cp1252).
# Usage (repo root launcher):  . (Join-Path $Root "tools\qemu_mem_preflight.ps1")
# Usage (tools launcher):      . (Join-Path $PSScriptRoot "qemu_mem_preflight.ps1")

# $null = not probed yet; afterwards holds [bool] (cached - one probe per session)
$script:QemuPreflightWhpxCache = $null

function Get-QemuHostFreeGB {
    # Win32_OperatingSystem.FreePhysicalMemory is reported in KB -> GB = /1MB
    try {
        $kb = (Get-CimInstance Win32_OperatingSystem -ErrorAction Stop).FreePhysicalMemory
        return [double]$kb / 1MB
    } catch {
        Write-Host "[MEM] WARN: free-RAM probe failed (Win32_OperatingSystem)" -ForegroundColor Yellow
        return [double]0
    }
}

function Get-QemuExePath {
    # Same resolution order as the launchers: default install dir, then PATH
    $exe = "C:\Program Files\qemu\qemu-system-x86_64.exe"
    if (Test-Path $exe) { return $exe }
    $alt = Get-Command "qemu-system-x86_64" -ErrorAction SilentlyContinue
    if ($alt) { return $alt.Source }
    return $null
}

function Test-QemuWhpxAvail {
    if ($null -ne $script:QemuPreflightWhpxCache) { return [bool]$script:QemuPreflightWhpxCache }
    $script:QemuPreflightWhpxCache = $false
    $exe = Get-QemuExePath
    if (-not $exe) { return $false }
    try {
        # local scope: do not leak EAP changes into the caller
        $ErrorActionPreference = "Continue"
        $out = & $exe -accel help 2>&1 | Out-String
        if ($out -match "whpx") { $script:QemuPreflightWhpxCache = $true }
    } catch {
        $script:QemuPreflightWhpxCache = $false
    }
    return [bool]$script:QemuPreflightWhpxCache
}

function Assert-QemuMemFree {
    param([double]$NeededGB, [string]$Label = '', [switch]$Force)
    if (-not $Label) { $Label = "qemu" }
    $free = Get-QemuHostFreeGB

    if ($free -ge $NeededGB) {
        Write-Host ("[MEM] OK free={0}GB needed={1}GB label={2}" -f `
            ([math]::Round($free, 1)), ([math]::Round($NeededGB, 1)), $Label) -ForegroundColor Green
        return
    }

    if ($Force) {
        Write-Host ""
        Write-Host "!!!! WARNING QEMU MEM OVERCOMMIT - CONTINUING (-Force) !!!!" -ForegroundColor Yellow
        Write-Host ("  label   : {0}" -f $Label) -ForegroundColor Yellow
        Write-Host ("  free    : {0} GB" -f ([math]::Round($free, 1))) -ForegroundColor Yellow
        Write-Host ("  needed  : {0} GB" -f ([math]::Round($NeededGB, 1))) -ForegroundColor Yellow
        Write-Host "  Host will page - guest tick degrades under pressure." -ForegroundColor Yellow
        Write-Host "!!!! WARNING QEMU MEM OVERCOMMIT - CONTINUING (-Force) !!!!" -ForegroundColor Yellow
        Write-Host ""
        return
    }

    Write-Host ""
    Write-Host "!!!! QEMU MEM PREFLIGHT FAILED - ABORT (use -Force to overcommit) !!!!" -ForegroundColor Red
    Write-Host ("  label   : {0}" -f $Label) -ForegroundColor Red
    Write-Host ("  free    : {0} GB" -f ([math]::Round($free, 1))) -ForegroundColor Red
    Write-Host ("  needed  : {0} GB" -f ([math]::Round($NeededGB, 1))) -ForegroundColor Red
    Write-Host "  Free host RAM < guest commit + headroom. Not launching." -ForegroundColor Red
    Write-Host "!!!! QEMU MEM PREFLIGHT FAILED - ABORT !!!!" -ForegroundColor Red
    Write-Host ""
    exit 1
}
