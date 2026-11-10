//! Job run tests against a test database and a local https feed server.

use crate::run::{JobConfig, JobDeps, run};
use api_core::calendar::FetcherConfig;
use api_core::calendar::sync::OutboxPublisher;
use api_core::calendar::test_server::{FixedResolver, Reply, TestServer, start};
use chrono::{Duration as Days, Utc};
use sqlx::PgPool;
use sqlx_db_tester::TestPg;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use uuid::Uuid;

const DATABASE_URL_ENV: &str = "DATABASE_URL";

fn db_url() -> String {
    dotenvy::dotenv().ok();
    std::env::var(DATABASE_URL_ENV)
        .unwrap_or_else(|_| "postgres://postgres:password@localhost:5432/our_places".into())
}

#[derive(Default)]
struct Counting(Mutex<Vec<Uuid>>);

#[async_trait::async_trait]
impl OutboxPublisher for Counting {
    async fn publish(&self, id: Uuid) -> Result<(), ()> {
        self.0.lock().map_err(|_| ())?.push(id);
        Ok(())
    }
}

async fn server() -> TestServer {
    let today = common::reference::listing_today(Utc::now());
    let body = format!(
        "BEGIN:VCALENDAR\r\nBEGIN:VEVENT\r\nUID:a@x\r\nDTSTART;VALUE=DATE:{}\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n",
        (today + Days::days(10)).format("%Y%m%d")
    );
    start(Arc::new(move |path: &str| match path {
        "/ok.ics" => Reply::Ok(body.clone().into_bytes()),
        "/503.ics" => Reply::Status(503),
        "/stall.ics" => Reply::Stall,
        _ => Reply::Status(404),
    }))
    .await
}

fn fetcher(s: &TestServer) -> FetcherConfig {
    FetcherConfig {
        connect_timeout: Duration::from_millis(500),
        total_timeout: Duration::from_secs(5),
        allow_private_hosts: true,
        extra_root_ca: Some(s.ca()),
        resolver: Some(FixedResolver::with(&[("feeds.test", "127.0.0.1")])),
        ..FetcherConfig::default()
    }
}

/// The test database's URL (the pool the job opens must reach it).
fn url_of(db: &TestPg) -> String {
    db.url()
}

async fn listing(pool: &PgPool) -> Uuid {
    let host: Uuid = sqlx::query_scalar(
        r#"INSERT INTO "user" (email, password_hash, first_name, last_name, slug)
           VALUES ('h-' || gen_random_uuid() || '@example.com', 'x', 'H', 'T',
                   'h-' || replace(gen_random_uuid()::text, '-', '')) RETURNING id"#,
    )
    .fetch_one(pool)
    .await
    .expect("host");
    sqlx::query_scalar(
        "INSERT INTO listing (is_active, name, listing_structure_id, country, added_at, user_id, slug)
         VALUES (TRUE, 'Villa ' || gen_random_uuid(), 1, 'Jamaica', NOW(), $1, 'v-' || replace(gen_random_uuid()::text, '-', ''))
         RETURNING id",
    )
    .bind(host)
    .fetch_one(pool)
    .await
    .expect("listing")
}

async fn feed(pool: &PgPool, s: &TestServer, listing: Uuid, path: &str) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO listing_calendar_feed (listing_id, name, url) VALUES ($1, 'Airbnb', $2) RETURNING id",
    )
    .bind(listing)
    .bind(s.url("feeds.test", path).to_string())
    .fetch_one(pool)
    .await
    .expect("feed")
}

