// a3sql config — version validation, startup options from a3sql.toml
//
// Reads `$A3SQL_CONFIG` or `./a3sql.toml` for an optional game_version override.
// If the wiki data version differs from the configured game version, the engine
// logs a drift warning at startup so users know their command database might
// not match their installed Arma 3 version.

use std::path::Path;

/// Parsed application config.
#[derive(Debug, Default, serde::Deserialize)]
pub(crate) struct Config {
    /// Expected Arma 3 game version (e.g. "2.20"). When set, the engine compares
    /// it to the arma3-wiki data version and logs a warning on mismatch.
    pub game_version: Option<String>,
}

impl Config {
    /// Load config from `$A3SQL_CONFIG` or `./a3sql.toml`. Returns default on
    /// missing file or parse error (config is advisory, not required).
    pub fn load() -> Self {
        let path = std::env::var("A3SQL_CONFIG")
            .ok()
            .map(|p| Path::new(&p).to_path_buf())
            .or_else(|| {
                let cwd = std::env::current_dir().ok()?;
                let p = cwd.join("a3sql.toml");
                if p.exists() {
                    Some(p)
                } else {
                    None
                }
            });

        match path {
            Some(p) => match std::fs::read_to_string(&p) {
                Ok(content) => match toml::from_str(&content) {
                    Ok(cfg) => cfg,
                    Err(e) => {
                        eprintln!("[a3sql] config parse error ({}): {}", p.display(), e);
                        Config::default()
                    }
                },
                Err(e) => {
                    eprintln!("[a3sql] config read error ({}): {}", p.display(), e);
                    Config::default()
                }
            },
            None => Config::default(),
        }
    }
}
