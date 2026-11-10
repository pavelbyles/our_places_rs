//! Sync orchestration tests (spec §1.4.5–7) against a local https server and
//! a test database.

use super::fetch::{FetcherConfig, GuardedFetcher};
use super::sync::{OutboxPublisher, SyncDeps, sync_feeds};
use super::test_server::{FixedResolver, Reply, TestServer, start};
use chrono::{Duration as Days, NaiveDate, Utc};
use common::calendar::dto::SyncOutcome;
use db_core::calendar::feeds::{FeedClaim, claim_for_listing};
use db_core::calendar::reconcile::{SyncContext, SyncTrigger};
use sqlx::PgPool;
use sqlx_db_tester::TestPg;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::time::Instant;
use uuid::Uuid;

const DATABASE_URL_ENV: &str = "DATABASE_URL";

fn test_db() -> TestPg {
    dotenvy::dotenv().ok();
    let url = std::env::var(DATABASE_URL_ENV)
        .unwrap_or_else(|_| "postgres://postgres:password@localhost:5432/our_places".into());
    TestPg::new(url, Path::new("../../db_core/migrations"))
}

fn future(days: i64) -> NaiveDate {
    common::reference::listing_today(Utc::now()) + Days::days(days)
}

fn ics(events: &[(&str, NaiveDate, NaiveDate)]) -> Vec<u8> {
    let mut out = String::from("BEGIN:VCALENDAR\r\nVERSION:2.0\r\n");
    for (uid, from, to) in events {
        out.push_str(&format!(
            "BEGIN:VEVENT\r\nUID:{uid}\r\nSUMMARY:Jane Guest\r\nDTSTART;VALUE=DATE:{}\r\nDTEND;VALUE=DATE:{}\r\nEND:VEVENT\r\n",
            from.format("%Y%m%d"),
            to.format("%Y%m%d")
        ));
    }
    out.push_str("END:VCALENDAR\r\n");
    out.into_bytes()
}

async fn server() -> TestServer {
    start(Arc::new(|path: &str| {
        match path.split('?').next().unwrap_or("") {
            "/ok.ics" | "/secret-path.ics" => Reply::Ok(ics(&[("a@x", future(10), future(14))])),
            "/ok2.ics" => Reply::Ok(ics(&[("b@x", future(20), future(22))])),
            "/503.ics" => Reply::Status(503),
            "/html.ics" => Reply::Ok(b"<html>Sign in</html>".to_vec()),
            "/stall.ics" => Reply::Stall,
            p if p.starts_with("/many-") => Reply::Ok(ics(&[(p, future(30), future(31))])),
            _ => Reply::Status(404),
        }
    }))
    .await
}

struct CountingPublisher {
    pool: PgPool,
    fail: bool,
    seen: Mutex<Vec<(Uuid, bool)>>,
}

#[async_trait::async_trait]
impl OutboxPublisher for CountingPublisher {
    async fn publish(&self, outbox_id: Uuid) -> Result<(), ()> {
        // A separate connection only sees the row once it's committed.
        let committed: bool =
            sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM email_outbox WHERE id = $1)")
                .bind(outbox_id)
                .fetch_one(&self.pool)
                .await
                .unwrap_or(false);
        self.seen.lock().expect("lock").push((outbox_id, committed));
        if self.fail { Err(()) } else { Ok(()) }
    }
}

impl CountingPublisher {
    async fn wait_for(&self, n: usize) -> Vec<(Uuid, bool)> {
        let started = std::time::Instant::now();
        loop {
            let seen = self.seen.lock().expect("lock").clone();
            if seen.len() >= n || started.elapsed() > Duration::from_secs(5) {
                return seen;
            }
            tokio::task::yield_now().await;
        }
    }
}

struct Fixture {
    _db: TestPg,
    pool: PgPool,
    server: TestServer,
    listing: Uuid,
    publisher: Arc<CountingPublisher>,
    deps: SyncDeps,
}

