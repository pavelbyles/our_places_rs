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
    /// Loads worker configuration parameters from environment variables with safe defaults.
    pub fn from_env() -> Self {
        static PUBSUB_SECRET_TOKEN_ENV: &str = "PUBSUB_SECRET_TOKEN";
        static MAX_EMAIL_RETRIES_ENV: &str = "MAX_EMAIL_RETRIES";
        static USE_MOCK_EMAIL_PROVIDER_ENV: &str = "USE_MOCK_EMAIL_PROVIDER";
        static SMTP_HOST_ENV: &str = "SMTP_HOST";

        let pubsub_secret_token = std::env::var(PUBSUB_SECRET_TOKEN_ENV).ok();
        let max_retries = std::env::var(MAX_EMAIL_RETRIES_ENV)
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(3);
        let use_mock_provider = std::env::var(USE_MOCK_EMAIL_PROVIDER_ENV)
            .map(|v| v == "true" || v == "1")
            .unwrap_or_else(|_| {
                // By default in development or test, mock provider is enabled if no SMTP_HOST is provided
                std::env::var(SMTP_HOST_ENV).is_err()
            });

        Self {
            pubsub_secret_token,
            max_retries,
            use_mock_provider,
        }
    }
}
