/// Runtime configuration for the Pancake compositor.
///
/// All values come from environment variables so Pancake works out of the box
/// with no config file. A proper config-file parser can layer on top later.
pub struct Config {
    /// Terminal emulator launched by Super+T.
    /// Reads `PANCAKE_TERMINAL`; defaults to `foot`.
    pub terminal: String,
}

impl Config {
    pub fn from_env() -> Self {
        Self {
            terminal: std::env::var("PANCAKE_TERMINAL")
                .unwrap_or_else(|_| "foot".to_string()),
        }
    }
}
