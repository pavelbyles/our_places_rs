//! Functions to read settings file

use config::{Config, ConfigError, File};
use serde::Deserialize;
use std::fmt;

// Overarching settings model
/// A service's configuration.
#[derive(Debug, Deserialize, Clone)]
pub struct Settings {
    /// Where the service listens.
    pub server: Server,
    /// Database connection.
    pub database: DatabaseSettings,
    /// Logging.
    pub log: Log,
    /// Environment it runs in.
    pub env: Env,
    /// Application behaviour.
    pub application: Application,
    /// Feature switches.
    pub feature_flags: FeatureFlags,
}

// Feature flags model
/// Feature switches.
#[derive(Debug, Deserialize, Clone)]
pub struct FeatureFlags {
    /// Whether listings can be permanently deleted.
    pub enable_hard_deletes: bool,
}

/// How long a `pending` booking holds its dates when no setting is given.
pub const DEFAULT_PENDING_HOLD_MINUTES: i32 = 120;

// Application settings model
/// Application behaviour settings.
#[derive(Debug, Deserialize, Clone)]
pub struct Application {
    /// Attempts allowed when generating a booking's confirmation code.
    pub max_attempts: u32,
    /// How long a new `pending` booking holds its dates, in minutes. Set via
    /// `[application] pending_hold_minutes` or `EA__APPLICATION__PENDING_HOLD_MINUTES`.
    #[serde(default = "default_pending_hold_minutes")]
    pub pending_hold_minutes: i32,
    /// Grace window, in seconds, within which an exchange rate submitted as
    /// "effective now" is not treated as backdated. Set via
    /// `[application] rate_effective_grace_seconds` or
    /// `EA__APPLICATION__RATE_EFFECTIVE_GRACE_SECONDS`; must be 1-300.
    #[serde(default = "default_rate_effective_grace_seconds")]
    pub rate_effective_grace_seconds: i64,
    /// Longest stay a quote or hold accepts, in nights. It only stops absurd
    /// date ranges: pricing loops per night, on a public endpoint. Set via
    /// `[application] max_stay_nights` or `EA__APPLICATION__MAX_STAY_NIGHTS`.
    #[serde(default = "default_max_stay_nights")]
    pub max_stay_nights: i64,
}

/// Longest stay accepted when no setting is given: about 10 years.
pub const DEFAULT_MAX_STAY_NIGHTS: i64 = 3650;

fn default_max_stay_nights() -> i64 {
    DEFAULT_MAX_STAY_NIGHTS
}

fn default_pending_hold_minutes() -> i32 {
    DEFAULT_PENDING_HOLD_MINUTES
}

/// Default grace window for "effective now" exchange rates, in seconds.
pub const DEFAULT_RATE_EFFECTIVE_GRACE_SECONDS: i64 = 60;

/// Largest allowed grace window: anything longer would amount to backdating.
pub const MAX_RATE_EFFECTIVE_GRACE_SECONDS: i64 = 300;

fn default_rate_effective_grace_seconds() -> i64 {
    DEFAULT_RATE_EFFECTIVE_GRACE_SECONDS
}

impl Application {
    /// Checks the stay limit, which must be positive or every booking would be
    /// refused.
    pub fn validate_max_stay_nights(&self) -> Result<(), String> {
        (self.max_stay_nights > 0).then_some(()).ok_or_else(|| {
            format!(
                "application.max_stay_nights must be positive, got {}",
                self.max_stay_nights
            )
        })
    }

    /// The rate grace window, or why the configured value is invalid.
    pub fn rate_effective_grace(&self) -> Result<chrono::Duration, String> {
        (1..=MAX_RATE_EFFECTIVE_GRACE_SECONDS)
            .contains(&self.rate_effective_grace_seconds)
            .then(|| chrono::Duration::seconds(self.rate_effective_grace_seconds))
            .ok_or_else(|| {
                format!(
                    "application.rate_effective_grace_seconds must be 1-{MAX_RATE_EFFECTIVE_GRACE_SECONDS}, got {}",
                    self.rate_effective_grace_seconds
                )
            })
    }
}

// Server model
/// Where the service listens.
#[derive(Debug, Deserialize, Clone)]
pub struct Server {
    /// Address to bind.
    pub host: String,
    /// Port to bind.
    pub port: u16,
}

// Database settings model
/// Database connection settings.
#[derive(Debug, Deserialize, Clone)]
pub struct DatabaseSettings {
    /// Database user.
    pub username: String,
    /// Database password.
    pub password: String,
    /// Database port.
    pub port: u16,
    /// Database host.
    pub host: String,
    /// Database name.
    pub database_name: String,
    /// Whether to connect through the Cloud SQL socket.
    pub cloud: bool,
    /// Cloud SQL instance, when `cloud` is set.
    pub instance_name: String,
    /// Full connection URL, if given.
    pub database_url: Option<String>,
}

// Log level model
/// Logging settings.
#[derive(Debug, Deserialize, Clone)]
pub struct Log {
    /// Log level filter.
    pub level: String,
}

// Environment model
/// The environment a service runs in.
#[derive(Clone, Debug, Deserialize)]
pub enum Env {
    /// A developer's machine.
    Development,
    /// Automated tests.
    Testing,
    /// The live service.
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
        if let Some(url) = &self.database_url {
            return url.clone();
        }
        if self.cloud {
            // Build the Cloud SQL Unix socket connection string
            format!(
                "postgres://{}:{}@localhost/{}?host=/cloudsql/{}",
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
}

// Load settings
// ... (structs and impls remain the same) ...

/// Resolves and loads application configuration settings based on the runtime environment.
pub fn get_settings() -> Result<Settings, ConfigError> {
    dotenvy::dotenv().ok();

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
