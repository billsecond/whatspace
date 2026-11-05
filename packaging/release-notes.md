**Early release.** The scanner, the folder tree, the treemap and the statistics are in place and tested on Windows, macOS, Linux and FreeBSD; what is planned next is in [ROADMAP.md](https://github.com/GITHUB_REPOSITORY/blob/main/ROADMAP.md).

WhatSpace for Windows, macOS, Linux and FreeBSD. Every file is listed with its SHA-256 in `SHA256SUMS.txt`.

| Platform | File | |
|---|---|---|
| Windows 10/11 (x64, x86, ARM64) | `whatspace-<ver>-windows-<arch>.msi` | Installer for all users: Program Files\WhatSpace, the Start menu, and "Scan with WhatSpace" on folders and drives in Explorer; uninstall from Installed apps. `whatspace-cli.exe` is installed next to the app. |
| | `whatspace-<ver>-windows-<arch>-portable.zip` | Portable: unzip anywhere and run `whatspace.exe`; includes `whatspace-cli.exe`. Registers nothing. |
| macOS 11+ (Apple silicon and Intel) | `whatspace-<ver>-macos-universal.dmg` | Drag WhatSpace to Applications. To scan everything, give it Full Disk Access in System Settings › Privacy & Security. |
| | `whatspace-cli-<ver>-macos-universal.zip` | The command-line tool alone. |
| Linux x86_64 and aarch64 (glibc 2.35+) | `whatspace-<ver>-linux-<arch>.AppImage` | `chmod +x` and run (if it does not start, add `--appimage-extract-and-run`). The `.zsync` next to it enables delta updates. |
| | `whatspace-<ver>-linux-<arch>.deb` / `.rpm` | `sudo apt install ./whatspace-*.deb` or `sudo dnf install ./whatspace-*.rpm`. |
| | `whatspace-<ver>-linux-<arch>.flatpak` | `flatpak install --user whatspace-*.flatpak` (the freedesktop runtime comes from Flathub). |
| | `whatspace-<ver>-linux-<arch>.tar.gz` | `bin/whatspace`, `bin/whatspace-cli`, the desktop entry, metainfo and icons under `share/`. |
| FreeBSD 15 (x86_64) | `whatspace-<ver>-freebsd-x86_64.tar.gz` | The two programs, the desktop entry and the icons. |

WhatSpace reads file metadata on your computer and sends nothing anywhere. It never deletes anything: Move to Recycle Bin / Trash asks first, and what it moves can be put back.

The builds are not signed with a publisher certificate yet: Windows SmartScreen and macOS Gatekeeper will ask before running them.
