# Windows packaging: build the release binary and produce
#   target/package/postillion-<version>-windows-<arch>.zip
# containing postillion.exe and an Install.ps1 that drops it into
# %LOCALAPPDATA%\Programs\Postillion with a Start Menu shortcut.
#
# Usage: pwsh -File scripts/package-windows.ps1
# Env:   PROFILE=debug for a fast unoptimized package (CI smoke); default release.
#
# The Start Menu shortcut is not cosmetic: Windows only shows toast
# notifications for an application with a registered AppUserModelID, and a
# Start Menu .lnk is what registers one. Without it crates/ui/src/notify.rs
# falls back to PowerShell's own AUMID.

$ErrorActionPreference = 'Stop'

$Root = Split-Path -Parent (Split-Path -Parent $MyInvocation.MyCommand.Path)
# `$Profile` is a PowerShell automatic variable — don't shadow it.
$BuildProfile = if ($env:PROFILE) { $env:PROFILE } else { 'release' }
$Arch = if ($env:PROCESSOR_ARCHITECTURE -eq 'ARM64') { 'aarch64' } else { 'x86_64' }
$Version = (Select-String -Path (Join-Path $Root 'Cargo.toml') -Pattern '^version' |
    Select-Object -First 1).Line -replace '.*"(.*)".*', '$1'

$OutDir = Join-Path $Root 'target\package'
$Stage = Join-Path $OutDir "postillion-$Version-windows-$Arch"
$Zip = "$Stage.zip"

Push-Location $Root
try {
    if ($BuildProfile -eq 'release') {
        cargo build --release -p postillion
        $Bin = Join-Path $Root 'target\release\postillion.exe'
    } else {
        cargo build -p postillion
        $Bin = Join-Path $Root 'target\debug\postillion.exe'
    }
    if ($LASTEXITCODE -ne 0) { throw "cargo build failed ($LASTEXITCODE)" }

    if (Test-Path $Stage) { Remove-Item -Recurse -Force $Stage }
    if (Test-Path $Zip) { Remove-Item -Force $Zip }
    New-Item -ItemType Directory -Force -Path $Stage | Out-Null
    Copy-Item $Bin (Join-Path $Stage 'postillion.exe')
    Copy-Item (Join-Path $Root 'apps\postillion\windows\postillion.ico') $Stage
    Copy-Item (Join-Path $Root 'LICENSE') $Stage

    $installer = @'
# Install Postillion for the current user (no admin rights needed).
$ErrorActionPreference = 'Stop'
$Here = Split-Path -Parent $MyInvocation.MyCommand.Path
$Target = Join-Path $env:LOCALAPPDATA 'Programs\Postillion'

# A running instance holds engine.lock and its own .exe open — stop it first,
# otherwise the copy below fails with a sharing violation.
Get-Process postillion -ErrorAction SilentlyContinue | ForEach-Object {
    $_.CloseMainWindow() | Out-Null
    if (-not $_.WaitForExit(5000)) { $_.Kill() }
}

New-Item -ItemType Directory -Force -Path $Target | Out-Null
Copy-Item (Join-Path $Here 'postillion.exe') $Target -Force
Copy-Item (Join-Path $Here 'postillion.ico') $Target -Force

$StartMenu = Join-Path $env:APPDATA 'Microsoft\Windows\Start Menu\Programs'
$Shortcut = (New-Object -ComObject WScript.Shell).CreateShortcut(
    (Join-Path $StartMenu 'Postillion.lnk'))
$Shortcut.TargetPath = Join-Path $Target 'postillion.exe'
$Shortcut.WorkingDirectory = $Target
$Shortcut.IconLocation = Join-Path $Target 'postillion.ico'
$Shortcut.Description = 'Multi-device controller for coding agents'
$Shortcut.Save()

# Add the install dir to the user PATH so `postillion status` works from a
# terminal. Read the raw user value, not $env:PATH — that one is the merged
# machine+user string and writing it back would copy machine entries into the
# user scope.
$UserPath = [Environment]::GetEnvironmentVariable('Path', 'User')
if ($UserPath -notlike "*$Target*") {
    $Joined = if ([string]::IsNullOrEmpty($UserPath)) { $Target } else { "$UserPath;$Target" }
    [Environment]::SetEnvironmentVariable('Path', $Joined, 'User')
    Write-Host "Added to PATH (new terminals only): $Target"
}

Write-Host "Installed: $(Join-Path $Target 'postillion.exe')"
'@
    Set-Content -Path (Join-Path $Stage 'Install.ps1') -Value $installer -Encoding UTF8

    Compress-Archive -Path $Stage -DestinationPath $Zip
    Remove-Item -Recurse -Force $Stage
    Write-Host "packaged: $Zip"
} finally {
    Pop-Location
}
