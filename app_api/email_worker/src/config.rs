#[derive(Debug, Clone)]
pub struct WorkerConfig {
    pub pubsub_secret_token: Option<String>,
    pub max_retries: i32,
    pub use_mock_provider: bool,
}

impl Default for WorkerConfig {
    fn default() -> Self {
        Self::from_env()
    }
}

impl WorkerConfig {
    pub fn from_env() -> Self {
        let pubsub_secret_token = std::env::var("PUBSUB_SECRET_TOKEN").ok();
        let max_retries = std::env::var("MAX_EMAIL_RETRIES")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(3);
        let use_mock_provider = std::env::var("USE_MOCK_EMAIL_PROVIDER")
            .map(|v| v == "true" || v == "1")
            .unwrap_or_else(|_| {
                // By default in development or test, mock provider is enabled if no SMTP_HOST is provided
                std::env::var("SMTP_HOST").is_err()
            });

        Self {
            pubsub_secret_token,
            max_retries,
            use_mock_provider,
        }
    }
}
