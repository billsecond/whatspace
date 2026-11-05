//! `cargo xtask icon`: draw WhatSpace's app icon and write it in every size the platforms want.
//!
//! The icon is original artwork drawn here, in code, and nothing else: a rounded tile in teal
//! green holding a small treemap, its largest block in amber (the folder taking the most space)
//! and the rest in white and pale tints. Nothing is traced or copied from any other product.
//!
//! Output (all listed in ATTRIBUTION.md): `assets/app-icon/whatspace.svg`, `whatspace-1024.png`,
//! `whatspace-macos-512.png` (with the margin macOS icons keep), `whatspace.ico`,
//! `whatspace.icns` and the `hicolor` tree for Linux (PNGs and the scalable SVG).

use std::path::Path;

/// Samples per pixel edge (4×4 supersampling).
const SS: u32 = 4;

/// The reverse-DNS name the Linux icon files carry.
const APP_ID: &str = "io.github.billsecond.whatspace";

/// macOS icons sit on Apple's grid: an 824 px body on a 1024 px canvas.
const MAC_SCALE: f32 = 824.0 / 1024.0;

type Rgba = [f32; 4];

fn mix(a: Rgba, b: Rgba, t: f32) -> Rgba {
    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t, a[3] + (b[3] - a[3]) * t]
}

/// `top` over `under` (straight alpha).
fn over(under: Rgba, top: Rgba) -> Rgba {
    let a = top[3] + under[3] * (1.0 - top[3]);
    if a <= 0.0 {
        return [0.0; 4];
    }
    let c = |i: usize| (top[i] * top[3] + under[i] * under[3] * (1.0 - top[3])) / a;
    [c(0), c(1), c(2), a]
}

fn rgb(hex: u32) -> Rgba {
    [((hex >> 16) & 255) as f32 / 255.0, ((hex >> 8) & 255) as f32 / 255.0, (hex & 255) as f32 / 255.0, 1.0]
}

/// Distance from a rounded rectangle's edge (negative inside); coordinates in the unit square.
fn round_rect(x: f32, y: f32, cx: f32, cy: f32, hw: f32, hh: f32, r: f32) -> f32 {
    let (dx, dy) = ((x - cx).abs() - (hw - r), (y - cy).abs() - (hh - r));
    dx.max(dy).min(0.0) + (dx.max(0.0).powi(2) + dy.max(0.0).powi(2)).sqrt() - r
}

/// The treemap's blocks: left, top, right, bottom (unit square) and colour.
const BLOCKS: [(f32, f32, f32, f32, u32); 5] = [
    (0.170, 0.170, 0.530, 0.830, 0xFF_C4_4D),
    (0.552, 0.170, 0.830, 0.500, 0xFF_FF_FF),
    (0.552, 0.522, 0.830, 0.680, 0xE3_F6_EE),
    (0.552, 0.702, 0.680, 0.830, 0xC6_EE_DF),
    (0.702, 0.702, 0.830, 0.830, 0xA8_E4_CF),
];
const BLOCK_R: f32 = 0.035;
const TILE_TOP: u32 = 0x22_B0_7D;
const TILE_BOTTOM: u32 = 0x0E_7A_5A;

/// The icon's colour at a point of the unit square.
fn sample(x: f32, y: f32) -> Rgba {
    if round_rect(x, y, 0.5, 0.5, 0.5, 0.5, 0.225) > 0.0 {
        return [0.0; 4];
    }
    let mut c = mix(rgb(TILE_TOP), rgb(TILE_BOTTOM), y);
    // A soft shadow under the whole treemap.
    if round_rect(x, y - 0.018, 0.5, 0.5, 0.33, 0.33, 0.04) <= 0.0 {
        c = over(c, [0.0, 0.12, 0.08, 0.20]);
    }
    for (l, t, r, b, colour) in BLOCKS {
        if round_rect(x, y, (l + r) / 2.0, (t + b) / 2.0, (r - l) / 2.0, (b - t) / 2.0, BLOCK_R) <= 0.0 {
            c = rgb(colour);
        }
    }
    c
}

/// The icon as `size`×`size` RGBA8 pixels, drawn inside `scale` of the canvas (1.0 fills it).
fn render(size: u32, scale: f32) -> Vec<u8> {
    let mut out = Vec::with_capacity((size * size * 4) as usize);
    let n = (SS * SS) as f32;
    for py in 0..size {
        for px in 0..size {
            let mut acc = [0.0f32; 4];
            for sy in 0..SS {
                for sx in 0..SS {
                    let u = (px as f32 + (sx as f32 + 0.5) / SS as f32) / size as f32;
                    let v = (py as f32 + (sy as f32 + 0.5) / SS as f32) / size as f32;
                    // Shrink about the centre for icons that keep a margin.
                    let (u, v) = ((u - 0.5) / scale + 0.5, (v - 0.5) / scale + 0.5);
                    let s = if (0.0..=1.0).contains(&u) && (0.0..=1.0).contains(&v) { sample(u, v) } else { [0.0; 4] };
                    // Premultiplied while averaging, so edges don't darken.
                    acc[0] += s[0] * s[3];
                    acc[1] += s[1] * s[3];
                    acc[2] += s[2] * s[3];
                    acc[3] += s[3];
                }
            }
            let a = acc[3] / n;
            for ch in acc.iter().take(3) {
                out.push(if a > 0.0 { ((ch / n / a).clamp(0.0, 1.0) * 255.0).round() as u8 } else { 0 });
            }
            out.push((a.clamp(0.0, 1.0) * 255.0).round() as u8);
        }
    }
    out
}

