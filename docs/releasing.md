# Releasing WhatSpace

The scripts in `packaging/` and the workflows in `.github/workflows/` are MailCraft's with
WhatSpace's names, and differ from them only where the program does (what it registers, its
description, its permissions, its own installer identity). They run on GitHub's standard runners
in any public repository, a fork included.

1. Bump the version: `cargo xtask version set 0.2.0` (the one source of truth is
   `[workspace.package] version`; the app and the CLI show it with `--version`), commit, and get
   `cargo xtask ci` green.
2. Tag that commit `v0.2.0` and push the tag. The Release workflow refuses a tag that isn't the
   version in `Cargo.toml`, then builds:

| Platform | Artifacts | Signing |
|---|---|---|
| Windows | x64, x86 and arm64 `.msi` + portable `.zip` | Azure Trusted Signing (`AZURE_*` secrets) or a certificate (`WINDOWS_CERTIFICATE*`) |
| macOS | universal `WhatSpace.app` in a `.dmg`, universal CLI zip | Developer ID + notarization (`APPLE_*` secrets) |
| Linux | x86_64 and aarch64 `.AppImage` (+ `.zsync`), `.flatpak` bundle, `.deb`, `.rpm`, `.tar.gz`; Flathub manifest | checksums |
| FreeBSD | x86_64 `.tar.gz` | checksums |

3. The run publishes ONE GitHub release, `WhatSpace v<version>` on that tag, with every file and a
   `SHA256SUMS.txt` (`packaging/release-notes.md` is its text). Running the same tag again
   replaces the files of that release. A version with a suffix (`0.2.0-rc.1`) is published as a
   pre-release.

Signing secrets are repository secrets; without them the builds are unsigned (ad-hoc on macOS) and
the scripts say so.

Dry run: `gh workflow run release.yml --ref <branch>` builds everything on any branch and keeps
the files as workflow artifacts without publishing anything (FreeBSD only when asked for: it is
slow). The workflow must be on the default branch before it can be started by hand.

## The checks that run without a release

| Workflow | When | What |
|---|---|---|
| CI | every pull request and push to main | `cargo xtask ci` on Windows, macOS and Linux |
| Packaging lint | pull requests touching packaging or workflows | actionlint, shellcheck, PowerShell parse, XML, desktop entry, AppStream, Flatpak manifests, MSI GUIDs (shared `craft-common-actions`) |
| FreeBSD | nightly, pushes to `release`, pull requests touching the FreeBSD build or `crates/fs` | build, test (as an ordinary user) and package in a FreeBSD 15 VM (shared) |
| Windows ARM64 | nightly, pushes to `release`, pull requests touching the Windows packaging | the ARM64 MSI installed, run and removed on a Windows 11 ARM64 runner (shared) |

## What the installers do

- **Windows (`.msi`, WiX 5):** per-machine install into Program Files, a Start Menu shortcut, an
  App Paths entry, and "Scan with WhatSpace" on folders, the inside of folders and drives in
  Explorer (under "Show more options" on Windows 11). Nothing else is registered, and all of it
  goes on uninstall. `whatspace.exe` is a Windows program with no console window;
  `whatspace-cli.exe` is the console program. The portable `.zip` holds the same two files and
  registers nothing.
- **macOS (`.dmg`):** `WhatSpace.app` (universal), hardened runtime, no sandbox (it has to read
  the whole disk; Full Disk Access is the owner's choice in System Settings).
- **Linux (`.deb`, `.rpm`, `.AppImage`, `.tar.gz`):** `/usr/bin/whatspace` and `whatspace-cli`, a
  desktop entry that offers WhatSpace for folders ("Open with"), AppStream metadata and the icons.
  The binaries need glibc 2.35 or later.
- **Flatpak:** host file system access (a disk analyzer has to see the disk; moving to the trash
  needs to write), no network.
- **Product identity:** the MSI's UpgradeCode and component GUIDs are WhatSpace's own. Never copy
  them from or into a sibling app (Windows would treat the two as one product).

## Building the packages on your own machine

Windows (needs the MSVC Rust toolchain, the Windows SDK and the .NET SDK):

```
dotnet tool install --global wix --version 5.0.2
wix extension add -g WixToolset.UI.wixext/5.0.2 WixToolset.Util.wixext/5.0.2
pwsh packaging/windows/package.ps1 -Arch x64
```

macOS: `bash packaging/macos/package.sh --arch universal`. Linux (with nfpm installed):
`bash packaging/linux/package.sh`. FreeBSD: `bash packaging/freebsd/package.sh`. Everything lands
in `dist/release/`.
