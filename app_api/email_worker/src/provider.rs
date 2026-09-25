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
            "[MOCK EMAIL DISPATCH] To: {}, Subject: {}",
            recipient, subject
        );
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

    pub fn from_env() -> Self {
        let smtp_host = std::env::var("SMTP_HOST").unwrap_or_else(|_| "localhost".to_string());
        let smtp_port = std::env::var("SMTP_PORT")
            .ok()
            .and_then(|p| p.parse().ok())
            .unwrap_or(587);
        let smtp_user = std::env::var("SMTP_USERNAME").ok();
        let smtp_password = std::env::var("SMTP_PASSWORD").ok();
        let from_email = std::env::var("SMTP_FROM_EMAIL")
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
            AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(&self.smtp_host)
                .port(self.smtp_port);

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
