/// Default cursor image loader.
///
/// Tries the system xcursor theme first; falls back to a built-in 16×16
/// white arrow so Pancake always shows something.

pub const DEFAULT_CURSOR_SIZE: u32 = 24;

// ── Public API ────────────────────────────────────────────────────────────────

/// RGBA pixels + dimensions + hotspot for the pointer cursor.
pub struct CursorImage {
    pub pixels: Vec<u8>,   // RGBA8
    pub width:  u32,
    pub height: u32,
    pub hot_x:  u32,
    pub hot_y:  u32,
}

/// Load the system cursor, falling back to the built-in arrow.
pub fn load_default() -> CursorImage {
    if let Some(img) = try_xcursor() {
        return img;
    }
    builtin_arrow()
}

// ── xcursor attempt ───────────────────────────────────────────────────────────

fn try_xcursor() -> Option<CursorImage> {
    let theme = std::env::var("XCURSOR_THEME").unwrap_or_else(|_| "default".into());
    let want_size: u32 = std::env::var("XCURSOR_SIZE")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(DEFAULT_CURSOR_SIZE);

    let theme = xcursor::CursorTheme::load(&theme);
    let path  = theme.load_icon("default")?;
    let data  = std::fs::read(path).ok()?;
    let images = xcursor::parser::parse_xcursor(&data)?;

    let img = images
        .iter()
        .min_by_key(|i| (i.size as i32 - want_size as i32).unsigned_abs())
        .or_else(|| images.first())?;

    // xcursor pixels are packed ARGB u32 — convert to RGBA bytes
    let pixels: Vec<u8> = img.pixels_argb.iter().flat_map(|&argb| {
        let argb = argb as u32;
        let a = ((argb >> 24) & 0xFF) as u8;
        let r = ((argb >> 16) & 0xFF) as u8;
        let g = ((argb >> 8)  & 0xFF) as u8;
        let b = (argb          & 0xFF) as u8;
        [r, g, b, a]
    }).collect();

    Some(CursorImage {
        pixels,
        width:  img.size,
        height: img.size,
        hot_x:  img.xhot,
        hot_y:  img.yhot,
    })
}

// ── Built-in 16×16 arrow (fallback) ──────────────────────────────────────────

fn builtin_arrow() -> CursorImage {
    const S: usize = 16;
    #[rustfmt::skip]
    const MAP: &[&str] = &[
        "B...............",
        "BB..............",
        "BWB.............",
        "BWWB............",
        "BWWWB...........",
        "BWWWWB..........",
        "BWWWWWB.........",
        "BWWWWWWB........",
        "BWWWWWWWB.......",
        "BWWWWWB.........",
        "BWWBWWB.........",
        "BWB.BWWB........",
        "BB...BWWB.......",
        "B.....BWB.......",
        "......BB........",
        "................",
    ];

    let mut pixels = vec![0u8; S * S * 4];
    for (y, row) in MAP.iter().enumerate() {
        for (x, ch) in row.chars().enumerate() {
            let i = (y * S + x) * 4;
            match ch {
                'B' => { pixels[i] = 0;   pixels[i+1] = 0;   pixels[i+2] = 0;   pixels[i+3] = 255; }
                'W' => { pixels[i] = 255; pixels[i+1] = 255; pixels[i+2] = 255; pixels[i+3] = 255; }
                _   => {}
            }
        }
    }

    CursorImage { pixels, width: S as u32, height: S as u32, hot_x: 0, hot_y: 0 }
}
