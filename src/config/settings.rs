use config::{Config, ConfigError, Environment, File};
use kubuno_storage::StorageConfig;
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct Settings {
    pub server:    ServerSettings,
    pub core:      CoreSettings,
    pub database:  DatabaseSettings,
    pub storage:   StorageConfig,
    pub keestore:  KeestoreSettings,
    pub hibp:      HibpSettings,
    pub logging:   LoggingSettings,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ServerSettings {
    pub host: String,
    pub port: u16,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CoreSettings {
    pub url:             String,
    pub internal_secret: String,
}

/// The `[database]` section is owned by kubuno-db: which of its fields matter
/// depends on the engine the binary was built for, and the pool is opened by
/// `kubuno_db::connect`.
pub use kubuno_db::DbSettings as DatabaseSettings;

#[derive(Debug, Clone, Deserialize)]
pub struct KeestoreSettings {
    pub max_kdbx_size_bytes: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct HibpSettings {
    pub api_url: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LoggingSettings {
    pub level:  String,
    pub format: LogFormat,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum LogFormat {
    Pretty,
    Json,
}

impl Settings {
    pub fn load() -> Result<Self, ConfigError> {
        let mut builder = Config::builder()
            .set_default("server.host", "127.0.0.1")?
            .set_default("server.port", 3114)?
            .set_default("core.url", "http://127.0.0.1:8080")?
            .set_default("core.internal_secret", "")?
            .set_default("database.max_connections", 5u64)?
            .set_default("database.min_connections", 1u64)?
            .set_default("database.connect_timeout", 10u64)?
            .set_default("database.run_migrations", true)?
            // SQLite only: where `<schema>.sqlite` lives.
            .set_default("database.path", "./data/db")?
            .set_default("storage.backend", "local")?
            .set_default("storage.local_path", "./data/vaults")?
            .set_default("storage.temp_path", "./data/temp")?
            .set_default("keestore.max_kdbx_size_bytes", 52_428_800u64)? // 50 MB
            .set_default("hibp.api_url", "https://api.pwnedpasswords.com/range")?
            .set_default("logging.level", "info")?
            .set_default("logging.format", "pretty")?;

        // Fichier de config
        if let Ok(path) = std::env::var("KKS_CONFIG_FILE") {
            builder = builder.add_source(File::with_name(&path).required(true));
        } else {
            builder = builder.add_source(File::with_name("config").required(false));
        }

        // Variables d'environnement injectées par le superviseur Kubuno
        builder = builder
            .set_override_option("database.host",     std::env::var("KUBUNO_DB_HOST").ok())?
            .set_override_option("database.port",     std::env::var("KUBUNO_DB_PORT").ok()
                                                        .and_then(|v| v.parse::<u64>().ok().map(|n| n.to_string())))?
            .set_override_option("database.user",     std::env::var("KUBUNO_DB_USER").ok())?
            .set_override_option("database.password", std::env::var("KUBUNO_DB_PASSWORD").ok())?
            .set_override_option("database.database", std::env::var("KUBUNO_DB_NAME").ok())?
            .set_override_option("database.path",     std::env::var("KUBUNO_DB_PATH").ok())?
            .set_override_option("core.internal_secret", std::env::var("KUBUNO_INTERNAL_SECRET").ok())?
            .set_override_option("core.url",          std::env::var("KUBUNO_CORE_URL").ok())?;

        // Variables d'environnement du module (KKS__)
        builder = builder.add_source(
            Environment::with_prefix("KKS")
                .separator("__")
                .try_parsing(true),
        );

        builder.build()?.try_deserialize()
    }
}
