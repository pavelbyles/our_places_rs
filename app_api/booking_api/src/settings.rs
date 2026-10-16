//! Functions to read settings file

use config::{Config, ConfigError, File};
use serde::Deserialize;
use std::fmt;

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
    pub cloud: bool,
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
                "postgres://{}:{}@localhost:5432/{}?host=/cloudsql/{}",
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

/// Tax compliance settings identifying the Merchant of Record.
#[derive(Debug, Clone, Deserialize)]
pub struct TaxSettings {
    pub taj_supplier_trn: String,
    pub supplier_name: String,
    pub supplier_address: String,
}

impl TaxSettings {
    /// Loads and validates statutory tax settings from environment variables.
    /// Fails fast if `TAJ_SUPPLIER_TRN` is missing or empty.
    pub fn from_env() -> Result<Self, String> {
        Self::from_lookup(|k| std::env::var(k).ok())
    }

    /// Loads and validates statutory tax settings from a key-value lookup function.
    pub fn from_lookup<F>(lookup: F) -> Result<Self, String>
    where
        F: Fn(&str) -> Option<String>,
    {
        let trn = lookup("TAJ_SUPPLIER_TRN").ok_or_else(|| {
            "TAJ_SUPPLIER_TRN is required for statutory tax invoicing".to_string()
        })?;
        if trn.trim().is_empty() {
            return Err("TAJ_SUPPLIER_TRN is required for statutory tax invoicing".to_string());
        }
        let supplier_name =
            lookup("TAJ_SUPPLIER_NAME").unwrap_or_else(|| "Our Places Jamaica Limited".to_string());
        let supplier_address = lookup("TAJ_SUPPLIER_ADDRESS")
            .unwrap_or_else(|| "Main Street, Ocho Rios, St. Ann, Jamaica".to_string());
        Ok(Self {
            taj_supplier_trn: trn,
            supplier_name,
            supplier_address,
        })
    }

    /// Converts into `db_core::models::SupplierInfo`.
    pub fn to_supplier_info(&self) -> db_core::models::SupplierInfo {
        db_core::models::SupplierInfo {
            supplier_name: self.supplier_name.clone(),
            supplier_address: self.supplier_address.clone(),
            supplier_trn: self.taj_supplier_trn.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tax_settings_missing_trn_fails() {
        let err = TaxSettings::from_lookup(|_| None).unwrap_err();
        assert!(err.contains("TAJ_SUPPLIER_TRN is required"));
    }

    #[test]
    fn test_tax_settings_empty_trn_fails() {
        let err = TaxSettings::from_lookup(|k| {
            if k == "TAJ_SUPPLIER_TRN" {
                Some("   ".to_string())
            } else {
                None
            }
        })
        .unwrap_err();
        assert!(err.contains("TAJ_SUPPLIER_TRN is required"));
    }

    #[test]
    fn test_tax_settings_valid_trn_succeeds() {
        let settings = TaxSettings::from_lookup(|k| {
            if k == "TAJ_SUPPLIER_TRN" {
                Some("123-456-789".to_string())
            } else {
                None
            }
        })
        .unwrap();
        assert_eq!(settings.taj_supplier_trn, "123-456-789");
        assert_eq!(settings.supplier_name, "Our Places Jamaica Limited");
        let supplier = settings.to_supplier_info();
        assert_eq!(supplier.supplier_trn, "123-456-789");
    }
}
