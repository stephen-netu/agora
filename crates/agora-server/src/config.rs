use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct Config {
    pub server: ServerConfig,
    pub database: DatabaseConfig,
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
        })
    }
}
