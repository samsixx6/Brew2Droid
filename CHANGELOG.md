# Brew2Droid changes

## Latest development snapshot — 2026-11-03

- Recognize 24 verified original MOD/MIF pairs without replacing the game's MIF files. Shared MIF layouts now use the same metadata selection for launching and dependency lookup. This recognizes specific verified file pairs; it is not a general MIF decoder. In startup-only tests, 16 showed normal startup screens, eight retained separate runtime issues, and the supplied Orcs & Elves variant remained unsupported.
- Correct shared REALLOC growth and allocation flags. Tracking logical allocation lengths prevents padding from overwriting newly zeroed bytes; this restores Tank Raid's mission/terrain loading. Preserve the metadata across save/restore while continuing to read older saves.
- Include the current BREW startup, graphics, widget, file/resource, text/encoding, audio and network compatibility changes and regression tests. Add the bounded SMAF FM/VMA DSP integration and licensed fallback fonts; no SoundFont bank, sample ROM or game data is bundled.
- Update the Android library's removable-storage folder selection and enlarge grid icons.

The Android identity remains `com.samsixx.brew2droid`, version 1.0/code 2. The corresponding optimized ARM64 APK, existing signing identity, 16 KiB alignment and native-library provenance were verified. The original-MIF update passed 916 targeted checks in the working tree, including 41 explicit original-package checks and 55 frontend contracts. Full-gameplay and physical-phone testing of this snapshot remain incomplete. Publication-copy verification is recorded in `RELEASE_INFO.json`.

## 1.0 - 2026-10-29

The Android app version is 1.0, with version code 2 and application ID `com.samsixx.brew2droid`. Earlier development builds with ID `io.github.brew.handset` are separate Android applications. The About screen now displays the installed app version instead of the upstream core version. This build includes the expanded button editor and translucent selected-key size panel.

The optimized native build and Android packaging passed. APK version metadata, signature and 16 KiB alignment were verified; the signing identity, native libraries and runtime assets are unchanged. Phone runtime testing of this package is pending.

## 0.1.0-dev1 - source snapshot, 2026-10-28

Android fork of Zeebx with a native game library, storage access flow, display profiles, cellphone keypad customization and mapping, and Zeebo on-screen/controller support.

The current snapshot includes shared compatibility work for BREW APIs, graphics, file access, RSA operations, and audio. Compatibility remains game-dependent.

The latest graphics change avoids expensive vertex-buffer invalidation between batches while preserving images at ring-buffer wrap. Raging Thunder 2 averaged approximately 30.3 FPS during the measured live race on a Galaxy S25 Ultra with sound active. This is a measurement for that game/device, not a general performance guarantee.

Core validation on 2026-10-27: 1,179 host tests passed, 0 failed, 5 existing tests ignored. Real OpenGL pixel contracts passed on the S25 in normal and fallback paths. APK identity, signature, ARM64 libraries and 16 KiB alignment were checked. A phone using 16 KiB memory pages was not part of that runtime test.

The proposed fallback covers are not included. Some game packages have no icons in their MIF files and show the generic library image unless separate artwork is provided.

The publishing source contains the Android frontend, shared emulator core and its tests. Unrelated platform frontends and six desktop branding files are omitted. The Android and host-test dependency trees are unchanged by the source cleanup.

The 2026-10-28 button editor update fills the available area below the toolbar in portrait and landscape. Selecting a key shows a translucent size panel at the top; tapping empty space hides it without resizing the canvas. Android packaging and the four Java layout contract suites passed. The native libraries and assets are unchanged; live phone visual verification of the editor is pending.

The package ID update also updates APK verification, pinned application metadata and device tools. The final launcher and Android test target match the new installed package; all 21 packaging-tool tests passed. The internal Java namespace and native bindings are retained.
