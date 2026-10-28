[CmdletBinding()]
param(
    [Parameter(Mandatory=$true)][string]$Serial,
    [Parameter(Mandatory=$true)][string]$Source,
    [Parameter(Mandatory=$true)][string]$PythonExecutable
)
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'environment.ps1')
Initialize-BrewEnvironment
$brewAdb = "$env:ANDROID_HOME\platform-tools\adb.exe"
$brewPackage = 'io.github.brew.handset'
$brewBundle = Join-Path $env:BREW_PROJECT_ROOT ('local-tests\imports\' + [DateTime]::UtcNow.ToString('yyyyMMddTHHmmssfffZ'))
& $PythonExecutable -B (Join-Path $PSScriptRoot 'handset_fixtures.py') --source $Source --destination $brewBundle
if ($LASTEXITCODE -ne 0) { throw 'Could not prepare the private fixtures.' }
& $brewAdb -s $Serial shell am force-stop $brewPackage
& $brewAdb -s $Serial shell run-as $brewPackage mkdir -p files/roms/handset-games
if ($LASTEXITCODE -ne 0) { throw 'The debuggable handset app is not available.' }
# PowerShell não transporta bytes de tar por um pipeline de texto.
$brewStart = [System.Diagnostics.ProcessStartInfo]::new($brewAdb)
$brewStart.UseShellExecute = $false
$brewStart.CreateNoWindow = $true
$brewStart.RedirectStandardInput = $true
$brewStart.RedirectStandardError = $true
foreach ($brewArg in @('-s',$Serial,'exec-in','run-as',$brewPackage,'tar','-xf','-','-C','files/roms/handset-games')) {
    $brewStart.ArgumentList.Add($brewArg)
}
$brewProcess = [System.Diagnostics.Process]::Start($brewStart)
$brewArchive = [System.IO.File]::OpenRead((Join-Path $brewBundle 'handset-games.tar'))
try { $brewArchive.CopyTo($brewProcess.StandardInput.BaseStream) }
finally { $brewArchive.Dispose(); $brewProcess.StandardInput.Close() }
$brewError = $brewProcess.StandardError.ReadToEnd()
$brewProcess.WaitForExit()
if ($brewProcess.ExitCode -ne 0) { throw "Private import failed: $brewError" }
$brewManifest = Get-Content -Raw (Join-Path $brewBundle 'manifest.json') | ConvertFrom-Json
foreach ($brewEntry in $brewManifest.PSObject.Properties) {
    $brewHash = (& $brewAdb -s $Serial shell run-as $brewPackage sha256sum "files/roms/handset-games/$($brewEntry.Name)") -join "`n"
    if ($LASTEXITCODE -ne 0 -or -not $brewHash.StartsWith($brewEntry.Value)) {
        throw "Private device copy differs: $($brewEntry.Name)"
    }
}
Write-Output "Verified $(@($brewManifest.PSObject.Properties).Count) app-private files; source collection was read-only."
Write-Output "Import manifest: $brewBundle\manifest.json"
