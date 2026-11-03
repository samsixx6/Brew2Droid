# Upstream and source provenance

Brew2Droid is derived from [Zeebx](https://github.com/ZeebxTeam/zeebx-emu), developed by ZeebxTeam and its contributors. `README_UPSTREAM.md`, source copyright notices, `LICENSE`, `LICENSE-GPL3` and third-party notices are retained.

This snapshot contains the Brew2Droid working changes through 2026-11-03 on checkout base `a048f56494e79c2cee21b0792d00b52fd8b7a614`; that base commit alone does not describe this modified source. `SOURCE_MANIFEST.sha256` identifies the prepared files. The manually maintained main README is preserved outside the generated manifest, and the existing screenshots are retained.

The publishing workspace contains the shared core, Android frontend and probe/test frontend. Gameplay/native sources, Android Java/resources, control artwork, fonts and vendored implementation files are copied from the corresponding tested working tree. The Cargo workspace/lockfile are narrowed to those three packages; their Android and host-test dependency graphs are checked against the working source. The translation test is scoped to the retained interfaces, and older synthetic test fixtures create their own scratch parents for a fresh checkout. These export adjustments do not change the runtime implementation.

The current SMAF FM/VMA integration uses the upstream DSP files and license recorded in `third-party/smaf-fm/provenance.json`, with Brew2Droid's bounded wrapper. Bundled fonts have their provenance and license notices under `assets/fonts` and `assets/licenses`. The exported build helper explicitly copies the font-provenance JSON into the generated APK license assets, so a clean checkout includes the same notice as the tested package. `tools/toolchain-lock.json` pins the build versions and Dynarmic patch hashes.

Unrelated platform frontends, desktop branding, unused cover proposals, games/firmware, complete MIF resources, SoundFont banks, SDK/compiler installations, device dumps, private reports, signing keys, local Git history and upstream release workflows are not part of this source export. The original-MIF support table contains only verified identities and class/dependency facts; it does not bundle original MIF bytes or implement general protected-MIF decoding.

The only source ZIP fixture, `frontends/handset-probe/tests/fixtures/save-migration.zip`, contains synthetic text rather than a game executable. The Gradle wrapper JAR and vendored LLVM public GPG key are public build tooling. Some opt-in integration tests require your own original game packages; those packages are not included.

`RELEASE_INFO.json` identifies the corresponding optimized ARM64 development APK and validation scope. Its existing debug signing identity is retained. The installed application ID is `com.samsixx.brew2droid`; internal Java/JNI names remain `io.github.brew.handset` to preserve the native bridge. No physical-phone or complete-gameplay qualification is claimed for this snapshot.
