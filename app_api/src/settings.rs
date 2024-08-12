//! Functions to read settings file

use std::fmt;

use config::{Config, ConfigError, File};
use serde::Deserialize;

const CONFIG_FILE_PATH: &str = "./config/Default.toml";
const CONFIG_FILE_PREFIX: &str = "./config/";

// Overarching settings model
#[derive(Debug, Deserialize)]
pub struct Settings {
    pub server: Server,
    pub database: DatabaseSettings,
    pub log: Log,
    pub env: Env,
}

// Server model
#[derive(Debug, Deserialize)]
pub struct Server {
    pub host: String,
    pub port: u16,
}

// Database settings model
#[derive(Debug, Deserialize)]
pub struct DatabaseSettings {
    pub username: String,
    pub password: String,
    pub port: u16,
    pub host: String,
    pub database_name: String,
    pub cloud: String,
    pub instance_name: String,
}

// Log level model
#[derive(Debug, Deserialize)]
pub struct Log {
    pub level: String,
}

// Environment model
#[derive(Clone, Debug, Deserialize)]
pub enum Env {
    Development,
    Testing,
    Production,
}

impl fmt::Display for Env {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Env::Development => write!(f, "Development"),
            Env::Testing => write!(f, "Testing"),
            Env::Production => write!(f, "Production"),
        }
    }
}

impl From<&str> for Env {
    fn from(env: &str) -> Self {
        match env {
            "Testing" => Env::Testing,
            "Production" => Env::Production,
            _ => Env::Development,
        }
    }
}

impl DatabaseSettings {

    /// Return the appropriate connection string based on if running in the cloud or not.
    /// Cloud connectivity to CloudSql is using the Unix socket method.
    pub fn connection_string(&self) -> String {
        // Cloud SQL uses Unix socket while local uses hostname
        if (self.cloud == "YES") {
            println!("postgres://{}:{}@localhost:5432/{}?host=/cloudsql/{}",
                     self.username, self.password, self.database_name, self.instance_name
            );
            return format!("postgres://{}:{}@localhost:5432/{}?host=/cloudsql/{}",
                           self.username, self.password, self.database_name, self.instance_name
            )
        }

        format!(
            "postgres://{}:{}@{}:{}/{}",
            self.username, self.password, self.host, self.port, self.database_name
        )
    }

    pub fn connection_string_without_db(&self) -> String {
        format!(
            "postgres://{}:{}@{}:{}",
            self.username, self.password, self.host, self.port
        )
    }
}

// Load settings
impl Settings {
    pub fn new() -> Result<Self, ConfigError> {
        static ENV_RUN_VAR: &str = "RUN_ENV"; // Environment var
        static ENV_RUN_VAL: &str = "Development"; // Default environment variable value

        // determine the run environment var
        let env = std::env::var(ENV_RUN_VAR).unwrap_or_else(|_| ENV_RUN_VAL.into());

        let s = Config::builder()
            .set_override("env", env.clone())?
            .add_source(File::with_name(CONFIG_FILE_PATH))
            .add_source(File::with_name(&format!("{}{}", CONFIG_FILE_PREFIX, env)))
            // This makes it so "EA_SERVER__PORT overrides server.port
            .add_source(config::Environment::with_prefix("EA").separator("__"))
            .build()?;

        s.try_deserialize()
    }
}

pub fn get_settings() -> Result<Settings, config::ConfigError> {
    Settings::new()
}
