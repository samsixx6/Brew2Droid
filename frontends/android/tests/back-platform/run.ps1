$ErrorActionPreference='Stop'
$brewRoot=(Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '../../../..')).Path
Push-Location $brewRoot
try {
    . .\tools\environment.ps1
    Initialize-BrewEnvironment
    $brewBack='local-tests/back-menu-host'
    New-Item -ItemType Directory -Force $brewBack | Out-Null
    & rustc --edition=2024 --crate-type cdylib tests/support/back_jni_host.rs -o "$brewBack/zeebx_android.dll"
    if($LASTEXITCODE -ne 0){throw 'Host JNI build failed'}
    $brewSources=@(Get-ChildItem -LiteralPath $PSScriptRoot -Recurse -Filter '*.java' | ForEach-Object FullName)
    $brewSources += (Resolve-Path frontends/android/apk/app/src/main/java/io/github/brew/handset/GameActivity.java).Path
    foreach($brewInput in @("AndroidMappingInput","ButtonBindings","MappingInputState")){$brewSources += (Resolve-Path "frontends/android/apk/app/src/main/java/io/github/brew/handset/$brewInput.java").Path}
    & "$env:JAVA_HOME\bin\javac.exe" -encoding UTF-8 -d "$brewBack/java" @brewSources
    if($LASTEXITCODE -ne 0){throw 'Host Activity compilation failed'}
    foreach($brewApi in @(26,33,35,36,37)){
        & "$env:JAVA_HOME\bin\java.exe" "-Djava.library.path=$((Resolve-Path $brewBack).Path)" "-Dtest.api=$brewApi" -cp "$brewBack/java" io.github.brew.handset.BackNavigationTest
        if($LASTEXITCODE -ne 0){throw "Back routing failed with modeled API $brewApi"}
    }
    & "$env:JAVA_HOME\bin\java.exe" "-Djava.library.path=$((Resolve-Path $brewBack).Path)" -cp "$brewBack/java" io.github.brew.handset.MappingCaptureBoundaryTest
    if($LASTEXITCODE -ne 0){throw 'Mapping capture Activity boundary failed'}
    & "$env:JAVA_HOME\bin\java.exe" "-Djava.library.path=$((Resolve-Path $brewBack).Path)" -cp "$brewBack/java" io.github.brew.handset.KeypadVibrationTest
    if($LASTEXITCODE -ne 0){throw 'Vibration service boundary failed'}
    & "$env:JAVA_HOME\bin\java.exe" "-Djava.library.path=$((Resolve-Path $brewBack).Path)" -cp "$brewBack/java" io.github.brew.handset.GameMenuBoundaryTest
    if($LASTEXITCODE -ne 0){throw 'Native menu JNI boundary failed'}
} finally {Pop-Location}
