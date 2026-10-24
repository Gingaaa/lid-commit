# Install LidCommit as a Windows Service
# Run this script as Administrator in PowerShell

param(
    [string]$InstallPath = "$env:LOCALAPPDATA\LidCommit",
    [string]$ApiKey = ""
)

$ErrorActionPreference = "Stop"
$ServiceName = "LidCommit"
$ExeName = "lid-commit.exe"

Write-Host "🔒 LidCommit Windows Installer" -ForegroundColor Cyan
Write-Host "================================" -ForegroundColor Cyan

# 1. Check if running as administrator
$isAdmin = ([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
if (-not $isAdmin) {
    Write-Error "This script must be run as Administrator. Right-click PowerShell -> Run as Administrator."
    exit 1
}

# 2. Check Rust is installed
if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
    Write-Host "Rust not found. Installing Rust..." -ForegroundColor Yellow
    $rustupUri = "https://win.rustup.rs/x86_64"
    $rustupInstaller = "$env:TEMP\rustup-init.exe"
    Invoke-WebRequest -Uri $rustupUri -OutFile $rustupInstaller
    Start-Process -FilePath $rustupInstaller -ArgumentList "-y" -Wait
    $env:PATH += ";$env:USERPROFILE\.cargo\bin"
    Write-Host "Rust installed!" -ForegroundColor Green
}

# 3. Build the release binary
Write-Host "`nBuilding LidCommit (release)..." -ForegroundColor Yellow
$projectDir = Split-Path -Parent $PSScriptRoot
Push-Location $projectDir
cargo build --release
Pop-Location
Write-Host "Build complete!" -ForegroundColor Green

# 4. Create install directory and copy binary
Write-Host "`nInstalling to $InstallPath..." -ForegroundColor Yellow
New-Item -ItemType Directory -Force -Path $InstallPath | Out-Null
Copy-Item "$projectDir\target\release\$ExeName" "$InstallPath\$ExeName" -Force

# 5. Copy example config if no config exists
$configDir = "$env:APPDATA\lid-commit"
$configFile = "$configDir\config.toml"
if (-not (Test-Path $configFile)) {
    New-Item -ItemType Directory -Force -Path $configDir | Out-Null
    Copy-Item "$projectDir\config.example.toml" $configFile
    Write-Host "Config created at: $configFile" -ForegroundColor Green
}

# 6. Set API key in environment if provided
if ($ApiKey) {
    [System.Environment]::SetEnvironmentVariable("GEMINI_API_KEY", $ApiKey, "User")
    Write-Host "GEMINI_API_KEY set in user environment." -ForegroundColor Green
} else {
    Write-Host "`n⚠️  Don't forget to set your GEMINI_API_KEY:" -ForegroundColor Yellow
    Write-Host '   [System.Environment]::SetEnvironmentVariable("GEMINI_API_KEY", "your_key", "User")'
}

# 7. Register as a Windows service using NSSM (Non-Sucking Service Manager)
# Alternatively use Task Scheduler for non-admin users
Write-Host "`nRegistering as Windows service..." -ForegroundColor Yellow

# Check for NSSM
if (Get-Command nssm -ErrorAction SilentlyContinue) {
    nssm install $ServiceName "$InstallPath\$ExeName"
    nssm set $ServiceName AppStdout "$InstallPath\lid-commit.log"
    nssm set $ServiceName AppStderr "$InstallPath\lid-commit-error.log"
    nssm set $ServiceName Start SERVICE_AUTO_START
    nssm start $ServiceName
    Write-Host "Service '$ServiceName' installed and started via NSSM!" -ForegroundColor Green
} else {
    # Fallback: Use Task Scheduler to run at startup
    Write-Host "NSSM not found. Using Task Scheduler instead..." -ForegroundColor Yellow

    $action = New-ScheduledTaskAction -Execute "$InstallPath\$ExeName"
    $trigger = New-ScheduledTaskTrigger -AtLogOn
    $settings = New-ScheduledTaskSettingsSet -ExecutionTimeLimit 0 -RestartCount 3 -RestartInterval (New-TimeSpan -Minutes 1)
    $principal = New-ScheduledTaskPrincipal -UserId $env:USERNAME -LogonType Interactive -RunLevel Highest

    Register-ScheduledTask `
        -TaskName $ServiceName `
        -Action $action `
        -Trigger $trigger `
        -Settings $settings `
        -Principal $principal `
        -Description "LidCommit: Auto-commit code changes when laptop lid closes" `
        -Force | Out-Null

    Start-ScheduledTask -TaskName $ServiceName
    Write-Host "Scheduled Task '$ServiceName' created and started!" -ForegroundColor Green
}

Write-Host "`n✅ LidCommit installed successfully!" -ForegroundColor Green
Write-Host "   Binary: $InstallPath\$ExeName"
Write-Host "   Config:  $configFile"
Write-Host "   Logs:    $InstallPath\lid-commit.log"
Write-Host "`nTo uninstall: Run .\uninstall.ps1 as Administrator"
