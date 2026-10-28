[CmdletBinding()]
param(
    [Parameter(Mandatory=$true)][string]$ApkPath,
    [string]$Capabilities,
    [switch]$PackageOnly,
    [switch]$Require16KRuntime,
    [string]$Output = 'local-tests\builds\package-check.json',
    [string]$PythonExecutable = $env:BREW_PYTHON
)
$ErrorActionPreference = 'Stop'
if (-not $PythonExecutable) {
    $brewPythonCommand = Get-Command python.exe -ErrorAction SilentlyContinue
    if (-not $brewPythonCommand) { throw 'Set BREW_PYTHON to a Python 3 executable.' }
    $PythonExecutable = $brewPythonCommand.Source
}
. (Join-Path $PSScriptRoot 'environment.ps1')
Initialize-BrewEnvironment
$brewArguments = @('-B', (Join-Path $PSScriptRoot 'apk_checks.py'), '--apk', $ApkPath,
    '--aapt2', "$env:ANDROID_HOME\build-tools\37.0.0\aapt2.exe",
    '--zipalign', "$env:ANDROID_HOME\build-tools\37.0.0\zipalign.exe", '--output', $Output)
if ($Capabilities) { $brewArguments += @('--capabilities', $Capabilities) }
if ($PackageOnly) { $brewArguments += '--package-only' }
if ($Require16KRuntime) { $brewArguments += '--require-16kb-runtime' }
& $PythonExecutable @brewArguments
if ($LASTEXITCODE -ne 0) { throw 'APK verification failed; see the diagnostic above.' }
