/// Syrup — the Pancake universal config IR.
///
/// `.confi` files declare a language with a `!lang <name>` directive on the
/// first non-comment line (or default to Syrup native). All frontends parse
/// their input and produce a `SyrupDoc` (nested string map). The compositor
/// then reads typed values from the doc via the helper methods.
///
/// # Supported languages
///
/// | Directive        | Syntax style                                  |
/// |------------------|-----------------------------------------------|
/// | `!lang syrup`    | Syrup native — block-scoped `key = value`     |
/// | `!lang lua`      | Lua 5.4 — return a nested table               |
/// | `!lang luajit`   | Same as `lua` (LuaJIT compat subset)          |
/// | `!lang cpp`      | C++ style — block `{ typed key = val; }`     |
/// | `!lang csharp`   | C# style  — same parser as cpp                |
///
/// # Example `.confi` (native Syrup)
/// ```text
/// compositor {
///     terminal   = "foot"
///     blur_passes = 4
///     wallpaper  = "/usr/share/pancake/walls/aero-default.jpg"
///     tint       = [0.55, 0.70, 1.00, 0.18]
/// }
/// keybinds {
///     terminal = "Super+T"
///     close    = "Super+Q"
///     quit     = "Super+Escape"
///     cycle    = "Super+Tab"
/// }
/// ```
pub mod native;
pub mod lua;

use std::collections::HashMap;

// ── Syrup IR ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum SyrupValue {
    String(String),
    Int(i64),
    Float(f64),
    Bool(bool),
    Array(Vec<SyrupValue>),
}

#[allow(dead_code)]
impl SyrupValue {
    pub fn as_str(&self) -> Option<&str> {
        if let Self::String(s) = self { Some(s.as_str()) } else { None }
    }
    pub fn as_int(&self) -> Option<i64> {
        match self {
            Self::Int(i)   => Some(*i),
            Self::Float(f) => Some(*f as i64),
            _              => None,
        }
    }
    pub fn as_float(&self) -> Option<f64> {
        match self {
            Self::Float(f) => Some(*f),
            Self::Int(i)   => Some(*i as f64),
            _              => None,
        }
    }
    pub fn as_bool(&self) -> Option<bool> {
        if let Self::Bool(b) = self { Some(*b) } else { None }
    }
    pub fn as_f32_array<const N: usize>(&self) -> Option<[f32; N]> {
        if let Self::Array(arr) = self {
            if arr.len() == N {
                let mut out = [0.0f32; N];
                for (i, v) in arr.iter().enumerate() {
                    out[i] = v.as_float()? as f32;
                }
                return Some(out);
            }
        }
        None
    }
}

/// A section → (key → value) map. Flat sections like `compositor.terminal`.
pub type SyrupSection = HashMap<String, SyrupValue>;

/// Top-level document: section name → its key/value pairs.
#[derive(Debug, Default)]
pub struct SyrupDoc(pub HashMap<String, SyrupSection>);

#[allow(dead_code)]
impl SyrupDoc {
    /// Get a value by `section.key`.
    pub fn get(&self, section: &str, key: &str) -> Option<&SyrupValue> {
        self.0.get(section)?.get(key)
    }

    /// Convenience typed getters.
    pub fn str_val(&self, section: &str, key: &str) -> Option<String> {
        self.get(section, key)?.as_str().map(String::from)
    }
    pub fn int_val(&self, section: &str, key: &str) -> Option<i64> {
        self.get(section, key)?.as_int()
    }
    pub fn float_val(&self, section: &str, key: &str) -> Option<f64> {
        self.get(section, key)?.as_float()
    }
    pub fn bool_val(&self, section: &str, key: &str) -> Option<bool> {
        self.get(section, key)?.as_bool()
    }
    pub fn f32_array<const N: usize>(&self, section: &str, key: &str) -> Option<[f32; N]> {
        self.get(section, key)?.as_f32_array::<N>()
    }
}

// ── Language detection + dispatch ─────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ConfiLang {
    Syrup,
    Lua,
    Cpp,   // C++ style — same parser as C#
    CSharp,
}

fn detect_lang(src: &str) -> ConfiLang {
    for line in src.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with("//") {
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("!lang") {
            let lang = rest.trim().to_lowercase();
            return match lang.as_str() {
                "lua" | "luajit" => ConfiLang::Lua,
                "cpp" | "c++"   => ConfiLang::Cpp,
                "csharp" | "c#" => ConfiLang::CSharp,
                _               => ConfiLang::Syrup,
            };
        }
        // No !lang directive found on the first meaningful line → native
        break;
    }
    ConfiLang::Syrup
}

/// Parse a `.confi` file and return the Syrup IR.
pub fn parse(src: &str) -> SyrupDoc {
    match detect_lang(src) {
        ConfiLang::Lua             => lua::parse(src),
        ConfiLang::Cpp |
        ConfiLang::CSharp          => native::parse_c_style(src),
        ConfiLang::Syrup           => native::parse(src),
    }
}

/// Load a `.confi` file from disk. Returns an empty doc on any error.
pub fn load(path: &std::path::Path) -> SyrupDoc {
    match std::fs::read_to_string(path) {
        Ok(src) => parse(&src),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => SyrupDoc::default(),
        Err(e) => {
            tracing::warn!("Could not read {path:?}: {e}");
            SyrupDoc::default()
        }
    }
}
