# Standalone executable builder for My Computer Keeper
[CmdletBinding()]
param(
    [string]$OutDir = "dist-standalone",
    [switch]$SkipUiBuild,
    [switch]$Clean,
    [switch]$TestRun
)

$ErrorActionPreference = "Stop"

Write-Host "==========================================================" -ForegroundColor Cyan
Write-Host "  My Computer Keeper - Standalone Executable Builder" -ForegroundColor Cyan
Write-Host "==========================================================" -ForegroundColor Cyan

$RootDir = Split-Path -Parent $PSScriptRoot
Set-Location $RootDir

if ($Clean) {
    Write-Host "[1/4] Cleaning previous build artifacts..." -ForegroundColor Yellow
    if (Test-Path "$RootDir\ui\dist") { Remove-Item -Recurse -Force "$RootDir\ui\dist" }
    if (Test-Path "$RootDir\$OutDir") { Remove-Item -Recurse -Force "$RootDir\$OutDir" }
}

# 1. Build Frontend UI
if (-not $SkipUiBuild) {
    Write-Host "[1/3] Building UI bundle (React + Vite + TailwindCSS)..." -ForegroundColor Green
    Push-Location "$RootDir\ui"
    try {
        if (-not (Test-Path "node_modules")) {
            Write-Host "  -> node_modules not found, running npm.cmd install..." -ForegroundColor Gray
            & npm.cmd install
            if ($LASTEXITCODE -ne 0) { throw "npm install failed" }
        }
        Write-Host "  -> Running npm.cmd run build..." -ForegroundColor Gray
        & npm.cmd run build
        if ($LASTEXITCODE -ne 0) { throw "Frontend build failed" }
    }
    finally {
        Pop-Location
    }
} else {
    Write-Host "[1/3] Skipping frontend build (-SkipUiBuild)" -ForegroundColor DarkGray
    if (-not (Test-Path "$RootDir\ui\dist\index.html")) {
        throw "Error: ui/dist/index.html not found. Please remove -SkipUiBuild to compile frontend assets."
    }
}

# 2. Compile Rust standalone binary
Write-Host "[2/3] Compiling Rust core engine and embedding UI into standalone binary (Release)..." -ForegroundColor Green
& cargo build --release -p my-computer-keeper
if ($LASTEXITCODE -ne 0) {
    throw "Cargo release build failed"
}

# 3. Output standalone executable
Write-Host "[3/3] Exporting standalone executable..." -ForegroundColor Green
$TargetExe = "$RootDir\target\release\my-computer-keeper.exe"
if (-not (Test-Path $TargetExe)) {
    throw "Build artifact does not exist: $TargetExe"
}

$DestDir = "$RootDir\$OutDir"
if (-not (Test-Path $DestDir)) {
    New-Item -ItemType Directory -Path $DestDir | Out-Null
}

$DestExe = "$DestDir\my-computer-keeper.exe"
Get-Process -Name "my-computer-keeper" -ErrorAction SilentlyContinue | ForEach-Object {
    Write-Host "  -> Closing running my-computer-keeper instance (PID $($_.Id))..." -ForegroundColor Yellow
    Stop-Process -Id $_.Id -Force -ErrorAction SilentlyContinue
}
Start-Sleep -Milliseconds 500
Copy-Item -Path $TargetExe -Destination $DestExe -Force

$ExeSize = (Get-Item $DestExe).Length / 1MB
$Sha256 = (Get-FileHash -Algorithm SHA256 -Path $DestExe).Hash

Write-Host "----------------------------------------------------------" -ForegroundColor Cyan
Write-Host " [SUCCESS] Standalone Executable built successfully!" -ForegroundColor Green
Write-Host " Output Path : $DestExe" -ForegroundColor White
Write-Host (" Binary Size : {0:N2} MB" -f $ExeSize) -ForegroundColor White
Write-Host " SHA-256     : $Sha256" -ForegroundColor Gray
Write-Host " Note        : 100% self-contained. Copy anywhere and double-click to run!" -ForegroundColor Yellow
Write-Host "----------------------------------------------------------" -ForegroundColor Cyan

if ($TestRun) {
    Write-Host "Running standalone self-verification test (Headless Mode)..." -ForegroundColor Cyan
    $TestPort = 14299
    $env:KEEPER_BIND_ADDR = "127.0.0.1:$TestPort"
    $Proc = Start-Process -FilePath $DestExe -ArgumentList "--headless" -PassThru -NoNewWindow
    Start-Sleep -Seconds 2
    try {
        $resp = Invoke-WebRequest -Uri "http://127.0.0.1:$TestPort/" -UseBasicParsing -TimeoutSec 5
        if ($resp.StatusCode -eq 200 -and $resp.Content -match 'id="root"') {
            Write-Host "  -> Embedded UI static hosting verified: PASS" -ForegroundColor Green
        } else {
            throw "Embedded UI verification failed"
        }
        $apiResp = Invoke-WebRequest -Uri "http://127.0.0.1:$TestPort/api/get_drives" -UseBasicParsing -TimeoutSec 5
        if ($apiResp.StatusCode -eq 200 -and $apiResp.Content -match '"ok":true') {
            Write-Host "  -> RPC API endpoint verified: PASS" -ForegroundColor Green
        } else {
            throw "RPC API verification failed"
        }
    }
    finally {
        Stop-Process -Id $Proc.Id -Force -ErrorAction SilentlyContinue
        Remove-Item Env:\KEEPER_BIND_ADDR -ErrorAction SilentlyContinue
    }
}
