# Building Brew2Droid

This document describes the Android ARM64 development build. The exact versions and Dynarmic patch hashes are recorded in `tools/toolchain-lock.json`. The complete Rust/C/C++ sources and Gradle wrapper are included; SDKs, compiler installations and cached build outputs are not.

This publishing workspace contains the emulator core (`zeebx`), Android frontend (`zeebx-android`) and test/probe frontend (`brew-probe`). Unrelated desktop, iOS, Switch and Libretro frontends and their branding assets are omitted. Shared controller artwork, translations and the small audio test fixture remain because the core or its tests compile them in.

## Tool versions

| Tool | Version used for the tested APK |
|---|---|
| Rust | 1.99.1 |
| Android minimum / compile / target API | 26 / 37 / 37 |
| Android NDK | 28.2.13676358 |
| Android build tools | 37.0.0 |
| Android CMake | 3.31.6, with Ninja |
| JDK | 17 |
| Gradle wrapper | 9.6.0 |
| Android Gradle plugin | 9.4.0 |
| ABI / Rust target | arm64-v8a / aarch64-linux-android |

Install Rust from https://rustup.rs/ and Android command-line tools or Android Studio from https://developer.android.com/studio . Install the listed SDK, NDK, build-tools and CMake packages using Android's SDK Manager. Review and accept the SDK licenses yourself. The SDK package name recorded for this snapshot is `platforms;android-37.0`.

## Linux build recipe

Install JDK 17 and set `JAVA_HOME`. Set `ANDROID_HOME` to your SDK directory. Start in the repository root, preferably in a path without spaces. No Qt installation is required for the Android package.

```bash
rustup toolchain install 1.99.1 --profile minimal
rustup target add aarch64-linux-android --toolchain 1.99.1

export ANDROID_NDK_HOME="$ANDROID_HOME/ndk/28.2.13676358"
export ANDROID_NDK_ROOT="$ANDROID_NDK_HOME"
export ZEEBX_ANDROID_ABI=arm64-v8a
export ZEEBX_ANDROID_PLATFORM=android-26
export CMAKE_TOOLCHAIN_FILE="$PWD/frontends/android/ndk-toolchain.cmake"
export CMAKE_MAKE_PROGRAM="$ANDROID_HOME/cmake/3.31.6/bin/ninja"
export PATH="$ANDROID_HOME/cmake/3.31.6/bin:$PATH"
export CC_aarch64_linux_android="$ANDROID_NDK_HOME/toolchains/llvm/prebuilt/linux-x86_64/bin/aarch64-linux-android26-clang"
export CXX_aarch64_linux_android="$ANDROID_NDK_HOME/toolchains/llvm/prebuilt/linux-x86_64/bin/aarch64-linux-android26-clang++"
export AR_aarch64_linux_android="$ANDROID_NDK_HOME/toolchains/llvm/prebuilt/linux-x86_64/bin/llvm-ar"
export CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER="$CC_aarch64_linux_android"
export RUSTFLAGS='-C link-arg=-Wl,-z,max-page-size=16384'

cargo +1.99.1 build --locked --release --target aarch64-linux-android -p zeebx-android

apkdir=frontends/android/apk
jnidir="$apkdir/app/src/main/jniLibs/arm64-v8a"
licensedir="$apkdir/app/src/main/assets/licenses"
mkdir -p "$jnidir" "$licensedir"
cp target/aarch64-linux-android/release/libzeebx_android.so "$jnidir/"
cp "$ANDROID_NDK_HOME/toolchains/llvm/prebuilt/linux-x86_64/sysroot/usr/lib/aarch64-linux-android/libc++_shared.so" "$jnidir/"
cp LICENSE LICENSE-GPL3 THIRD-PARTY-NOTICES.txt "$licensedir/"
bash "$apkdir/gradlew" -p "$apkdir" --no-daemon assembleDebug
```

If you set `CARGO_TARGET_DIR`, use that directory instead of `target/` in the library-copy command. The resulting APK is `frontends/android/apk/app/build/outputs/apk/debug/app-debug.apk`.

The commands mirror the tested native build flags and packaging steps. The supplied Windows maintainer script, `tools/build_android.ps1`, was used for the tested APK; this exported Linux recipe has not been run in a clean Linux environment. The older upstream `compilar.sh` and Android README are retained for provenance, but their default NDK/API and Gradle instructions predate this snapshot; use the versions and commands above for this Android fork.

## Existing Windows maintainer setup

`tools/build_android.ps1 -Configuration Release` builds optimized native code and a debug-signed APK. It uses `tools/environment.ps1`, an isolated `.tools` installation and a `Q:` alias to avoid paths with spaces. A fresh clone needs those tools installed; they are deliberately excluded from Git. The lockfile lists the exact tool versions. Do not replace an existing `Q:` mapping for another project. This script is retained as the exact build procedure used by the maintainer, rather than a one-click SDK installer.

## Tests

With a C/C++ host compiler, CMake and Ninja available:

```bash
cargo +1.99.1 test --locked --release --no-default-features --features gl -p zeebx -p brew-probe --jobs 4
```

The corresponding Windows working tree and this Android-focused publishing copy each passed 1,179 tests, with 0 failures and 5 existing ignored tests, on 2026-10-27. The publishing-copy verification disabled release LTO and used 16 codegen units to shorten test linking; the APK build profile was unchanged. Private game collections and captured states are not required for this automated suite. Live game benchmarks need your own files.

## Signing and releases

The current installable APK is a development preview. Before a stable release, configure release signing with your own protected key and local credentials. Keep the same release key for subsequent app updates and increment `versionCode` and `versionName` in `frontends/android/apk/app/build.gradle.kts`. Preserve the package ID if you intend an upgrade of the same app. Do not upload signing keys or passwords.

Distribute the corresponding source and included license notices alongside your APK release. See `PUBLISHING.md` for the manual GitHub workflow.
