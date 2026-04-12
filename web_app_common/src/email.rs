use lettre::transport::smtp::authentication::Credentials;
use lettre::{Message, Transport};
use std::env;

const VERIFICATION_TEMPLATE: &str = include_str!("../templates/verification_email.html");

pub async fn send_verification_email(
    to_email: &str,
    first_name: &str,
    otp: &str,
) -> anyhow::Result<()> {
    // For local development, we use msmtp if SMTP_HOST is not configured
    let smtp_host = env::var("SMTP_HOST").unwrap_or_default();

    let html_content = VERIFICATION_TEMPLATE
        .replace("{{first_name}}", first_name)
        .replace("{{otp}}", otp);

    tracing::info!("Sending verification email to {} (OTP: {})", to_email, otp);

    let from_email = env::var("SMTP_FROM").unwrap_or_else(|_| "noreply@ourplaces.io".to_string());

    let email = Message::builder()
        .from(from_email.parse()?)
        .to(to_email.parse()?)
        .subject("Verify Your Email - Our Places")
        .header(lettre::message::header::ContentType::TEXT_HTML)
        .body(html_content)?;

    if !smtp_host.is_empty() {
        // SMTP Configuration (Production)
        let smtp_user = env::var("SMTP_USER")?;
        let smtp_pass = env::var("SMTP_PASS")?;

        let mailer = lettre::transport::smtp::SmtpTransport::relay(&smtp_host)?
            .credentials(Credentials::new(smtp_user, smtp_pass))
            .build();

        let email_copy = email.clone();
        tokio::task::spawn_blocking(move || -> Result<lettre::transport::smtp::response::Response, lettre::transport::smtp::Error> {
            mailer.send(&email_copy)
        }).await.map_err(|e| anyhow::anyhow!("Spawn blocking failed: {}", e))??;
    } else {
        // msmtp Configuration (Local/Non-production)
        tracing::info!("Using msmtp for email delivery");
        use lettre::transport::sendmail::SendmailTransport;

        let mailer = SendmailTransport::new();

        let email_copy = email.clone();
        tokio::task::spawn_blocking(move || mailer.send(&email_copy))
            .await
            .map_err(|e| anyhow::anyhow!("Spawn blocking failed: {}", e))??;
    }

    Ok(())
}