fn png(size: u32, scale: f32) -> Result<Vec<u8>, String> {
    let img = image::RgbaImage::from_raw(size, size, render(size, scale)).ok_or("icon buffer has the wrong size")?;
    let mut bytes = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageRgba8(img).write_to(&mut bytes, image::ImageFormat::Png).map_err(|e| format!("encoding the icon: {e}"))?;
    Ok(bytes.into_inner())
}

/// An Apple `.icns` holding PNG entries.
fn icns(entries: &[(&[u8; 4], Vec<u8>)]) -> Vec<u8> {
    let total: usize = 8 + entries.iter().map(|(_, d)| 8 + d.len()).sum::<usize>();
    let mut out = Vec::with_capacity(total);
    out.extend_from_slice(b"icns");
    out.extend_from_slice(&(total as u32).to_be_bytes());
    for (kind, data) in entries {
        out.extend_from_slice(*kind);
        out.extend_from_slice(&((8 + data.len()) as u32).to_be_bytes());
        out.extend_from_slice(data);
    }
    out
}

/// The same drawing as SVG, for the Linux scalable icon and for anyone who wants to edit it.
fn svg() -> String {
    let px = |v: f32| (v * 1024.0).round() as i32;
    let mut blocks = String::new();
    for (l, t, r, b, colour) in BLOCKS {
        blocks.push_str(&format!(
            "  <rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" rx=\"{}\" fill=\"#{colour:06X}\"/>\n",
            px(l),
            px(t),
            px(r) - px(l),
            px(b) - px(t),
            px(BLOCK_R)
        ));
    }
    format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 1024 1024\">\n  <defs>\n    <linearGradient id=\"tile\" x1=\"0\" y1=\"0\" x2=\"0\" y2=\"1\">\n      <stop offset=\"0\" stop-color=\"#{TILE_TOP:06X}\"/>\n      <stop offset=\"1\" stop-color=\"#{TILE_BOTTOM:06X}\"/>\n    </linearGradient>\n  </defs>\n  <rect width=\"1024\" height=\"1024\" rx=\"230\" fill=\"url(#tile)\"/>\n  <rect x=\"174\" y=\"192\" width=\"676\" height=\"676\" rx=\"41\" fill=\"#001F14\" fill-opacity=\"0.2\"/>\n{blocks}</svg>\n"
    )
}

pub fn run(root: &Path) -> Result<(), String> {
    let dir = root.join("assets/app-icon");
    let write = |rel: &str, bytes: &[u8]| -> Result<(), String> {
        let path = dir.join(rel);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
        }
        std::fs::write(&path, bytes).map_err(|e| format!("{}: {e}", path.display()))?;
        println!("wrote {}", path.display());
        Ok(())
    };
    let svg = svg();
    write("whatspace.svg", svg.as_bytes())?;
    write(&format!("hicolor/scalable/apps/{APP_ID}.svg"), svg.as_bytes())?;
    let mut by_size = std::collections::BTreeMap::new();
    for size in [16u32, 24, 32, 48, 64, 128, 256, 512, 1024] {
        by_size.insert(size, png(size, 1.0)?);
    }
    let get = |size: u32| by_size.get(&size).cloned().ok_or_else(|| format!("no {size} px icon"));
    for size in [16u32, 24, 32, 48, 64, 128, 256, 512] {
        write(&format!("hicolor/{size}x{size}/apps/{APP_ID}.png"), &get(size)?)?;
    }
    write("whatspace-1024.png", &get(1024)?)?;
    write("whatspace-macos-512.png", &png(512, MAC_SCALE)?)?;
    let ico = crate::ico::pack(&[get(16)?, get(24)?, get(32)?, get(48)?, get(64)?, get(128)?, get(256)?])?;
    write("whatspace.ico", &ico)?;
    let mac = |size: u32| png(size, MAC_SCALE);
    let entries: Vec<(&[u8; 4], Vec<u8>)> = vec![
        (b"icp4", mac(16)?),
        (b"icp5", mac(32)?),
        (b"icp6", mac(64)?),
        (b"ic07", mac(128)?),
        (b"ic08", mac(256)?),
        (b"ic09", mac(512)?),
        (b"ic10", mac(1024)?),
        (b"ic11", mac(32)?),
        (b"ic12", mac(64)?),
        (b"ic13", mac(256)?),
        (b"ic14", mac(512)?),
    ];
    write("whatspace.icns", &icns(&entries))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_icon_has_its_parts() {
        // Outside the rounded corner: see-through. The tile, the amber block, a white block.
        assert_eq!(sample(0.01, 0.01)[3], 0.0);
        let tile = sample(0.5, 0.06);
        assert!(tile[1] > tile[0] && tile[1] > tile[2], "green tile: {tile:?}");
        assert_eq!(sample(0.35, 0.5), rgb(0xFF_C4_4D));
        assert_eq!(sample(0.69, 0.33), rgb(0xFF_FF_FF));
        // The gap between blocks shows the tile, not a block.
        assert_ne!(sample(0.541, 0.5), rgb(0xFF_C4_4D));
    }

    #[test]
    fn renders_and_packs() {
        let px = render(16, 1.0);
        assert_eq!(px.len(), 16 * 16 * 4);
        assert_eq!(px[3], 0, "the corner pixel is see-through");
        assert_eq!(px[(8 * 16 + 8) * 4 + 3], 255);
        let data = png(32, 1.0).unwrap();
        assert_eq!(crate::ico::png_size(&data).unwrap(), (32, 32));
        let icns = icns(&[(b"icp5", data.clone())]);
        assert_eq!(&icns[..4], b"icns");
        assert_eq!(u32::from_be_bytes([icns[4], icns[5], icns[6], icns[7]]) as usize, icns.len());
        assert!(svg().contains("viewBox=\"0 0 1024 1024\"") && svg().matches("<rect").count() == 7);
    }
}
