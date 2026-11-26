#Requires -Version 5.1
<#
.SYNOPSIS
  Standalone Windows installer for caf (+ decaf.cmd wrapper).
.EXAMPLE
  .\scripts\install.ps1
  .\scripts\install.ps1 -Shell
#>
param(
    [string]$BinDir = "$env:USERPROFILE\.local\bin",
    [switch]$Shell
)

$ErrorActionPreference = "Stop"
$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$RepoDir = Split-Path -Parent $ScriptDir

Write-Host "==> Building caf (release)..."
cargo build --release --manifest-path (Join-Path $RepoDir "Cargo.toml")
if ($LASTEXITCODE -ne 0) { throw "cargo build failed" }

New-Item -ItemType Directory -Force -Path $BinDir | Out-Null
Copy-Item -Force (Join-Path $RepoDir "target\release\caf.exe") (Join-Path $BinDir "caf.exe")
Set-Content -Path (Join-Path $BinDir "decaf.cmd") -Value "@echo off`r`n`"%~dp0caf.exe`" off %*`r`n"

$userPath = [Environment]::GetEnvironmentVariable("Path", "User")
if ($userPath -notlike "*$BinDir*") {
    [Environment]::SetEnvironmentVariable("Path", "$BinDir;$userPath", "User")
    Write-Host "Added $BinDir to user PATH (open a new shell to pick it up)."
}

$ver = & (Join-Path $BinDir "caf.exe") --version
Write-Host "==> Installed $(Join-Path $BinDir 'caf.exe') + decaf.cmd ($ver)"

if ($Shell) {
    & (Join-Path $BinDir "caf.exe") shell install
}
