//! How sizes, counts and shares are written.
//!
//! Platforms disagree about what "MB" means. Windows' Explorer counts in 1024s and writes KB,
//! MB and GB; macOS's Finder counts in 1000s; GNOME and KDE count in 1024s and write KiB, MiB
//! and GiB. [`UnitStyle::platform_default`] follows the platform, and the owner can choose.

use serde::{Deserialize, Serialize};

/// How sizes are counted and labelled.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum UnitStyle {
    /// 1024s, labelled KB, MB, GB (Windows' Explorer).
    Windows,
    /// 1024s, labelled KiB, MiB, GiB (IEC; GNOME and KDE).
    Iec,
    /// 1000s, labelled kB, MB, GB (SI; macOS's Finder, drive makers).
    Si,
}

impl Default for UnitStyle {
    fn default() -> Self {
        Self::platform_default()
    }
}

impl UnitStyle {
    pub const ALL: [UnitStyle; 3] = [UnitStyle::Windows, UnitStyle::Iec, UnitStyle::Si];

    /// What the platform's own file manager uses.
    pub const fn platform_default() -> UnitStyle {
        if cfg!(windows) {
            UnitStyle::Windows
        } else if cfg!(target_os = "macos") {
            UnitStyle::Si
        } else {
            UnitStyle::Iec
        }
    }

    pub const fn base(self) -> u64 {
        match self {
            UnitStyle::Si => 1000,
            UnitStyle::Windows | UnitStyle::Iec => 1024,
        }
    }

    /// The unit names from bytes up.
    pub const fn units(self) -> [&'static str; 7] {
        match self {
            UnitStyle::Windows => ["bytes", "KB", "MB", "GB", "TB", "PB", "EB"],
            UnitStyle::Iec => ["bytes", "KiB", "MiB", "GiB", "TiB", "PiB", "EiB"],
            UnitStyle::Si => ["bytes", "kB", "MB", "GB", "TB", "PB", "EB"],
        }
    }

    /// The style's name in a menu.
    pub const fn label(self) -> &'static str {
        match self {
            UnitStyle::Windows => "KB, MB, GB of 1024 (Windows)",
            UnitStyle::Iec => "KiB, MiB, GiB of 1024 (IEC)",
            UnitStyle::Si => "kB, MB, GB of 1000 (SI, macOS)",
        }
    }
}

/// Whether sizes pick their own unit or all use one.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Scale {
    #[default]
    Auto,
    Bytes,
    Kilo,
    Mega,
    Giga,
    Tera,
}

impl Scale {
    pub const ALL: [Scale; 6] = [Scale::Auto, Scale::Bytes, Scale::Kilo, Scale::Mega, Scale::Giga, Scale::Tera];

    const fn exponent(self) -> Option<u32> {
        match self {
            Scale::Auto => None,
            Scale::Bytes => Some(0),
            Scale::Kilo => Some(1),
            Scale::Mega => Some(2),
            Scale::Giga => Some(3),
            Scale::Tera => Some(4),
        }
    }

    /// The scale's name in a menu, in `style`'s units.
    pub fn label(self, style: UnitStyle) -> String {
        match self.exponent() {
            None => "Automatic".to_string(),
            Some(e) => style.units().get(e as usize).copied().unwrap_or("bytes").to_string(),
        }
    }
}

/// How to write a size.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SizeFormat {
    pub style: UnitStyle,
    pub scale: Scale,
}

impl SizeFormat {
    pub fn new(style: UnitStyle, scale: Scale) -> SizeFormat {
        SizeFormat { style, scale }
    }

    /// `bytes` written in this format: "1 byte", "512 bytes", "1.46 KB", "12.3 MB", "245 GB".
    pub fn format(&self, bytes: u64) -> String {
        let units = self.style.units();
        let base = self.style.base() as f64;
        match self.scale.exponent() {
            Some(0) => format!("{} {}", group(bytes), units[0]),
            Some(e) => {
                let v = bytes as f64 / base.powi(e as i32);
                format!("{} {}", group_float(v, 1), units.get(e as usize).copied().unwrap_or(""))
            }
            None => {
                if bytes == 1 {
                    return "1 byte".to_string();
                }
                if bytes < self.style.base() {
                    return format!("{bytes} bytes");
                }
                let mut v = bytes as f64;
                let mut e = 0usize;
                while v >= base && e + 1 < units.len() {
                    v /= base;
                    e += 1;
                }
                // Three significant digits, the way file managers write sizes.
                let text = if v < 10.0 {
                    format!("{v:.2}")
                } else if v < 100.0 {
                    format!("{v:.1}")
                } else {
                    format!("{v:.0}")
                };
                format!("{text} {}", units.get(e).copied().unwrap_or(""))
            }
        }
    }
}

