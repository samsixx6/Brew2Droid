# O compilador GNU monta um caminho interno sem aspas; o alias evita espaços sem mover fontes.
function Initialize-BrewEnvironment {
    $brewProject = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..')).TrimEnd('\')
    $brewMappings = @(subst.exe)
    $brewMapping = $brewMappings | Where-Object { $_.StartsWith('Q:\: => ') }
    if ($brewMapping) {
        $brewExisting = $brewMapping.Substring(8).TrimEnd('\')
        if (-not $brewExisting.Equals($brewProject,[System.StringComparison]::OrdinalIgnoreCase)) {
            throw 'Q: belongs to another project; choose a free build alias before continuing.'
        }
    } else {
        subst.exe Q: $brewProject
        if ($LASTEXITCODE -ne 0) { throw 'Could not reserve Q: for this project.' }
    }
    $env:BREW_PROJECT_ROOT = 'Q:\'
    $env:RUSTUP_HOME = 'Q:\.tools\rustup'
    $env:RUSTUP_TOOLCHAIN = '1.99.1-x86_64-pc-windows-gnu'
    $env:CARGO_HOME = 'Q:\.tools\cargo'
    $env:JAVA_HOME = if ($env:BREW_JAVA_HOME) { $env:BREW_JAVA_HOME } else { 'C:\Program Files\Android\Android Studio\jbr' }
    $env:ANDROID_HOME = 'Q:\.tools\android-sdk'
    $env:ANDROID_SDK_ROOT = $env:ANDROID_HOME
    $env:ANDROID_NDK_HOME = 'Q:\.tools\android-sdk\ndk\28.2.13676358'
    $env:ANDROID_NDK_ROOT = $env:ANDROID_NDK_HOME
    $env:GRADLE_USER_HOME = 'Q:\.tools\gradle-user'
    $brewCompiler = 'Q:\.tools\winlibs\mingw64'
    $brewCmake = 'Q:\.tools\android-sdk\cmake\3.31.6\bin'
    $env:PATH = "$brewCompiler\bin;$brewCmake;Q:\.tools\cargo\bin;Q:\.tools\android-sdk\platform-tools;$env:JAVA_HOME\bin;$env:PATH"
    $env:GCC_EXEC_PREFIX = 'Q:/.tools/winlibs/mingw64/lib/gcc/'
    $env:LIBRARY_PATH = 'Q:/.tools/winlibs/mingw64/x86_64-w64-mingw32/lib;Q:/.tools/winlibs/mingw64/lib/gcc/x86_64-w64-mingw32/16.2.0'
    $env:CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER = "$brewCompiler\bin\gcc.exe"
    $env:CC = "$brewCompiler\bin\gcc.exe"
    $env:CXX = "$brewCompiler\bin\g++.exe"
    $env:CMAKE_GENERATOR = 'Ninja'
    $env:CMAKE_MAKE_PROGRAM = "$brewCmake\ninja.exe"
}
