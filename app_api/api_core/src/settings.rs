//! Functions to read settings file

use config::{Config, ConfigError, File};
use serde::Deserialize;
use std::fmt;

// Overarching settings model
#[derive(Debug, Deserialize, Clone)]
pub struct Settings {
    pub server: Server,
    pub database: DatabaseSettings,
    pub log: Log,
    pub env: Env,
    pub application: Application,
}

// Application settings model
#[derive(Debug, Deserialize, Clone)]
pub struct Application {
    pub max_attempts: u32,
}

// Server model
#[derive(Debug, Deserialize, Clone)]
pub struct Server {
    pub host: String,
    pub port: u16,
}

// Database settings model
#[derive(Debug, Deserialize, Clone)]
pub struct DatabaseSettings {
    pub username: String,
    pub password: String,
    pub port: u16,
    pub host: String,
    pub database_name: String,
    pub cloud: bool,
    pub instance_name: String,
}

// Log level model
#[derive(Debug, Deserialize, Clone)]
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

impl TryFrom<String> for Env {
    type Error = String;

    fn try_from(s: String) -> Result<Self, Self::Error> {
        match s.to_lowercase().as_str() {
            "development" => Ok(Self::Development),
            "testing" => Ok(Self::Testing),
            "production" => Ok(Self::Production),
            other => Err(format!(
                "{} is not a supported environment. Use either 'Development', 'Testing' or 'Production'.",
                other
            )),
        }
    }
}

impl DatabaseSettings {
    /// Builds the correct connection string based on the configuration.
    /// It will build a standard TCP connection string for local dev,
    /// and a Unix socket connection string when `cloud` is "YES".
    pub fn connection_string(&self) -> String {
        if self.cloud {
            // Build the Cloud SQL Unix socket connection string
            format!(
                "postgres://{}:{}@/{}?host=/cloudsql/{}/.s.PGSQL.5432",
                self.username, self.password, self.database_name, self.instance_name
            )
        } else {
            // Build the regular TCP connection string for local/other environments
            format!(
                "postgres://{}:{}@{}:{}/{}",
                self.username, self.password, self.host, self.port, self.database_name
            )
        }
    }

    #[allow(dead_code)]
    pub fn connection_string_without_db(&self) -> String {
        format!(
            "postgres://{}:{}@{}:{}",
            self.username, self.password, self.host, self.port
        )
    }
}

// Load settings
// ... (structs and impls remain the same) ...

// This is a cleaned-up version of YOUR original function.
pub fn get_settings() -> Result<Settings, ConfigError> {
    const DEFAULT_CONFIG_FILE: &str = "config/Default.toml";
    const ENV_CONFIG_PREFIX: &str = "config/";
    const RUN_ENV_VAR: &str = "RUN_ENV";
    const DEFAULT_RUN_ENV: &str = "Development";

    // 1. Determine the current run environment.
    let run_env_str = std::env::var(RUN_ENV_VAR).unwrap_or_else(|_| DEFAULT_RUN_ENV.into());

    let environment: Env = run_env_str
        .clone()
        .try_into()
        .map_err(|e: String| ConfigError::Message(e))?;

    // 2. Build the path for the environment-specific configuration file.
    let env_config_file = format!("{}{}.toml", ENV_CONFIG_PREFIX, environment);

    let settings = Config::builder()
        // Start with the base configuration file.
        .add_source(File::with_name(DEFAULT_CONFIG_FILE))
        // Layer on the environment-specific configuration.
        // .required(false) makes it non-fatal if a file for an env doesn't exist.
        .add_source(File::with_name(&env_config_file).required(false))
        // Override the 'env' field to ensure it reflects the correct, active environment.
        .set_override("env", run_env_str)?
        // Override with any environment variables (e.g., for production secrets).
        .add_source(config::Environment::with_prefix("EA").separator("__"))
        .build()?;

    settings.try_deserialize()
}
