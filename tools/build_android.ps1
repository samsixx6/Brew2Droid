[CmdletBinding()]
param([ValidateSet('Debug','Release')][string]$Configuration = 'Release')

$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'environment.ps1')
Initialize-BrewEnvironment
$brewRoot = $env:BREW_PROJECT_ROOT
$brewLock = Get-Content -Raw -LiteralPath (Join-Path $PSScriptRoot 'toolchain-lock.json') | ConvertFrom-Json
Push-Location $brewRoot
try {
    foreach ($brewPatch in $brewLock.dynarmic.local_patch_files.PSObject.Properties) {
        $brewPatchHash = (Get-FileHash -Algorithm SHA256 -LiteralPath $brewPatch.Name).Hash
        if ($brewPatchHash -ne $brewPatch.Value) { throw "Pinned Dynarmic patch differs: $($brewPatch.Name)" }
    }
    $brewRust = & cargo --version
    if ($LASTEXITCODE -ne 0 -or $brewRust -notlike "cargo $($brewLock.rust) *") {
        throw 'Install the exact project Rust toolchain before building.'
    }
    $brewRustHost = (& rustc -vV) -join "`n"
    if ($LASTEXITCODE -ne 0 -or $brewRustHost -notmatch 'host: x86_64-pc-windows-gnu') {
        throw 'The Windows build requires the pinned x86_64-pc-windows-gnu Rust host.'
    }
    $brewTargets = & rustup target list --installed --toolchain $env:RUSTUP_TOOLCHAIN
    if ($LASTEXITCODE -ne 0 -or $brewTargets -notcontains 'aarch64-linux-android') {
        throw 'Install the aarch64-linux-android target for the pinned GNU toolchain.'
    }
    $brewJava = (& "$env:JAVA_HOME\bin\java.exe" -version 2>&1) -join "`n"
    if ($LASTEXITCODE -ne 0 -or $brewJava -notmatch 'version "17\.') { throw 'JDK 17 is required.' }
    $brewNdkProperties = Get-Content -Raw -LiteralPath "$env:ANDROID_NDK_HOME\source.properties"
    if ($brewNdkProperties -notmatch "Pkg.Revision\s*=\s*$([regex]::Escape($brewLock.android.ndk))") {
        throw 'The installed NDK does not match tools/toolchain-lock.json.'
    }
    $brewNdkBin = "$env:ANDROID_NDK_HOME\toolchains\llvm\prebuilt\windows-x86_64\bin"
    $env:CARGO_TARGET_DIR = 'Q:\.tools\target-android'
    $env:CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER = "$brewNdkBin\aarch64-linux-android26-clang.cmd"
    $env:CC_aarch64_linux_android = "$brewNdkBin\aarch64-linux-android26-clang.cmd"
    $env:CXX_aarch64_linux_android = "$brewNdkBin\aarch64-linux-android26-clang++.cmd"
    $env:AR_aarch64_linux_android = "$brewNdkBin\llvm-ar.exe"
    $env:CMAKE_TOOLCHAIN_FILE = 'Q:\frontends\android\ndk-toolchain.cmake'
    $env:ZEEBX_ANDROID_ABI = 'arm64-v8a'
    $env:ZEEBX_ANDROID_PLATFORM = 'android-26'
    $env:RUSTFLAGS = '-C link-arg=-Wl,-z,max-page-size=16384'
    $brewCargoArgs = @('build','--locked','--target','aarch64-linux-android','-p','zeebx-android')
    $brewProfile = 'debug'
    if ($Configuration -eq 'Release') { $brewCargoArgs += '--release'; $brewProfile = 'release' }
    & cargo @brewCargoArgs
    if ($LASTEXITCODE -ne 0) { throw 'Android native build failed.' }

    $brewJni = 'frontends\android\apk\app\src\main\jniLibs\arm64-v8a'
    New-Item -ItemType Directory -Path $brewJni -Force | Out-Null
    Copy-Item -LiteralPath "$env:CARGO_TARGET_DIR\aarch64-linux-android\$brewProfile\libzeebx_android.so" -Destination $brewJni
    Copy-Item -LiteralPath "$env:ANDROID_NDK_HOME\toolchains\llvm\prebuilt\windows-x86_64\sysroot\usr\lib\aarch64-linux-android\libc++_shared.so" -Destination $brewJni
    $brewLicenses = 'frontends\android\apk\app\src\main\assets\licenses'
    New-Item -ItemType Directory -Path $brewLicenses -Force | Out-Null
    Copy-Item -LiteralPath 'LICENSE','LICENSE-GPL3','THIRD-PARTY-NOTICES.txt' -Destination $brewLicenses

    # O Rust Release é otimizado; o invólucro APK continua de diagnóstico para exportar o relatório via run-as.
    & '.\frontends\android\apk\gradlew.bat' -p 'frontends\android\apk' --no-daemon assembleDebug
    if ($LASTEXITCODE -ne 0) { throw 'Android packaging failed.' }
    $brewOutput = 'local-tests\builds\brew-handset-dev-arm64.apk'
    New-Item -ItemType Directory -Path (Split-Path $brewOutput) -Force | Out-Null
    Copy-Item -LiteralPath 'frontends\android\apk\app\build\outputs\apk\debug\app-debug.apk' -Destination $brewOutput
    Write-Output "Diagnostic APK ($Configuration native code): $brewRoot$brewOutput"
    Get-FileHash -Algorithm SHA256 -LiteralPath $brewOutput
} finally {
    Pop-Location
}
