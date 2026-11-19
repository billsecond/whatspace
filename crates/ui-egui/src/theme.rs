//! Colours. Text and surfaces come from egui's own light and dark visuals, so the window
//! follows the system's theme; the bars, folders and file-type colours are ours.

use egui::Color32;
use whatspace_model::Category;

/// The colours one frame is drawn with.
#[derive(Clone, Copy, Debug)]
pub struct Palette {
    pub dark: bool,
    pub text: Color32,
    pub weak: Color32,
    pub accent: Color32,
    pub bar: Color32,
    pub track: Color32,
    pub selected: Color32,
    pub hover: Color32,
    pub warn: Color32,
    pub folder: Color32,
    pub grid: Color32,
    pub canvas: Color32,
}

impl Palette {
    pub fn of(ui: &egui::Ui) -> Palette {
        let v = ui.visuals();
        let dark = v.dark_mode;
        Palette {
            dark,
            text: v.text_color(),
            weak: v.weak_text_color(),
            accent: if dark { Color32::from_rgb(0x5E, 0xA8, 0xF8) } else { Color32::from_rgb(0x1F, 0x6F, 0xD1) },
            bar: if dark { Color32::from_rgb(0x3D, 0x8B, 0xE0) } else { Color32::from_rgb(0x4A, 0x90, 0xE2) },
            track: if dark { Color32::from_rgb(0x2A, 0x2E, 0x35) } else { Color32::from_rgb(0xE4, 0xE8, 0xEE) },
            selected: v.selection.bg_fill,
            hover: if dark { Color32::from_white_alpha(14) } else { Color32::from_black_alpha(12) },
            warn: v.warn_fg_color,
            folder: if dark { Color32::from_rgb(0xE8, 0xB9, 0x4A) } else { Color32::from_rgb(0xE0, 0xA8, 0x2E) },
            grid: if dark { Color32::from_white_alpha(18) } else { Color32::from_black_alpha(20) },
            canvas: v.extreme_bg_color,
        }
    }
}

/// The colour of a kind of file, in the treemap, the extension list and the file icons.
pub fn category_color(c: Category) -> Color32 {
    match c {
        Category::Image => Color32::from_rgb(0x3B, 0x9C, 0xD9),
        Category::Video => Color32::from_rgb(0xD9, 0x4F, 0x70),
        Category::Audio => Color32::from_rgb(0x8E, 0x6B, 0xD8),
        Category::Archive => Color32::from_rgb(0xE0, 0x9A, 0x2B),
        Category::Document => Color32::from_rgb(0x3F, 0xB2, 0x8C),
        Category::Code => Color32::from_rgb(0x6C, 0x8E, 0xBF),
        Category::Program => Color32::from_rgb(0xC0, 0x62, 0x3A),
        Category::Data => Color32::from_rgb(0xB5, 0xA4, 0x2E),
        Category::System => Color32::from_rgb(0x8A, 0x8F, 0x98),
        Category::Other => Color32::from_rgb(0xA8, 0x8B, 0x75),
    }
}

/// The background of a folder in the treemap, a little darker each level down.
pub fn folder_fill(depth: u16, dark: bool) -> Color32 {
    let step = u8::try_from(depth.min(8)).unwrap_or(8);
    if dark { Color32::from_gray(0x34u8.saturating_sub(step * 3)) } else { Color32::from_gray(0xEEu8.saturating_sub(step * 6)) }
}

/// How full a volume looks: green, then amber past 75 %, red past 90 %.
pub fn usage_color(fraction: f32) -> Color32 {
    if fraction > 0.9 {
        Color32::from_rgb(0xD9, 0x48, 0x3B)
    } else if fraction > 0.75 {
        Color32::from_rgb(0xE0, 0x9A, 0x2B)
    } else {
        Color32::from_rgb(0x3F, 0xA8, 0x5E)
    }
}

/// `c` lighter (`amount` > 0) or darker (< 0), by up to 1.0.
pub fn shade(c: Color32, amount: f32) -> Color32 {
    let a = amount.clamp(-1.0, 1.0);
    let f = |v: u8| -> u8 {
        let v = f32::from(v);
        let out = if a >= 0.0 { v + (255.0 - v) * a } else { v * (1.0 + a) };
        out.round().clamp(0.0, 255.0) as u8
    };
    Color32::from_rgba_unmultiplied(f(c.r()), f(c.g()), f(c.b()), c.a())
}
