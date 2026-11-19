# Asset attribution

Every non-code asset in this repository (icons, images, example files) is listed here with its author, source and licence. `cargo xtask assets` (part of `cargo xtask ci`) fails if an asset file is missing from this table.

**Policy (mandatory):** WhatSpace contains **no iconography, images, themes or other assets of TreeSize, WinDirStat, Microsoft, Apple, Google or any other vendor**. Every asset is original work by WhatSpace contributors or third-party material under an open licence (OSI open source, public domain / CC0, or a Creative Commons licence that allows redistribution). Screenshots of other vendors' software are never committed.

Generated-in-code art is original and has no file to list: the folder, file and link icons and the share bars (`crates/ui-egui/src/rows.rs`), the treemap's cushion shading and the file-type colours (`crates/ui-egui/src/treemap_view.rs`, `crates/ui-egui/src/theme.rs`). The interface uses egui's built-in fonts, which come with the `egui` crate and are not files in this repository. The app icon files below are written by `cargo xtask icon` from the drawing in `xtask/src/icon.rs`.

| Asset | Author | Source | Licence | Notes |
|---|---|---|---|---|
| `assets/app-icon/README.md` | WhatSpace contributors | written for this project | MIT OR Apache-2.0 | What the app icon is and how it is made |
| `assets/app-icon/whatspace.svg` | WhatSpace contributors | original artwork, drawn in code by `cargo xtask icon` | MIT OR Apache-2.0 | App icon (SVG) |
| `assets/app-icon/whatspace-1024.png` | WhatSpace contributors | original artwork, drawn in code by `cargo xtask icon` | MIT OR Apache-2.0 | App icon |
| `assets/app-icon/whatspace-macos-512.png` | WhatSpace contributors | original artwork, drawn in code by `cargo xtask icon` | MIT OR Apache-2.0 | App icon, with the margin macOS icons keep |
| `assets/app-icon/whatspace.icns` | WhatSpace contributors | original artwork, drawn in code by `cargo xtask icon` | MIT OR Apache-2.0 | App icon (macOS) |
| `assets/app-icon/whatspace.ico` | WhatSpace contributors | original artwork, drawn in code by `cargo xtask icon` | MIT OR Apache-2.0 | App icon (Windows) |
| `assets/app-icon/hicolor/scalable/apps/io.github.billsecond.whatspace.svg` | WhatSpace contributors | original artwork, drawn in code by `cargo xtask icon` | MIT OR Apache-2.0 | App icon (Linux, scalable) |
| `assets/app-icon/hicolor/16x16/apps/io.github.billsecond.whatspace.png` | WhatSpace contributors | original artwork, drawn in code by `cargo xtask icon` | MIT OR Apache-2.0 | App icon (Linux) |
| `assets/app-icon/hicolor/24x24/apps/io.github.billsecond.whatspace.png` | WhatSpace contributors | original artwork, drawn in code by `cargo xtask icon` | MIT OR Apache-2.0 | App icon (Linux) |
| `assets/app-icon/hicolor/32x32/apps/io.github.billsecond.whatspace.png` | WhatSpace contributors | original artwork, drawn in code by `cargo xtask icon` | MIT OR Apache-2.0 | App icon (Linux) |
| `assets/app-icon/hicolor/48x48/apps/io.github.billsecond.whatspace.png` | WhatSpace contributors | original artwork, drawn in code by `cargo xtask icon` | MIT OR Apache-2.0 | App icon (Linux) |
| `assets/app-icon/hicolor/64x64/apps/io.github.billsecond.whatspace.png` | WhatSpace contributors | original artwork, drawn in code by `cargo xtask icon` | MIT OR Apache-2.0 | App icon (Linux) |
| `assets/app-icon/hicolor/128x128/apps/io.github.billsecond.whatspace.png` | WhatSpace contributors | original artwork, drawn in code by `cargo xtask icon` | MIT OR Apache-2.0 | App icon (Linux) |
| `assets/app-icon/hicolor/256x256/apps/io.github.billsecond.whatspace.png` | WhatSpace contributors | original artwork, drawn in code by `cargo xtask icon` | MIT OR Apache-2.0 | App icon (Linux; also the window icon) |
| `assets/app-icon/hicolor/512x512/apps/io.github.billsecond.whatspace.png` | WhatSpace contributors | original artwork, drawn in code by `cargo xtask icon` | MIT OR Apache-2.0 | App icon (Linux) |
| `docs/images/treemap.png` | WhatSpace contributors | screenshot of WhatSpace itself showing an invented demo disk, made with `crates/ui-egui/examples/ui_shot.rs` | MIT OR Apache-2.0 | README and AppStream: the folder tree and the treemap |
| `docs/images/details.png` | WhatSpace contributors | screenshot of WhatSpace itself showing an invented demo disk, made with `crates/ui-egui/examples/ui_shot.rs` | MIT OR Apache-2.0 | README: the folder tree and Details |
| `docs/images/extensions.png` | WhatSpace contributors | screenshot of WhatSpace itself showing an invented demo disk, made with `crates/ui-egui/examples/ui_shot.rs` | MIT OR Apache-2.0 | README: space by file type |
| `docs/images/largest-dark.png` | WhatSpace contributors | screenshot of WhatSpace itself showing an invented demo disk, made with `crates/ui-egui/examples/ui_shot.rs` | MIT OR Apache-2.0 | README and AppStream: the largest files, dark theme |