/// A count with thousands separators: 1234567 → "1,234,567".
pub fn group(n: u64) -> String {
    let digits = n.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (i, ch) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(ch);
    }
    out
}

/// A number with thousands separators and `decimals` decimals: 12345.67 → "12,345.7".
pub fn group_float(v: f64, decimals: usize) -> String {
    if !v.is_finite() || v < 0.0 {
        return "0".to_string();
    }
    let text = format!("{v:.decimals$}");
    let (whole, frac) = text.split_once('.').unwrap_or((&text, ""));
    let whole = whole.parse::<u64>().map(group).unwrap_or_else(|_| whole.to_string());
    if frac.is_empty() { whole } else { format!("{whole}.{frac}") }
}

/// `part` as a percentage of `whole` (0 when the whole is nothing).
pub fn percent(part: u64, whole: u64) -> f64 {
    if whole == 0 { 0.0 } else { (part as f64 / whole as f64 * 100.0).clamp(0.0, 100.0) }
}

/// A percentage as written in a column: "12.3 %", "< 0.1 %" for a sliver, "0 %" for nothing.
pub fn format_percent(p: f64) -> String {
    if !p.is_finite() || p <= 0.0 {
        "0 %".to_string()
    } else if p < 0.1 {
        "< 0.1 %".to_string()
    } else if p >= 99.95 {
        "100 %".to_string()
    } else {
        format!("{p:.1} %")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn automatic_sizes_in_each_style() {
        let w = SizeFormat::new(UnitStyle::Windows, Scale::Auto);
        assert_eq!(w.format(0), "0 bytes");
        assert_eq!(w.format(1), "1 byte");
        assert_eq!(w.format(1023), "1023 bytes");
        assert_eq!(w.format(1024), "1.00 KB");
        assert_eq!(w.format(1536), "1.50 KB");
        assert_eq!(w.format(12 * 1024 * 1024 + 300 * 1024), "12.3 MB");
        assert_eq!(w.format(245 * 1024 * 1024 * 1024), "245 GB");
        let iec = SizeFormat::new(UnitStyle::Iec, Scale::Auto);
        assert_eq!(iec.format(3 * 1024 * 1024), "3.00 MiB");
        let si = SizeFormat::new(UnitStyle::Si, Scale::Auto);
        assert_eq!(si.format(999), "999 bytes");
        assert_eq!(si.format(1000), "1.00 kB");
        assert_eq!(si.format(500_000_000_000), "500 GB");
        assert_eq!(si.format(u64::MAX), "18.4 EB");
    }

    #[test]
    fn fixed_scales() {
        let mb = SizeFormat::new(UnitStyle::Windows, Scale::Mega);
        assert_eq!(mb.format(5 * 1024 * 1024 * 1024), "5,120.0 MB");
        let bytes = SizeFormat::new(UnitStyle::Si, Scale::Bytes);
        assert_eq!(bytes.format(1_234_567), "1,234,567 bytes");
        assert_eq!(Scale::Giga.label(UnitStyle::Iec), "GiB");
        assert_eq!(Scale::Auto.label(UnitStyle::Iec), "Automatic");
    }

    #[test]
    fn counts_and_percentages() {
        assert_eq!(group(0), "0");
        assert_eq!(group(999), "999");
        assert_eq!(group(1000), "1,000");
        assert_eq!(group(1_234_567), "1,234,567");
        assert_eq!(group_float(f64::NAN, 1), "0");
        assert_eq!(percent(1, 0), 0.0);
        assert_eq!(percent(1, 4), 25.0);
        assert_eq!(format_percent(0.0), "0 %");
        assert_eq!(format_percent(0.04), "< 0.1 %");
        assert_eq!(format_percent(12.34), "12.3 %");
        assert_eq!(format_percent(100.0), "100 %");
    }
}
