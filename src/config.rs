/// Runtime configuration for the Pancake compositor.
///
/// Pancake loads config from (in order, first found wins):
///   $XDG_CONFIG_HOME/pancake/config.confi   — Syrup universal format
///   $XDG_CONFIG_HOME/pancake/config.toml    — legacy TOML (still supported)
///
/// Send SIGHUP to the running compositor to reload without restarting.
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};

use serde::Deserialize;
use tracing::{info, warn};

use crate::syrup;

// ── SIGHUP reload flag ────────────────────────────────────────────────────────

pub static RELOAD_REQUESTED: AtomicBool = AtomicBool::new(false);

pub fn install_sighup_handler() {
    unsafe {
        libc::signal(libc::SIGHUP, sighup_handler as *const () as libc::sighandler_t);
    }
}

extern "C" fn sighup_handler(_: libc::c_int) {
    RELOAD_REQUESTED.store(true, Ordering::Relaxed);
}

// ── Config struct ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct Config {
    /// Terminal emulator launched by Super+T (or configured keybind).
    pub terminal: String,
    /// Dual-Kawase blur ping-pong passes (higher = more blur).
    pub blur_passes: usize,
    /// Divisor for blur FBO resolution (2 = half-res).
    pub blur_downsample: u32,
    /// Aero glass tint layered over blurred background (RGBA, linear).
    pub tint: [f32; 4],
    /// Optional wallpaper image path. Supports JPEG, PNG, BMP, GIF.
    pub wallpaper: Option<PathBuf>,
    /// Keybindings (Super + <key>).
    pub keybinds: Keybinds,
    /// Extra apps launched at startup (beyond foot + waybar).
    #[allow(dead_code)]
    pub startup_apps: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct Keybinds {
    pub terminal: String,
    pub close:    String,
    pub quit:     String,
    pub cycle:    String,
}

impl Default for Keybinds {
    fn default() -> Self {
        Self {
            terminal: "Super+T".into(),
            close:    "Super+Q".into(),
            quit:     "Super+Escape".into(),
            cycle:    "Super+Tab".into(),
        }
    }
}

impl Config {
    pub fn load() -> Self {
        let base = config_base_dir();

        // Try .confi first, then fall back to .toml
        let confi_path = base.join("config.confi");
        let toml_path  = base.join("config.toml");

        if confi_path.exists() {
            info!("Loading Syrup config from {confi_path:?}");
            let doc = syrup::load(&confi_path);
            return Self::from_syrup(&doc);
        }

        if toml_path.exists() {
            info!("Loading TOML config from {toml_path:?}");
            if let Some(cfg) = Self::from_toml(&toml_path) {
                return cfg;
            }
        }

        info!("No config found in {base:?} — using defaults");
        Self::defaults()
    }

    fn defaults() -> Self {
        Self {
            terminal:       std::env::var("PANCAKE_TERMINAL").unwrap_or_else(|_| "foot".into()),
            blur_passes:    4,
            blur_downsample: 2,
            tint:           [0.55, 0.70, 1.00, 0.18],
            wallpaper:      None,
            keybinds:       Keybinds::default(),
            startup_apps:   vec![],
        }
    }

    // ── Syrup loader ──────────────────────────────────────────────────────────

    fn from_syrup(doc: &syrup::SyrupDoc) -> Self {
        let d = Self::defaults();
        Self {
            terminal: doc.str_val("compositor", "terminal")
                .unwrap_or(d.terminal),
            blur_passes: doc.int_val("compositor", "blur_passes")
                .map(|i| i as usize)
                .unwrap_or(d.blur_passes),
            blur_downsample: doc.int_val("compositor", "blur_downsample")
                .map(|i| i as u32)
                .unwrap_or(d.blur_downsample),
            tint: doc.f32_array::<4>("compositor", "tint")
                .unwrap_or(d.tint),
            wallpaper: doc.str_val("compositor", "wallpaper")
                .map(PathBuf::from),
            keybinds: Keybinds {
                terminal: doc.str_val("keybinds", "terminal").unwrap_or(d.keybinds.terminal),
                close:    doc.str_val("keybinds", "close").unwrap_or(d.keybinds.close),
                quit:     doc.str_val("keybinds", "quit").unwrap_or(d.keybinds.quit),
                cycle:    doc.str_val("keybinds", "cycle").unwrap_or(d.keybinds.cycle),
            },
            startup_apps: doc.0.get("startup")
                .and_then(|s| s.get("apps"))
                .and_then(|v| {
                    if let syrup::SyrupValue::Array(arr) = v {
                        Some(arr.iter().filter_map(|i| i.as_str().map(String::from)).collect())
                    } else {
                        None
                    }
                })
                .unwrap_or_default(),
        }
    }

    // ── TOML loader (legacy) ──────────────────────────────────────────────────

    fn from_toml(path: &std::path::Path) -> Option<Self> {
        let src = std::fs::read_to_string(path).ok()?;
        let file: ConfigFile = match toml::from_str(&src) {
            Ok(f) => f,
            Err(e) => {
                warn!("TOML parse error in {path:?}: {e}");
                return None;
            }
        };
        let d = Self::defaults();
        let comp = file.compositor.unwrap_or_default();
        Some(Self {
            terminal:        comp.terminal.unwrap_or(d.terminal),
            blur_passes:     comp.blur_passes.unwrap_or(d.blur_passes),
            blur_downsample: comp.blur_downsample.unwrap_or(d.blur_downsample),
            tint:            comp.tint.unwrap_or(d.tint),
            wallpaper:       comp.wallpaper.map(PathBuf::from),
            keybinds:        Keybinds::default(),
            startup_apps:    vec![],
        })
    }
}

// ── TOML file shape ───────────────────────────────────────────────────────────

#[derive(Deserialize, Default)]
struct ConfigFile {
    compositor: Option<CompositorSection>,
}

#[derive(Deserialize, Default)]
struct CompositorSection {
    terminal:        Option<String>,
    blur_passes:     Option<usize>,
    blur_downsample: Option<u32>,
    tint:            Option<[f32; 4]>,
    wallpaper:       Option<String>,
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn config_base_dir() -> PathBuf {
    let base = std::env::var("XDG_CONFIG_HOME")
        .ok()
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            let home = std::env::var("HOME").unwrap_or_else(|_| "/root".into());
            PathBuf::from(home).join(".config")
        });
    base.join("pancake")
}