async fn fixture(fail_publish: bool) -> Fixture {
    let db = test_db();
    let pool = db.get_pool().await;
    let server = server().await;
    let host: Uuid = sqlx::query_scalar(
        r#"INSERT INTO "user" (email, password_hash, first_name, last_name, slug)
           VALUES ('h-' || gen_random_uuid() || '@example.com', 'x', 'H', 'T',
                   'h-' || replace(gen_random_uuid()::text, '-', '')) RETURNING id"#,
    )
    .fetch_one(&pool)
    .await
    .expect("host");
    let listing: Uuid = sqlx::query_scalar(
        "INSERT INTO listing (is_active, name, listing_structure_id, country, added_at, user_id, slug)
         VALUES (TRUE, 'Villa', 1, 'Jamaica', NOW(), $1, 'v-' || replace(gen_random_uuid()::text, '-', ''))
         RETURNING id",
    )
    .bind(host)
    .fetch_one(&pool)
    .await
    .expect("listing");
    let publisher = Arc::new(CountingPublisher {
        pool: pool.clone(),
        fail: fail_publish,
        seen: Mutex::new(Vec::new()),
    });
    let fetcher = GuardedFetcher::new(FetcherConfig {
        connect_timeout: Duration::from_millis(300),
        total_timeout: Duration::from_millis(800),
        allow_private_hosts: true,
        extra_root_ca: Some(server.ca()),
        resolver: Some(FixedResolver::with(&[("feeds.test", "127.0.0.1")])),
        ..FetcherConfig::default()
    })
    .expect("fetcher");
    let deps = SyncDeps {
        pool: pool.clone(),
        publisher: publisher.clone(),
        fetcher,
    };
    Fixture {
        _db: db,
        pool,
        server,
        listing,
        publisher,
        deps,
    }
}

impl Fixture {
    async fn feed(&self, path: &str) -> Uuid {
        sqlx::query_scalar(
            "INSERT INTO listing_calendar_feed (listing_id, name, url) VALUES ($1, 'Airbnb', $2) RETURNING id",
        )
        .bind(self.listing)
        .bind(self.server.url("feeds.test", path).to_string())
        .fetch_one(&self.pool)
        .await
        .expect("feed")
    }

    async fn claims(&self) -> Vec<FeedClaim> {
        claim_for_listing(&self.pool, self.listing, chrono::Duration::seconds(60))
            .await
            .expect("claim")
            .claims
    }

    async fn blocks(&self, feed: Uuid) -> i64 {
        sqlx::query_scalar("SELECT COUNT(*) FROM listing_blocked_dates WHERE feed_id = $1")
            .bind(feed)
            .fetch_one(&self.pool)
            .await
            .expect("count")
    }
}

/// Collects every log line written while a future runs under [`LogCapture::dispatch`].
#[derive(Clone, Default)]
struct LogCapture(Arc<Mutex<Vec<u8>>>);

