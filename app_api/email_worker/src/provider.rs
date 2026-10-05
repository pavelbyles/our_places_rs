use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use tokio::sync::Mutex;
use tracing::{error, info};

#[async_trait::async_trait]
pub trait EmailProvider: Send + Sync {
    async fn send_email(&self, recipient: &str, subject: &str, body: &str) -> Result<(), String>;
}

/// Mock email provider used in development and tests.
pub struct MockEmailProvider {
    pub sent_emails: Arc<Mutex<Vec<(String, String, String)>>>,
    pub fail_count: AtomicU32,
}

impl Default for MockEmailProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl MockEmailProvider {
    pub fn new() -> Self {
        Self {
            sent_emails: Arc::new(Mutex::new(Vec::new())),
            fail_count: AtomicU32::new(0),
        }
    }

    pub fn with_failures(fail_attempts: u32) -> Self {
        Self {
            sent_emails: Arc::new(Mutex::new(Vec::new())),
            fail_count: AtomicU32::new(fail_attempts),
        }
    }

    pub async fn get_sent_emails(&self) -> Vec<(String, String, String)> {
        self.sent_emails.lock().await.clone()
    }
}

#[async_trait::async_trait]
impl EmailProvider for MockEmailProvider {
    async fn send_email(&self, recipient: &str, subject: &str, body: &str) -> Result<(), String> {
        let current_fails = self.fail_count.load(Ordering::SeqCst);
        if current_fails > 0 {
            self.fail_count.fetch_sub(1, Ordering::SeqCst);
            let err = "Simulated mock provider transmission failure".to_string();
            info!(
                "[MOCK EMAIL FAILED] To: {}, Subject: {} - {}",
                recipient, subject, err
            );
            return Err(err);
        }

        info!(
            "\n╔══════════════════════════════════════════════════════════════════════════════╗\n║ [MOCK EMAIL DISPATCH]                                                        ║\n╠══════════════════════════════════════════════════════════════════════════════╣\n║ To:      {}\n║ Subject: {}\n╠────────────────────────────────── Body ──────────────────────────────────────╣\n{}\n╚══════════════════════════════════════════════════════════════════════════════╝",
            recipient, subject, body
        );

        // Also save rendered HTML to temporary directory for easy visual inspection in a browser
        let email_dir = std::env::temp_dir().join("our_places_emails");
        if let Err(e) = std::fs::create_dir_all(&email_dir) {
            tracing::warn!("Failed to create email preview directory: {}", e);
        } else {
            let sanitized_subject: String = subject
                .chars()
                .map(|c| if c.is_alphanumeric() { c } else { '_' })
                .collect();
            let file_path = email_dir.join(format!(
                "{}_{}.html",
                chrono::Utc::now().format("%Y%m%d_%H%M%S"),
                sanitized_subject
            ));
            if let Err(e) = std::fs::write(&file_path, body) {
                tracing::warn!("Failed to write mock email to {:?}: {}", file_path, e);
            } else {
                info!("Saved mock email HTML preview to: {:?}", file_path);
            }
        }

        self.sent_emails.lock().await.push((
            recipient.to_string(),
            subject.to_string(),
            body.to_string(),
        ));
        Ok(())
    }
}

/// Lettre SMTP email provider for production environments.
pub struct LettreSmtpEmailProvider {
    smtp_host: String,
    smtp_port: u16,
    smtp_user: Option<String>,
    smtp_password: Option<String>,
    from_email: String,
}

impl LettreSmtpEmailProvider {
    pub fn new(
        smtp_host: String,
        smtp_port: u16,
        smtp_user: Option<String>,
        smtp_password: Option<String>,
        from_email: String,
    ) -> Self {
        Self {
            smtp_host,
            smtp_port,
            smtp_user,
            smtp_password,
            from_email,
        }
    }

    /// Constructs an SMTP email provider instance from configured environment variables.
    pub fn from_env() -> Self {
        static SMTP_HOST_ENV: &str = "SMTP_HOST";
        static SMTP_PORT_ENV: &str = "SMTP_PORT";
        static SMTP_USERNAME_ENV: &str = "SMTP_USERNAME";
        static SMTP_USER_ENV: &str = "SMTP_USER";
        static SMTP_PASSWORD_ENV: &str = "SMTP_PASSWORD";
        static SMTP_PASS_ENV: &str = "SMTP_PASS";
        static SMTP_FROM_EMAIL_ENV: &str = "SMTP_FROM_EMAIL";
        static SMTP_FROM_ENV: &str = "SMTP_FROM";

        let smtp_host = std::env::var(SMTP_HOST_ENV).unwrap_or_else(|_| "localhost".to_string());
        let smtp_port = std::env::var(SMTP_PORT_ENV)
            .ok()
            .and_then(|p| p.parse().ok())
            .unwrap_or(587);
        let smtp_user = std::env::var(SMTP_USERNAME_ENV)
            .or_else(|_| std::env::var(SMTP_USER_ENV))
            .ok();
        let smtp_password = std::env::var(SMTP_PASSWORD_ENV)
            .or_else(|_| std::env::var(SMTP_PASS_ENV))
            .ok();
        let from_email = std::env::var(SMTP_FROM_EMAIL_ENV)
            .or_else(|_| std::env::var(SMTP_FROM_ENV))
            .unwrap_or_else(|_| "no-reply@ourplaces.io".to_string());

        Self::new(smtp_host, smtp_port, smtp_user, smtp_password, from_email)
    }
}

#[async_trait::async_trait]
impl EmailProvider for LettreSmtpEmailProvider {
    async fn send_email(&self, recipient: &str, subject: &str, body: &str) -> Result<(), String> {
        use lettre::message::header::ContentType;
        use lettre::transport::smtp::authentication::Credentials;
        use lettre::{AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor};

        let email = Message::builder()
            .from(
                self.from_email
                    .parse()
                    .map_err(|e| format!("Invalid from address: {}", e))?,
            )
            .to(recipient
                .parse()
                .map_err(|e| format!("Invalid recipient address: {}", e))?)
            .subject(subject)
            .header(ContentType::TEXT_HTML)
            .body(body.to_string())
            .map_err(|e| format!("Failed to build email message: {}", e))?;

        let mut transport_builder =
            if self.smtp_host == "localhost" || self.smtp_host == "127.0.0.1" {
                AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(&self.smtp_host)
                    .port(self.smtp_port)
            } else if self.smtp_port == 465 {
                AsyncSmtpTransport::<Tokio1Executor>::relay(&self.smtp_host)
                    .map_err(|e| format!("Failed to create SMTPS relay transport: {}", e))?
                    .port(self.smtp_port)
            } else {
                AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(&self.smtp_host)
                    .map_err(|e| format!("Failed to create STARTTLS relay transport: {}", e))?
                    .port(self.smtp_port)
            };

        if let (Some(user), Some(pass)) = (&self.smtp_user, &self.smtp_password) {
            transport_builder =
                transport_builder.credentials(Credentials::new(user.clone(), pass.clone()));
        }

        let transport = transport_builder.build();

        transport.send(email).await.map_err(|e| {
            error!("SMTP dispatch failure: {}", e);
            format!("SMTP error: {}", e)
        })?;

        info!("SMTP email sent successfully to {}", recipient);
        Ok(())
    }
}
