# Uninstall LidCommit Windows Service / Scheduled Task and Files
# Run this script as Administrator in PowerShell

param(
    [switch]$RemoveConfig = $false
)

$ServiceName = "LidCommit"
$ExeName = "lid-commit.exe"
$InstallPath = "$env:LOCALAPPDATA\Programs\LidCommit"
$ConfigDir = "$env:APPDATA\lid-commit"

Write-Host "==========================================" -ForegroundColor Cyan
Write-Host "    LidCommit Uninstaller for Windows    " -ForegroundColor Cyan
Write-Host "==========================================" -ForegroundColor Cyan

# 1. Stop and terminate any running daemon processes
Write-Host "`n1. Stopping running instances of $ExeName..." -ForegroundColor Yellow
Get-Process -Name "lid-commit" -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
Start-Sleep -Milliseconds 500

# 2. Remove Windows Service or Scheduled Task
Write-Host "`n2. Removing registered service / task..." -ForegroundColor Yellow
if (Get-Command nssm -ErrorAction SilentlyContinue) {
    nssm stop $ServiceName 2>$null
    nssm remove $ServiceName confirm 2>$null
    Write-Host "NSSM service '$ServiceName' removed." -ForegroundColor Green
}

if (Get-ScheduledTask -TaskName $ServiceName -ErrorAction SilentlyContinue) {
    Stop-ScheduledTask -TaskName $ServiceName -ErrorAction SilentlyContinue
    Unregister-ScheduledTask -TaskName $ServiceName -Confirm:$false -ErrorAction SilentlyContinue
    Write-Host "Scheduled Task '$ServiceName' unregistered." -ForegroundColor Green
}

# 3. Remove Startup shortcuts
$startupShortcut = "$env:APPDATA\Microsoft\Windows\Start Menu\Programs\Startup\LidCommit.lnk"
if (Test-Path $startupShortcut) {
    Remove-Item $startupShortcut -Force -ErrorAction SilentlyContinue
    Write-Host "Removed Startup shortcut." -ForegroundColor Green
}

# 4. Remove installation directory
Write-Host "`n3. Removing installation directory..." -ForegroundColor Yellow
if (Test-Path $InstallPath) {
    Remove-Item -Path $InstallPath -Recurse -Force -ErrorAction SilentlyContinue
    Write-Host "Removed application files from: $InstallPath" -ForegroundColor Green
}

# 5. Handle user configurations
if ($RemoveConfig -or (Test-Path $ConfigDir)) {
    if (-not $RemoveConfig) {
        $confirmation = Read-Host "`nDo you also want to delete your configuration, API keys, and logs from $ConfigDir? (y/N)"
        if ($confirmation -match '^(y|yes)$') {
            $RemoveConfig = $true
        }
    }

    if ($RemoveConfig) {
        Remove-Item -Path $ConfigDir -Recurse -Force -ErrorAction SilentlyContinue
        Write-Host "Removed user configuration and logs from: $ConfigDir" -ForegroundColor Green
    } else {
        Write-Host "User configurations and saved API keys were preserved in: $ConfigDir" -ForegroundColor Cyan
    }
}

Write-Host "`n✅ LidCommit has been successfully uninstalled from your system!" -ForegroundColor Green

