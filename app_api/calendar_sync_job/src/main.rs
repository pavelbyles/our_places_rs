//! Entry point of the calendar sync Cloud Run Job. Exits non-zero only when
//! the configuration or the database is unusable.

use anyhow::Context;
use api_core::settings::{self, CalendarSyncSettings, check_cloud_guard};
use calendar_sync_job::{JobConfig, JobDeps, run, run::RUN_DEADLINE};
use std::sync::Arc;

/// Env var: the admin portal's base URL, for links in emails.
const ADMIN_BASE_URL_ENV: &str = "ADMIN_BASE_URL";

async fn start() -> anyhow::Result<()> {
    let config = settings::get_settings().context("could not load settings")?;
    let calendar = CalendarSyncSettings::from_process_env().context("invalid calendar settings")?;
    check_cloud_guard(&calendar, |key| std::env::var(key).ok())
        .context("calendar settings refused on Cloud Run")?;
    let fetcher = api_core::calendar::fetcher_config(&calendar)
        .await
        .map_err(|e| anyhow::anyhow!("calendar fetcher misconfigured: {e}"))?;
    let job = JobConfig {
        database_url: config.database.connection_string(),
        concurrency: calendar.job_concurrency,
        deadline: RUN_DEADLINE,
        admin_base_url: std::env::var(ADMIN_BASE_URL_ENV)
            .unwrap_or_else(|_| "http://localhost:3012".to_string()),
    };
    let deps = JobDeps {
        publisher: Arc::new(api_core::email_publisher::EmailPublisher::from_env()),
        fetcher,
    };
    run(job, deps).await.map(|_| ())
}

#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();
    api_core::tracing_utils::init_subscriber(env!("CARGO_PKG_NAME"));
    let outcome = start().await;
    if let Err(e) = &outcome {
        tracing::error!(error = ?e, "calendar sync job failed");
    }
    api_core::tracing_utils::shutdown_telemetry().await;
    if outcome.is_err() {
        std::process::exit(1);
    }
}
