use lettre::transport::smtp::authentication::Credentials;
use lettre::{Message, Transport};
use std::env;

const VERIFICATION_TEMPLATE: &str = include_str!("../templates/verification_email.html");
const BOOKING_CONFIRMATION_TEMPLATE: &str = include_str!("../templates/booking_confirmation.html");

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

pub async fn send_booking_confirmation(
    to_email: &str,
    first_name: &str,
    listing_name: &str,
    confirmation_code: &str,
    check_in: &str,
    check_out: &str,
    guests: i32,
) -> anyhow::Result<()> {
    let smtp_host = env::var("SMTP_HOST").unwrap_or_default();
    let site_url = env::var("SITE_URL").unwrap_or_else(|_| "http://localhost:3000".to_string());

    let html_content = BOOKING_CONFIRMATION_TEMPLATE
        .replace("{{first_name}}", first_name)
        .replace("{{listing_name}}", listing_name)
        .replace("{{confirmation_code}}", confirmation_code)
        .replace("{{check_in}}", check_in)
        .replace("{{check_out}}", check_out)
        .replace("{{guests}}", &guests.to_string())
        .replace("{{site_url}}", &site_url);

    tracing::info!("Sending booking confirmation email to {}", to_email);

    let from_email = env::var("SMTP_FROM").unwrap_or_else(|_| "noreply@ourplaces.io".to_string());

    let email = Message::builder()
        .from(from_email.parse()?)
        .to(to_email.parse()?)
        .subject(format!("Booking Confirmed: {}", listing_name))
        .header(lettre::message::header::ContentType::TEXT_HTML)
        .body(html_content)?;

    if !smtp_host.is_empty() {
        let smtp_user = env::var("SMTP_USER")?;
        let smtp_pass = env::var("SMTP_PASS")?;

        let mailer = lettre::transport::smtp::SmtpTransport::relay(&smtp_host)?
            .credentials(Credentials::new(smtp_user, smtp_pass))
            .build();

        let email_copy = email.clone();
        tokio::task::spawn_blocking(move || mailer.send(&email_copy))
            .await
            .map_err(|e| anyhow::anyhow!("Spawn blocking failed: {}", e))??;
    } else {
        tracing::info!("Using msmtp for booking confirmation");
        use lettre::transport::sendmail::SendmailTransport;
        let mailer = SendmailTransport::new();
        let email_copy = email.clone();
        tokio::task::spawn_blocking(move || mailer.send(&email_copy))
            .await
            .map_err(|e| anyhow::anyhow!("Spawn blocking failed: {}", e))??;
    }

    Ok(())
}
