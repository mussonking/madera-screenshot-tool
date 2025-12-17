use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColorInfo {
    pub hex: String,
    pub hex_lower: String,
    pub rgb: RgbColor,
    pub hsl: HslColor,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RgbColor {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HslColor {
    pub h: u16,
    pub s: u8,
    pub l: u8,
}

impl ColorInfo {
    pub fn from_rgb(r: u8, g: u8, b: u8) -> Self {
        let hex = format!("#{:02X}{:02X}{:02X}", r, g, b);
        let hex_lower = format!("#{:02x}{:02x}{:02x}", r, g, b);
        let hsl = rgb_to_hsl(r, g, b);

        Self {
            hex,
            hex_lower,
            rgb: RgbColor { r, g, b },
            hsl,
        }
    }

    pub fn format_rgb(&self) -> String {
        format!("rgb({}, {}, {})", self.rgb.r, self.rgb.g, self.rgb.b)
    }

    pub fn format_hsl(&self) -> String {
        format!("hsl({}, {}%, {}%)", self.hsl.h, self.hsl.s, self.hsl.l)
    }
}

fn rgb_to_hsl(r: u8, g: u8, b: u8) -> HslColor {
    let r = r as f64 / 255.0;
    let g = g as f64 / 255.0;
    let b = b as f64 / 255.0;

    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let l = (max + min) / 2.0;

    if (max - min).abs() < f64::EPSILON {
        return HslColor {
            h: 0,
            s: 0,
            l: (l * 100.0).round() as u8,
        };
    }

    let d = max - min;
    let s = if l > 0.5 {
        d / (2.0 - max - min)
    } else {
        d / (max + min)
    };

    let h = if (max - r).abs() < f64::EPSILON {
        ((g - b) / d + if g < b { 6.0 } else { 0.0 }) / 6.0
    } else if (max - g).abs() < f64::EPSILON {
        ((b - r) / d + 2.0) / 6.0
    } else {
        ((r - g) / d + 4.0) / 6.0
    };

    HslColor {
        h: (h * 360.0).round() as u16,
        s: (s * 100.0).round() as u8,
        l: (l * 100.0).round() as u8,
    }
}

/// Get pixel color at screen coordinates
pub fn get_pixel_color(x: i32, y: i32) -> Option<ColorInfo> {
    #[cfg(target_os = "windows")]
    {
        use windows::Win32::Graphics::Gdi::{GetDC, GetPixel, ReleaseDC};
        use windows::Win32::Foundation::HWND;

        unsafe {
            let hdc = GetDC(HWND::default());
            if hdc.is_invalid() {
                return None;
            }

            let color = GetPixel(hdc, x, y);
            ReleaseDC(HWND::default(), hdc);

            if color.0 == 0xFFFFFFFF {
                return None;
            }

            let r = (color.0 & 0xFF) as u8;
            let g = ((color.0 >> 8) & 0xFF) as u8;
            let b = ((color.0 >> 16) & 0xFF) as u8;

            Some(ColorInfo::from_rgb(r, g, b))
        }
    }

    #[cfg(not(target_os = "windows"))]
    {
        None
    }
}

/// Get a region of pixels around a point for magnifier
pub fn get_magnifier_region(
    center_x: i32,
    center_y: i32,
    radius: i32,
) -> Option<Vec<Vec<(u8, u8, u8)>>> {
    #[cfg(target_os = "windows")]
    {
        use windows::Win32::Graphics::Gdi::{GetDC, GetPixel, ReleaseDC};
        use windows::Win32::Foundation::HWND;

        unsafe {
            let hdc = GetDC(HWND::default());
            if hdc.is_invalid() {
                return None;
            }

            let size = radius * 2 + 1;
            let mut pixels: Vec<Vec<(u8, u8, u8)>> = Vec::with_capacity(size as usize);

            for dy in -radius..=radius {
                let mut row: Vec<(u8, u8, u8)> = Vec::with_capacity(size as usize);
                for dx in -radius..=radius {
                    let color = GetPixel(hdc, center_x + dx, center_y + dy);
                    if color.0 == 0xFFFFFFFF {
                        row.push((0, 0, 0));
                    } else {
                        let r = (color.0 & 0xFF) as u8;
                        let g = ((color.0 >> 8) & 0xFF) as u8;
                        let b = ((color.0 >> 16) & 0xFF) as u8;
                        row.push((r, g, b));
                    }
                }
                pixels.push(row);
            }

            ReleaseDC(HWND::default(), hdc);
            Some(pixels)
        }
    }

    #[cfg(not(target_os = "windows"))]
    {
        None
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ColorFormat {
    HexUpper,
    HexLower,
    Rgb,
    Hsl,
}

impl Default for ColorFormat {
    fn default() -> Self {
        Self::HexUpper
    }
}

impl ColorFormat {
    pub fn format(&self, color: &ColorInfo) -> String {
        match self {
            ColorFormat::HexUpper => color.hex.clone(),
            ColorFormat::HexLower => color.hex_lower.clone(),
            ColorFormat::Rgb => color.format_rgb(),
            ColorFormat::Hsl => color.format_hsl(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColorPickSettings {
    pub format: ColorFormat,
    pub max_history: usize,
    pub magnifier_size: u8,  // radius in pixels
}

impl Default for ColorPickSettings {
    fn default() -> Self {
        Self {
            format: ColorFormat::HexUpper,
            max_history: 50,
            magnifier_size: 10,
        }
    }
}
