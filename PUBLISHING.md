# Publishing Brew2Droid on GitHub

You do not need to compile the app to publish the source and the already tested preview APK. The source belongs in a repository; the installable APK belongs in its Releases section. GitHub Pages is not needed for an Android download.

## 1. Set up your account and GitHub Desktop

1. Create an account at https://github.com/ if you do not already have one.
2. Install the Windows version of GitHub Desktop from https://desktop.github.com/download/ .
3. Open it, sign in, and finish its Git name/email setup. You can select the GitHub-provided private email address when configuring your commit identity.

## 2. Add the prepared source

The prepared local repository tracks `https://github.com/samsixx6/Brew2Droid` on `main`. Its existing, manually written README is preserved.

1. In Desktop, use **File > Add local repository**.
2. Select the prepared source folder, then confirm the selection.
3. The source files are staged. In **Summary**, enter `Add Brew2Droid source code`, then commit to `main`.
4. Click **Push origin** to upload the source to the existing repository.
5. Refresh the repository page and check that the README and source folders appear together.

## 3. Add the APK as a development release

1. Open **Releases** on your repository page and choose **Draft a new release**.
2. Create the tag `v0.1.0-dev1` on `main`.
3. Use `Brew2Droid 0.1.0-dev1 — Development Preview` as the title. Copy the supplied release notes into the description.
4. Attach the prepared APK and checksum file. You may also attach the prepared source ZIP; GitHub additionally creates source archives from the tagged commit.
5. Select **This is a pre-release**, because this build is an early development APK with a debug-signed wrapper.
6. Review the files, then choose **Publish release**. Use the save-draft option if you want to finish later.

Share `https://github.com/samsixx6/Brew2Droid/releases` so people can find the APK. Upload the individual release files, rather than an archive containing your whole development workspace.

## 4. What this prepared source contains

The Android frontend, shared emulator core, core/probe tests, actual in-game control artwork and branding, Cargo.lock, patched Dynarmic sources, Gradle wrapper, licenses, attribution and build instructions are included. Unrelated platform frontends and desktop branding are omitted. The original Zeebx README is retained separately for credits and history.

Actual games/firmware, SoundFont banks, private tool installations, device captures, personal development history, signing keys, and the proposed 16 game-cover thumbnails are excluded. The tiny ZIP used by an automated migration test contains only synthetic text. Upstream release workflows are not enabled in this copy; the first release is manual.

Keep the GPL license files, third-party notices and upstream credits. Publish the source corresponding to each downloadable APK. An optional source ZIP and `SOURCE_MANIFEST.sha256` have been prepared for this first snapshot.

## 5. Later updates

Keep using this repository after the first publication. Refresh the clean source copy with your new changes while preserving its `.git` folder, commit them in Desktop, and push them. Build and test the next APK, update Android's version values, then make a release with a new version tag. Keep private tools, game files and signing material outside the upload tree.

The current upload folder is a snapshot. Changes made in a separate development folder do not automatically appear here. The provided development APK has its existing signing identity; a future stable signing-key change may require testers to reinstall rather than upgrade.

## Official references

- https://docs.github.com/en/desktop/adding-and-cloning-repositories/adding-an-existing-project-to-github-using-github-desktop
- https://docs.github.com/en/repositories/releasing-projects-on-github/managing-releases-in-a-repository
