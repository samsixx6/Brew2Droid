[CmdletBinding()]
param()
$ErrorActionPreference='Stop'
. (Join-Path $PSScriptRoot 'environment.ps1')
Initialize-BrewEnvironment
Push-Location $env:BREW_PROJECT_ROOT
try {
    $brewClasses='local-tests/native-android-ui/java-policy'
    New-Item -ItemType Directory -Force -Path $brewClasses | Out-Null
    & "$env:JAVA_HOME/bin/javac.exe" -d $brewClasses 'frontends/android/apk/app/src/main/java/io/github/brew/handset/StorageAccess.java' 'frontends/android/tests/StorageAccessTest.java'
    if($LASTEXITCODE -ne 0){throw 'Storage policy test compilation failed'}
    & "$env:JAVA_HOME/bin/java.exe" -cp $brewClasses io.github.brew.handset.StorageAccessTest
    if($LASTEXITCODE -ne 0){throw 'Storage policy checks failed'}
} finally { Pop-Location }
