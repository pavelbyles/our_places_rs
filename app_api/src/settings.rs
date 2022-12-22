use std::fmt;

use config::{Config, ConfigError, File};
use serde::Deserialize;

#[derive(Debug, Deserialize, Clone)]
pub struct Server {
    pub host: String,
    pub port: u16,
}

#[derive(Debug, Deserialize, Clone)]
pub struct Log {
    pub level: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct Settings {
    pub server: Server,
    pub log: Log,
    pub env: Env,
}

const CONFIG_FILE_PATH: &str = "./config/Default.toml";
const CONFIG_FILE_PREFIX: &str = "./config/";

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

impl Settings {
    pub fn new() -> Result<Self, ConfigError> {
        static ENV_RUN_VAR: &str = "RUN_ENV";
        static ENV_RUN_VAL: &str = "Development";

        let env = std::env::var(ENV_RUN_VAR).unwrap_or_else(|_| ENV_RUN_VAL.into());
        let s = Config::builder()
            .set_override("env", env.clone())?
            .add_source(File::with_name(CONFIG_FILE_PATH))
            .add_source(File::with_name(&format!("{}{}", CONFIG_FILE_PREFIX, env)))
            // This makes it so "EA_SERVER__PORT overrides server.port
            .add_source(config::Environment::with_prefix("ea").separator("__"))
            .build()?;

        s.try_deserialize()
    }
}