fn config(url: String, concurrency: u16, deadline: Duration) -> JobConfig {
    JobConfig {
        database_url: url,
        concurrency,
        deadline,
        admin_base_url: "https://admin.test".into(),
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn run_tests_one_ok_and_one_failing_feed_give_the_summary() {
    let db = TestPg::new(db_url(), Path::new("../../db_core/migrations"));
    let pool = db.get_pool().await;
    let s = server().await;
    let v = listing(&pool).await;
    feed(&pool, &s, v, "/ok.ics").await;
    let failing = feed(&pool, &s, v, "/503.ics").await;
    let summary = run(
        config(url_of(&db), 4, Duration::from_secs(60)),
        JobDeps {
            publisher: Arc::new(Counting::default()),
            fetcher: fetcher(&s),
        },
    )
    .await
    .expect("run");
    assert_eq!(
        (
            summary.feeds_ok,
            summary.feeds_failed,
            summary.feeds_deferred
        ),
        (1, 1, 0)
    );
    let failures: i32 =
        sqlx::query_scalar("SELECT consecutive_failures FROM listing_calendar_feed WHERE id = $1")
            .bind(failing)
            .fetch_one(&pool)
            .await
            .expect("failures");
    assert_eq!(failures, 1, "scheduled failures count");
    // Nothing is due any more.
    let again = run(
        config(url_of(&db), 4, Duration::from_secs(60)),
        JobDeps {
            publisher: Arc::new(Counting::default()),
            fetcher: fetcher(&s),
        },
    )
    .await
    .expect("run");
    assert_eq!(again.feeds_ok + again.feeds_failed, 0);
}

#[tokio::test(flavor = "multi_thread")]
async fn run_tests_the_deadline_releases_claims_that_never_started() {
    let db = TestPg::new(db_url(), Path::new("../../db_core/migrations"));
    let pool = db.get_pool().await;
    let s = server().await;
    // With one worker, the stalled listing holds it past the deadline, so the
    // second listing's feed never starts.
    let slow = listing(&pool).await;
    feed(&pool, &s, slow, "/stall.ics").await;
    let later = listing(&pool).await;
    let unstarted = feed(&pool, &s, later, "/ok.ics").await;
    let summary = run(
        config(url_of(&db), 1, Duration::from_millis(400)),
        JobDeps {
            publisher: Arc::new(Counting::default()),
            fetcher: fetcher(&s),
        },
    )
    .await
    .expect("run");
    assert_eq!(summary.feeds_deferred, 2, "{summary:?}");
    let attempt: Option<chrono::DateTime<Utc>> =
        sqlx::query_scalar("SELECT last_attempt_at FROM listing_calendar_feed WHERE id = $1")
            .bind(unstarted)
            .fetch_one(&pool)
            .await
            .expect("attempt");
    assert_eq!(attempt, None, "the unstarted claim is released");
}

#[tokio::test]
async fn run_tests_an_unreachable_database_is_an_error() {
    let result = run(
        config(
            "postgres://postgres:password@127.0.0.1:1/none".into(),
            2,
            Duration::from_secs(5),
        ),
        JobDeps {
            publisher: Arc::new(Counting::default()),
            fetcher: FetcherConfig::default(),
        },
    )
    .await;
    assert!(result.is_err());
}

#[tokio::test(flavor = "multi_thread")]
async fn run_tests_stuck_calendar_emails_are_republished_once() {
    let db = TestPg::new(db_url(), Path::new("../../db_core/migrations"));
    let pool = db.get_pool().await;
    let s = server().await;
    let stuck: Uuid = sqlx::query_scalar(
        "INSERT INTO email_outbox (recipient_email, subject, template_id, payload, max_retries, created_at)
         VALUES ('h@example.com', 's', 'calendar_conflict_host', '{}'::jsonb, 3, NOW() - INTERVAL '10 minutes')
         RETURNING id",
    )
    .fetch_one(&pool)
    .await
    .expect("stuck");
    sqlx::query(
        "INSERT INTO email_outbox (recipient_email, subject, template_id, payload, max_retries, created_at)
         VALUES ('g@example.com', 's', 'booking_confirmation_guest', '{}'::jsonb, 3, NOW() - INTERVAL '10 minutes')",
    )
    .execute(&pool)
    .await
    .expect("other");
    let publisher = Arc::new(Counting::default());
    let summary = run(
        config(url_of(&db), 2, Duration::from_secs(30)),
        JobDeps {
            publisher: publisher.clone(),
            fetcher: fetcher(&s),
        },
    )
    .await
    .expect("run");
    assert_eq!(summary.emails_republished, 1);
    assert_eq!(*publisher.0.lock().expect("lock"), vec![stuck]);
}
