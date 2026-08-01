// a3sql config — version validation, startup options from a3sql.toml
//
// Reads `$A3SQL_CONFIG` or `./a3sql.toml` for an optional game_version override,
// public_key for Ed25519 query signing, and auth_required flag.
// If the wiki data version differs from the configured game version, the engine
// logs a drift warning at startup so users know their command database might
// not match their installed Arma 3 version.

use std::path::Path;
use std::sync::LazyLock;

/// Cached application config, loaded once on first access.
#[allow(dead_code, reason = "phased auth implementation")]
pub(crate) static CONFIG: LazyLock<Config> = LazyLock::new(Config::load);

/// Parsed application config.
#[derive(Debug, Default, serde::Deserialize)]
pub(crate) struct Config {
    /// Expected Arma 3 game version (e.g. "2.20"). When set, the engine compares
    /// it to the arma3-wiki data version and logs a warning on mismatch.
    pub game_version: Option<String>,

    /// Hex‑encoded Ed25519 public key (64 hex chars). Required when
    /// `auth_required` is `true` and the `auth` feature is enabled.
    #[allow(dead_code, reason = "phased auth implementation")]
    pub public_key: Option<String>,

    /// Require every query to carry a `SIGNED <sig> <query>` prefix with a
    /// valid Ed25519 signature. Default: `false`.
    #[allow(dead_code, reason = "phased auth implementation")]
    pub auth_required: Option<bool>,

    /// When `true`, the TCP listener rejects anonymous connections — a
    /// `LOGIN <user> <pass>` is mandatory even if CBA credentials are empty.
    /// Intended for shared/dedicated hosts where any local process could
    /// otherwise connect. Default: `true` (fail-closed — the listener refuses
    /// connections when no credentials are configured; set this to `false`
    /// only for trusted loopback-only deployments that want anonymous access).
    pub listener_require_auth: Option<bool>,

    /// Directory for file I/O commands (SAVE, LOAD, export_to_file).
    /// Defaults to `./a3sql_data/` when not set.
    pub data_dir: Option<String>,
}

impl Config {
    /// Whether the TCP listener requires LOGIN auth on every connection.
    /// True by default (fail-closed): when `listener_require_auth` is unset or
    /// true, or when credentials are configured (non-empty creds force LOGIN).
    pub(crate) fn listener_auth_required(&self) -> bool {
        self.listener_require_auth.unwrap_or(true)
    }

    /// Whether auth verification is required.
    #[allow(dead_code, reason = "phased auth implementation")]
    pub(crate) fn auth_enabled(&self) -> bool {
        #[cfg(feature = "auth")]
        {
            self.auth_required.unwrap_or(false)
        }
        #[cfg(not(feature = "auth"))]
        {
            false
        }
    }

    /// The configured Ed25519 public key bytes, if any.
    #[allow(dead_code, reason = "phased auth implementation")]
    pub(crate) fn public_key_bytes(&self) -> Option<[u8; 32]> {
        #[cfg(feature = "auth")]
        {
            self.public_key.as_ref().and_then(|hex| crate::auth::hex_to_pubkey(hex))
        }
        #[cfg(not(feature = "auth"))]
        {
            None
        }
    }

    /// The directory for file I/O (SAVE, LOAD, export_to_file).
    /// Defaults to `./a3sql_data/`.
    pub(crate) fn data_dir(&self) -> &std::path::Path {
        match self.data_dir.as_deref() {
            Some(d) => std::path::Path::new(d),
            None => std::path::Path::new("./a3sql_data"),
        }
    }

    /// Load config from `$A3SQL_CONFIG` or `./a3sql.toml`. Returns default on
    /// missing file or parse error (config is advisory, not required).
    pub fn load() -> Self {
        let path = std::env::var("A3SQL_CONFIG")
            .ok()
            .map(|p| Path::new(&p).to_path_buf())
            .or_else(|| {
                let cwd = std::env::current_dir().ok()?;
                let p = cwd.join("a3sql.toml");
                if p.exists() { Some(p) } else { None }
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
