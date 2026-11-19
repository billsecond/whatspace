# WhatSpace app icon

A **treemap on a green tile**: five rounded blocks the way WhatSpace's own treemap divides a
folder, the largest in amber (the folder taking the most space) and the rest in white and pale
greens. Original artwork, drawn in code by `xtask/src/icon.rs`; nothing in it is traced or copied
from any other product's icon.

## Palette

| Colour | Hex | Used for |
|---|---|---|
| Tile, top | `#22b07d` | the gradient field |
| Tile, bottom | `#0e7a5a` | the gradient field |
| Amber | `#ffc44d` | the largest block |
| White and pale greens | `#ffffff`, `#e3f6ee`, `#c6eedf`, `#a8e4cf` | the other blocks |

## Geometry

A unit square with a rounded-square tile (corner radius 0.225, 230 of 1024). The blocks fill
0.17 to 0.83 each way with gaps of 0.022 and corners of 0.035, over a soft shadow. macOS icons
sit on Apple's 824/1024 grid (`whatspace-macos-512.png`, `whatspace.icns`); the others fill the
canvas.

## Files

`cargo xtask icon` (or `packaging/icons.sh`) writes every file here; they are committed, so builds
never need to run it. Change the drawing in `xtask/src/icon.rs`, run it, and commit what changed.
Every file is listed in `ATTRIBUTION.md`.

| File | For |
|---|---|
| `whatspace.svg`, `hicolor/scalable/apps/io.github.billsecond.whatspace.svg` | the drawing as SVG; Linux's scalable icon |
| `hicolor/<n>x<n>/apps/io.github.billsecond.whatspace.png` | Linux desktop icons, 16 to 512 px; the window icon (256 px) |
| `whatspace-1024.png` | the full-size picture |
| `whatspace-macos-512.png` | macOS, with the margin macOS icons keep |
| `whatspace.ico` | Windows: the executable, the installer, Installed apps |
| `whatspace.icns` | macOS: the app bundle |
