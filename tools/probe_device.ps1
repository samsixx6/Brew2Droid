[CmdletBinding()]
param(
    [Parameter(Mandatory=$true)][string]$Serial,
    [string]$ApkPath = 'local-tests\builds\brew-handset-dev-arm64.apk',
    [switch]$Install,
    [string]$PythonExecutable = $env:BREW_PYTHON
)
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'environment.ps1')
Initialize-BrewEnvironment
$brewAdb = "$env:ANDROID_HOME\platform-tools\adb.exe"
$brewPackage = 'io.github.brew.handset'
$brewDeviceState = & $brewAdb -s $Serial get-state
if ($LASTEXITCODE -ne 0 -or $brewDeviceState -ne 'device') { throw 'The selected device is not authorized.' }
$brewApk = (Resolve-Path -LiteralPath $ApkPath).Path
if ($Install) {
    & $brewAdb -s $Serial install -r $brewApk
    if ($LASTEXITCODE -ne 0) { throw 'Diagnostic APK installation failed.' }
}
# O hash do pacote instalado evita aceitar um relatório de outro APK com o mesmo ID.
$brewInstalled = & $brewAdb -s $Serial shell pm path $brewPackage
if ($LASTEXITCODE -ne 0 -or $brewInstalled -notmatch '^package:(.+base\.apk)\s*$') {
    throw 'The expected single-APK diagnostic app is not installed.'
}
$brewInstalledPath = $Matches[1]
$brewHashOutput = (& $brewAdb -s $Serial shell sha256sum $brewInstalledPath) -join "`n"
$brewApkHash = (Get-FileHash -Algorithm SHA256 -LiteralPath $brewApk).Hash.ToLowerInvariant()
if ($LASTEXITCODE -ne 0 -or -not $brewHashOutput.StartsWith($brewApkHash)) {
    throw 'The installed APK does not match the selected artifact.'
}
& $brewAdb -s $Serial shell am force-stop $brewPackage
& $brewAdb -s $Serial shell run-as $brewPackage rm -f files/capabilities.json
if ($LASTEXITCODE -ne 0) { throw 'The app must be the debuggable diagnostic build.' }
& $brewAdb -s $Serial shell am start -W -n "$brewPackage/io.github.brew.handset.GameActivity" --ez brew.probe_only true
if ($LASTEXITCODE -ne 0) { throw 'Android did not start the diagnostic activity.' }
$brewDestination = Join-Path $env:BREW_PROJECT_ROOT 'local-tests\devices'
New-Item -ItemType Directory -Path $brewDestination -Force | Out-Null
$brewReport = $null
$brewClock = [System.Diagnostics.Stopwatch]::StartNew()
while ($brewClock.Elapsed.TotalSeconds -lt 20) {
    $brewJson = (& $brewAdb -s $Serial shell run-as $brewPackage cat files/capabilities.json 2>$null) -join "`n"
    if ($LASTEXITCODE -eq 0) {
        try { $brewReport = $brewJson | ConvertFrom-Json; break } catch { }
    }
    Start-Sleep -Milliseconds 500
}
if (-not $brewReport) {
    & $brewAdb -s $Serial logcat -b crash -d -v threadtime |
        Set-Content -Encoding utf8 -LiteralPath (Join-Path $brewDestination 'probe-crash.log')
    throw 'No native report appeared. Keep the phone unlocked; inspect local-tests/devices/probe-crash.log if it crashed.'
}
$brewReportPath = Join-Path $brewDestination 'capabilities.json'
$brewJson | Set-Content -Encoding utf8 -LiteralPath $brewReportPath
$brewRecord = [ordered]@{
    schema_version = 1
    captured_at_utc = [DateTime]::UtcNow.ToString('o')
    apk_sha256 = $brewApkHash
    model = ((& $brewAdb -s $Serial shell getprop ro.product.model) -join '').Trim()
    android_api = ((& $brewAdb -s $Serial shell getprop ro.build.version.sdk) -join '').Trim()
    native_report = $brewReport
}
$brewRecord | ConvertTo-Json -Depth 10 | Set-Content -Encoding utf8 -LiteralPath (Join-Path $brewDestination 'probe-run.json')
$brewSafeSerial = $Serial -replace '[^A-Za-z0-9_-]', '_'
$brewRunId = [DateTime]::UtcNow.ToString('yyyyMMddTHHmmssfffZ')
$brewArchive = Join-Path $brewDestination "probes\$brewSafeSerial\$brewRunId"
New-Item -ItemType Directory -Path $brewArchive -Force | Out-Null
Copy-Item -LiteralPath $brewReportPath,(Join-Path $brewDestination 'probe-run.json') -Destination $brewArchive
& (Join-Path $PSScriptRoot 'check_package.ps1') -ApkPath $brewApk -Capabilities $brewReportPath -PythonExecutable $PythonExecutable
Write-Output "Native evidence archived: $brewArchive"
