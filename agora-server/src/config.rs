use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct Config {
    pub server: ServerConfig,
    pub database: DatabaseConfig,
    #[serde(default)]
    pub media: MediaConfig,
}

#[derive(Debug, Deserialize)]
pub struct ServerConfig {
    pub bind: String,
    pub server_name: String,
}

#[derive(Debug, Deserialize)]
pub struct DatabaseConfig {
    pub backend: String,
    pub uri: String,
}

#[derive(Debug, Deserialize)]
pub struct MediaConfig {
    #[serde(default = "default_media_store_path")]
    pub store_path: String,
    #[serde(default = "default_max_upload_bytes")]
    pub max_upload_bytes: u64,
}

impl Default for MediaConfig {
    fn default() -> Self {
        Self {
            store_path: default_media_store_path(),
            max_upload_bytes: default_max_upload_bytes(),
        }
    }
}

fn default_media_store_path() -> String {
    "media_store".to_owned()
}

fn default_max_upload_bytes() -> u64 {
    50 * 1024 * 1024 // 50 MiB
}

impl Config {
    /// Load config from a TOML file, falling back to sensible defaults.
    pub fn load(path: Option<&str>) -> anyhow::Result<Self> {
        if let Some(path) = path {
            let content = std::fs::read_to_string(path)?;
            let config: Config = toml::from_str(&content)?;
            return Ok(config);
        }

        // Try default paths.
        for candidate in &["agora.toml", "config.toml"] {
            if let Ok(content) = std::fs::read_to_string(candidate) {
                if let Ok(config) = toml::from_str(&content) {
                    return Ok(config);
                }
            }
        }

        // Fall back to defaults.
        Ok(Config {
            server: ServerConfig {
                bind: "127.0.0.1:8008".to_owned(),
                server_name: "localhost".to_owned(),
            },
            database: DatabaseConfig {
                backend: "sqlite".to_owned(),
                uri: "sqlite:agora.db?mode=rwc".to_owned(),
            },
            media: MediaConfig::default(),
        })
    }
}
