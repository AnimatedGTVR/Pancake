/// Runtime configuration for the Pancake compositor.
///
/// Config is loaded from `$XDG_CONFIG_HOME/pancake/config.toml`
/// (falling back to `~/.config/pancake/config.toml`). Missing values
/// fall back to built-in defaults and the `PANCAKE_TERMINAL` env-var.
///
/// Send SIGHUP to the compositor process to reload the config file at
/// runtime without restarting.
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};

use serde::Deserialize;
use tracing::{info, warn};

// ── SIGHUP reload flag ────────────────────────────────────────────────────────

/// Set to `true` by the SIGHUP signal handler; cleared after reloading.
pub static RELOAD_REQUESTED: AtomicBool = AtomicBool::new(false);

/// Install a SIGHUP handler that sets [`RELOAD_REQUESTED`].
///
/// # Safety
/// The handler only performs an atomic store, which is async-signal-safe.
pub fn install_sighup_handler() {
    unsafe {
        libc::signal(libc::SIGHUP, sighup_handler as *const () as libc::sighandler_t);
    }
}

extern "C" fn sighup_handler(_: libc::c_int) {
    RELOAD_REQUESTED.store(true, Ordering::Relaxed);
}

// ── Config struct ─────────────────────────────────────────────────────────────

/// Live compositor configuration. All fields have sensible defaults.
#[derive(Debug, Clone)]
pub struct Config {
    /// Terminal emulator launched by Super+T.
    pub terminal: String,
    /// Number of dual-Kawase blur ping-pong passes (higher = more blur).
    pub blur_passes: usize,
    /// Divisor for blur FBO resolution (2 = half-res, cheaper).
    pub blur_downsample: u32,
    /// Aero glass tint colour layered over the blurred background (RGBA, linear).
    pub tint: [f32; 4],
}

impl Config {
    /// Load from disk, falling back to built-in defaults for any missing values.
    pub fn load() -> Self {
        let path = config_path();
        let src = match std::fs::read_to_string(&path) {
            Ok(s) => s,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                info!("No config file at {path:?} — using defaults");
                return Self::defaults();
            }
            Err(e) => {
                warn!("Could not read config at {path:?}: {e} — using defaults");
                return Self::defaults();
            }
        };

        let file: ConfigFile = match toml::from_str(&src) {
            Ok(f) => f,
            Err(e) => {
                warn!("Config parse error in {path:?}: {e} — using defaults");
                return Self::defaults();
            }
        };

        let d = Self::defaults();
        let comp = file.compositor.unwrap_or_default();
        info!("Config loaded from {path:?}");

        Self {
            terminal: comp.terminal.unwrap_or(d.terminal),
            blur_passes: comp.blur_passes.unwrap_or(d.blur_passes),
            blur_downsample: comp.blur_downsample.unwrap_or(d.blur_downsample),
            tint: comp.tint.unwrap_or(d.tint),
        }
    }

    fn defaults() -> Self {
        Self {
            terminal: std::env::var("PANCAKE_TERMINAL")
                .unwrap_or_else(|_| "foot".into()),
            blur_passes: 4,
            blur_downsample: 2,
            tint: [0.55, 0.70, 1.00, 0.18],
        }
    }
}

// ── TOML file shape ───────────────────────────────────────────────────────────

#[derive(Deserialize, Default)]
struct ConfigFile {
    compositor: Option<CompositorSection>,
}

#[derive(Deserialize, Default)]
struct CompositorSection {
    terminal: Option<String>,
    blur_passes: Option<usize>,
    blur_downsample: Option<u32>,
    /// RGBA, linear (e.g. `tint = [0.55, 0.70, 1.00, 0.18]`)
    tint: Option<[f32; 4]>,
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn config_path() -> PathBuf {
    let base = std::env::var("XDG_CONFIG_HOME")
        .ok()
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            let home = std::env::var("HOME").unwrap_or_else(|_| "/root".into());
            PathBuf::from(home).join(".config")
        });
    base.join("pancake").join("config.toml")
}
