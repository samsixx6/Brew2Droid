# Upstream and source provenance

Brew2Droid is derived from Zeebx, developed by ZeebxTeam and its contributors:
https://github.com/ZeebxTeam/zeebx-emu

The original upstream README is preserved as `README_UPSTREAM.md`. Copyright and license notices in the source and vendored libraries are retained. `tools/toolchain-lock.json` records the original donor revision and local Dynarmic patch hashes. This export was made from checkout base `a048f56494e79c2cee21b0792d00b52fd8b7a614` plus the current Brew2Droid working changes on 2026-10-28; that commit alone does not describe this modified snapshot.

`SOURCE_MANIFEST.sha256` identifies the files in this prepared source archive. The manually maintained main README on GitHub is preserved locally and is outside this archive manifest. Android gameplay code, runtime assets and vendored implementation sources are copied from the tested working tree. The Cargo workspace and lockfile are narrowed to the core, Android and probe/test packages; their resolved Android and host-test dependency trees are unchanged. The unused desktop, iOS, Switch and Libretro frontends and six desktop branding files are omitted. Core integration tests are included; their translation test checks the retained shared and Android interfaces, and their scratch-directory setup creates its own parents for a fresh checkout. The public README, build/publishing documents and Git ignore rules are prepared for this export. Local reports, private tool installations, old Git metadata and upstream release automation are excluded.

The ARM64 preview APK identified in `RELEASE_INFO.json` was built from the corresponding gameplay source. Its wrapper is debug-signed. Publishing it as a development pre-release accurately describes its status; use a maintained private release-signing key for a later stable distribution, and retain that key for future updates. Signing keys do not belong in this repository.

The tiny `frontends/handset-probe/tests/fixtures/save-migration.zip` is a synthetic unit-test fixture. Its only payload is the text `synthetic test module, not executable`; it contains no game executable. The Gradle wrapper JAR and the vendored LLVM public GPG key are public build tooling, not private signing material.

Tests can generate temporary data locally. The original private test captures and actual games are not part of this export. The 16 proposed cover thumbnails and their source sheet are also excluded.

The 2026-10-28 refresh includes the Android button editor update: the canvas fills the area below the toolbar in both orientations, and the selected-key size controls appear as a translucent top overlay. The native libraries and runtime assets are byte-identical to the previous preview APK. The updated Android wrapper and instrumentation package compiled, and the four Java layout contract suites passed. Live phone visual verification of this editor update is pending.