impl std::io::Write for LogCapture {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.lock().expect("log lock").extend_from_slice(buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl LogCapture {
    fn dispatch(&self) -> tracing::Dispatch {
        let writer = self.clone();
        tracing::Dispatch::new(
            tracing_subscriber::fmt()
                .with_max_level(tracing::Level::TRACE)
                .with_ansi(false)
                .with_writer(move || writer.clone())
                .finish(),
        )
    }

    fn text(&self) -> String {
        String::from_utf8_lossy(&self.0.lock().expect("log lock")).into_owned()
    }
}

fn ctx(trigger: SyncTrigger) -> SyncContext {
    SyncContext {
        trigger,
        admin_base_url: "https://admin.test".into(),
    }
}

fn soon(ms: u64) -> Instant {
    Instant::now() + Duration::from_millis(ms)
}

#[tokio::test(flavor = "multi_thread")]
async fn ok_failing_and_malformed_feeds_give_their_own_results() {
    let f = fixture(false).await;
    let ok = f.feed("/ok.ics").await;
    let down = f.feed("/503.ics").await;
    let html = f.feed("/html.ics").await;
    let results = sync_feeds(
        &f.deps,
        f.claims().await,
        &ctx(SyncTrigger::Manual),
        4,
        soon(5000),
    )
    .await;
    let by = |id: Uuid| results.iter().find(|r| r.feed_id == id).expect("result");
    assert_eq!(by(ok).outcome, SyncOutcome::Ok);
    assert_eq!(by(ok).events, Some(1));
    assert_eq!(by(down).outcome, SyncOutcome::Failed);
    assert_eq!(by(down).error_code.as_deref(), Some("fetch_http_status"));
    assert_eq!(by(html).outcome, SyncOutcome::Failed);
    assert_eq!(by(html).error_code.as_deref(), Some("feed_not_calendar"));
    assert_eq!(
        (
            f.blocks(ok).await,
            f.blocks(down).await,
            f.blocks(html).await
        ),
        (1, 0, 0)
    );
    let error: Option<String> =
        sqlx::query_scalar("SELECT last_error FROM listing_calendar_feed WHERE id = $1")
            .bind(down)
            .fetch_one(&f.pool)
            .await
            .expect("error");
    assert_eq!(error.as_deref(), Some("calendar server returned HTTP 503"));
}

#[tokio::test(flavor = "multi_thread")]
async fn five_feeds_of_one_listing_use_one_connection_at_a_time() {
    let f = fixture(false).await;
    for i in 0..5 {
        f.feed(&format!("/many-{i}.ics")).await;
    }
    let claims = f.claims().await;
    assert_eq!(claims.len(), 5);
    let idle_before = f.pool.num_idle();
    let size_before = f.pool.size() as usize;
    let in_use_before = size_before.saturating_sub(idle_before);
    let pool = f.pool.clone();
    let done = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let flag = done.clone();
    let sampler = tokio::spawn(async move {
        let mut high = 0usize;
        while !flag.load(std::sync::atomic::Ordering::Relaxed) {
            let in_use = (pool.size() as usize).saturating_sub(pool.num_idle());
            high = high.max(in_use);
            tokio::task::yield_now().await;
        }
        high
    });
    let results = sync_feeds(&f.deps, claims, &ctx(SyncTrigger::Manual), 8, soon(5000)).await;
    done.store(true, std::sync::atomic::Ordering::Relaxed);
    let high = sampler.await.expect("sampler");
    assert!(
        results.iter().all(|r| r.outcome == SyncOutcome::Ok),
        "{results:?}"
    );
    // sqlx returns a dropped connection to the pool from a background task, so
    // the previous feed's connection can still count as in use for a moment
    // after the next feed acquires one. Serial use peaks at +2; five feeds in
    // parallel would reach +5.
    assert!(high <= in_use_before + 2, "high-water mark {high}");
}

#[tokio::test(flavor = "multi_thread")]
async fn the_deadline_defers_without_counting_a_failure() {
    let f = fixture(false).await;
    let stalled = f.feed("/stall.ics").await;
    let results = sync_feeds(
        &f.deps,
        f.claims().await,
        &ctx(SyncTrigger::Scheduled),
        4,
        soon(300),
    )
    .await;
    assert_eq!(results[0].feed_id, stalled);
    assert_eq!(results[0].outcome, SyncOutcome::Deferred);
    let (status, failures): (common::calendar::FeedStatus, i32) = sqlx::query_as(
        "SELECT last_status, consecutive_failures FROM listing_calendar_feed WHERE id = $1",
    )
    .bind(stalled)
    .fetch_one(&f.pool)
    .await
    .expect("feed");
    assert_eq!((status, failures), (common::calendar::FeedStatus::Never, 0));
}

#[tokio::test(flavor = "multi_thread")]
async fn feeds_not_started_by_the_deadline_are_released() {
    let f = fixture(false).await;
    let feed = f.feed("/ok.ics").await;
    let claims = f.claims().await;
    let results = sync_feeds(
        &f.deps,
        claims,
        &ctx(SyncTrigger::Scheduled),
        4,
        Instant::now(),
    )
    .await;
    assert_eq!(results[0].outcome, SyncOutcome::Deferred);
    let attempt: Option<chrono::DateTime<Utc>> =
        sqlx::query_scalar("SELECT last_attempt_at FROM listing_calendar_feed WHERE id = $1")
            .bind(feed)
            .fetch_one(&f.pool)
            .await
            .expect("feed");
    assert_eq!(attempt, None, "claim released");
    assert!(f.server.requests().is_empty());
}

async fn conflicted(f: &Fixture) {
    sqlx::query(
        r#"INSERT INTO booking (confirmation_code, guest_id, listing_id, status, date_from, date_to,
             currency, daily_rate, number_of_persons, total_days, sub_total_price, total_price,
             cancellation_policy)
         SELECT 'CONF123456', user_id, id, 'confirmed', $2, $3, 'USD', 100, 2, 4, 400, 400, 'flexible'
         FROM listing WHERE id = $1"#,
    )
    .bind(f.listing)
    .bind(future(11))
    .bind(future(15))
    .execute(&f.pool)
    .await
    .expect("booking");
}

#[tokio::test(flavor = "multi_thread")]
async fn emails_are_published_after_commit() {
    let f = fixture(false).await;
    conflicted(&f).await;
    f.feed("/ok.ics").await;
    let results = sync_feeds(
        &f.deps,
        f.claims().await,
        &ctx(SyncTrigger::Manual),
        4,
        soon(5000),
    )
    .await;
    assert_eq!(results[0].conflicts_opened, 1);
    let seen = f.publisher.wait_for(1).await;
    assert_eq!(seen.len(), 1);
    assert!(seen[0].1, "the outbox row was committed before publishing");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_publish_failure_does_not_fail_the_sync() {
    let f = fixture(true).await;
    conflicted(&f).await;
    f.feed("/ok.ics").await;
    let results = sync_feeds(
        &f.deps,
        f.claims().await,
        &ctx(SyncTrigger::Manual),
        4,
        soon(5000),
    )
    .await;
    assert_eq!(results[0].outcome, SyncOutcome::Ok);
    assert_eq!(results[0].conflicts_opened, 1);
    assert_eq!(
        f.publisher.wait_for(1).await.len(),
        1,
        "publish was attempted"
    );
}

#[tokio::test]
async fn a_publish_failure_is_logged_at_error_and_the_rest_still_publish() {
    use tracing::instrument::WithSubscriber;
    let db = test_db();
    let pool = db.get_pool().await;
    let publisher = CountingPublisher {
        pool,
        fail: true,
        seen: Mutex::new(Vec::new()),
    };
    let ids = [Uuid::now_v7(), Uuid::now_v7()];
    let logs = LogCapture::default();
    super::sync::publish_all(&publisher, &ids)
        .with_subscriber(logs.dispatch())
        .await;
    assert_eq!(publisher.seen.lock().expect("lock").len(), 2);
    let text = logs.text();
    assert_eq!(
        text.lines()
            .filter(|l| l.contains("ERROR") && l.contains("calendar email publish failed"))
            .count(),
        2,
        "{text}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn logs_never_contain_the_feed_path_or_query() {
    use tracing::instrument::WithSubscriber;
    let f = fixture(false).await;
    f.feed("/secret-path.ics?s=topsecret").await;
    f.feed("/503.ics?token=topsecret").await;
    f.feed("/html.ics?token=topsecret").await;
    let logs = LogCapture::default();
    sync_feeds(
        &f.deps,
        f.claims().await,
        &ctx(SyncTrigger::Scheduled),
        4,
        soon(5000),
    )
    .with_subscriber(logs.dispatch())
    .await;
    let text = logs.text();
    assert!(text.contains("feeds.test"), "the host is logged: {text}");
    assert!(!text.contains("topsecret"), "{text}");
    assert!(!text.contains("secret-path"), "{text}");
    assert!(!text.contains("Jane Guest"), "event text is never logged");
}
