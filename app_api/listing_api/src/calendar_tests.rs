//! Calendar route tests (spec §3.5) against a local https feed server.

use crate::authz_tests::{bearer, listing_of, settings, test_db, user_with};
use crate::calendar::CalendarRuntime;
use actix_web::http::StatusCode;
use actix_web::{App, test, web};
use api_core::calendar::sync::OutboxPublisher;
use api_core::calendar::test_server::{FixedResolver, Reply, TestServer, start};
use api_core::calendar::{FetcherConfig, GuardedFetcher};
use chrono::{Duration as Days, NaiveDate, Utc};
use sqlx::PgPool;
use sqlx_db_tester::TestPg;
use std::sync::Arc;
use std::time::Duration;
use uuid::Uuid;

struct NoopPublisher;

#[async_trait::async_trait]
impl OutboxPublisher for NoopPublisher {
    async fn publish(&self, _outbox_id: Uuid) -> Result<(), ()> {
        Ok(())
    }
}

fn future(days: i64) -> NaiveDate {
    common::reference::listing_today(Utc::now()) + Days::days(days)
}

fn ics(events: &[(&str, NaiveDate, NaiveDate)]) -> Vec<u8> {
    let mut out = String::from("BEGIN:VCALENDAR\r\nVERSION:2.0\r\n");
    for (uid, from, to) in events {
        out.push_str(&format!(
            "BEGIN:VEVENT\r\nUID:{uid}\r\nDTSTART;VALUE=DATE:{}\r\nDTEND;VALUE=DATE:{}\r\nEND:VEVENT\r\n",
            from.format("%Y%m%d"),
            to.format("%Y%m%d")
        ));
    }
    out.push_str("END:VCALENDAR\r\n");
    out.into_bytes()
}

pub(crate) struct Ctx {
    pub _db: TestPg,
    pub pool: PgPool,
    pub server: TestServer,
    pub owner: Uuid,
    pub other_host: Uuid,
    pub admin: Uuid,
    pub listing: Uuid,
    pub other_listing: Uuid,
    runtime: web::Data<CalendarRuntime>,
}

pub(crate) async fn ctx() -> Ctx {
    ctx_with_deadline(Duration::from_secs(20)).await
}

async fn ctx_with_deadline(deadline: Duration) -> Ctx {
    let db = test_db();
    let pool = db.get_pool().await;
    let server = start(Arc::new(|path: &str| {
        match path.split('?').next().unwrap_or("") {
            "/ok.ics" => Reply::Ok(ics(&[("a@airbnb.com", future(10), future(14))])),
            "/ok2.ics" => Reply::Ok(ics(&[("b@vrbo.com", future(20), future(22))])),
            "/503.ics" => Reply::Status(503),
            "/stall.ics" => Reply::Stall,
            p if p.starts_with("/many-") => Reply::Ok(ics(&[(p, future(30), future(31))])),
            _ => Reply::Status(404),
        }
    }))
    .await;
    let fetcher = GuardedFetcher::new(FetcherConfig {
        connect_timeout: Duration::from_millis(500),
        total_timeout: Duration::from_secs(5),
        allow_private_hosts: true,
        extra_root_ca: Some(server.ca()),
        resolver: Some(FixedResolver::with(&[
            ("feeds.test", "127.0.0.1"),
            ("internal.test", "169.254.169.254"),
        ])),
        ..FetcherConfig::default()
    })
    .expect("fetcher");
    let runtime = web::Data::new(
        CalendarRuntime::with_fetcher(
            fetcher,
            Arc::new(NoopPublisher),
            "https://admin.test".into(),
        )
        .with_deadline(deadline),
    );
    let owner = user_with(&pool, "ARRAY['host']").await;
    let other_host = user_with(&pool, "ARRAY['host']").await;
    let admin = user_with(&pool, "ARRAY['admin']").await;
    let listing = listing_of(&pool, owner).await;
    let other_listing = listing_of(&pool, other_host).await;
    Ctx {
        _db: db,
        pool,
        server,
        owner,
        other_host,
        admin,
        listing,
        other_listing,
        runtime,
    }
}

impl Ctx {
    pub async fn call(
        &self,
        method: actix_web::http::Method,
        uri: &str,
        as_user: Option<Uuid>,
        body: Option<serde_json::Value>,
    ) -> (
        StatusCode,
        serde_json::Value,
        actix_web::http::header::HeaderMap,
    ) {
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(self.pool.clone()))
                .app_data(web::Data::new(settings()))
                .app_data(self.runtime.clone())
                .configure(crate::apis::configure_routes),
        )
        .await;
        let mut req = test::TestRequest::default().method(method).uri(uri);
        if let Some(user) = as_user {
            req = req.insert_header(bearer(user));
        }
        let req = match body {
            Some(json) => req.set_json(json).to_request(),
            None => req.to_request(),
        };
        let resp = test::call_service(&app, req).await;
        let status = resp.status();
        let headers = resp.headers().clone();
        let bytes = test::read_body(resp).await;
        let json = serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null);
        (status, json, headers)
    }

    pub fn feed_url(&self, path: &str) -> String {
        self.server.url("feeds.test", path).to_string()
    }

    pub async fn insert_feed(&self, listing: Uuid, path: &str) -> Uuid {
        sqlx::query_scalar(
            "INSERT INTO listing_calendar_feed (listing_id, name, url) VALUES ($1, 'Seeded', $2) RETURNING id",
        )
        .bind(listing)
        .bind(self.feed_url(path))
        .fetch_one(&self.pool)
        .await
        .expect("feed")
    }

    pub async fn forget_attempts(&self) {
        sqlx::query("UPDATE listing_calendar_feed SET last_attempt_at = NOW() - INTERVAL '1 hour' WHERE last_attempt_at IS NOT NULL")
            .execute(&self.pool)
            .await
            .expect("forget attempts");
    }

    pub async fn blocks_of(&self, feed: Uuid) -> i64 {
        sqlx::query_scalar("SELECT COUNT(*) FROM listing_blocked_dates WHERE feed_id = $1")
            .bind(feed)
            .fetch_one(&self.pool)
            .await
            .expect("count")
    }
}

mod feeds {
    use super::*;
    use actix_web::http::Method;

    fn feeds_uri(listing: Uuid) -> String {
        format!("/api/v1/listings/{listing}/calendar/feeds")
    }

    fn new_feed(url: &str) -> serde_json::Value {
        serde_json::json!({ "name": "  Airbnb  ", "url": url })
    }

    fn id_of(body: &serde_json::Value) -> Uuid {
        body["feed"]["id"]
            .as_str()
            .and_then(|s| s.parse().ok())
            .expect("feed id")
    }

    #[actix_web::test]
    async fn adding_a_feed_syncs_it() {
        let c = ctx().await;
        let (status, body, _) = c
            .call(
                Method::POST,
                &feeds_uri(c.listing),
                Some(c.owner),
                Some(new_feed(&c.feed_url("/ok.ics"))),
            )
            .await;
        assert_eq!(status, StatusCode::CREATED, "{body}");
        assert_eq!(body["sync"]["outcome"], "ok");
        assert_eq!(body["sync"]["events"], 1);
        assert_eq!(body["feed"]["last_status"], "ok");
        assert_eq!(body["feed"]["name"], "Airbnb");
        assert_eq!(c.blocks_of(id_of(&body)).await, 1);

        let (status, list, _) = c
            .call(Method::GET, &feeds_uri(c.listing), Some(c.owner), None)
            .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(list.as_array().map(Vec::len), Some(1));
    }

    #[actix_web::test]
    async fn a_failed_first_sync_still_saves_the_feed() {
        let c = ctx().await;
        let url = c.server.url("internal.test", "/ok.ics").to_string();
        let (status, body, _) = c
            .call(
                Method::POST,
                &feeds_uri(c.listing),
                Some(c.owner),
                Some(new_feed(&url)),
            )
            .await;
        assert_eq!(status, StatusCode::CREATED, "{body}");
        assert_eq!(body["sync"]["outcome"], "failed");
        assert_eq!(body["sync"]["error_code"], "address_not_allowed");
        assert_eq!(body["feed"]["last_status"], "failed");
        assert_eq!(body["feed"]["last_error"], "address not allowed");
    }

    #[actix_web::test]
    async fn limits_duplicates_and_bad_urls_are_refused() {
        let c = ctx().await;
        let dup = c.feed_url("/ok.ics");
        c.insert_feed(c.listing, "/ok.ics").await;
        let (status, body, _) = c
            .call(
                Method::POST,
                &feeds_uri(c.listing),
                Some(c.owner),
                Some(new_feed(&dup)),
            )
            .await;
        assert_eq!(
            (status, body["code"].as_str()),
            (StatusCode::CONFLICT, Some("CALENDAR_FEED_DUPLICATE"))
        );

        for i in 0..4 {
            c.insert_feed(c.listing, &format!("/many-{i}.ics")).await;
        }
        let (status, body, _) = c
            .call(
                Method::POST,
                &feeds_uri(c.listing),
                Some(c.owner),
                Some(new_feed(&c.feed_url("/ok2.ics"))),
            )
            .await;
        assert_eq!(
            (status, body["code"].as_str()),
            (StatusCode::CONFLICT, Some("CALENDAR_FEED_LIMIT"))
        );

        let own = format!(
            "https://anything.example/listings/{}/calendar.ics",
            c.other_listing
        );
        let (status, body, _) = c
            .call(
                Method::POST,
                &feeds_uri(c.other_listing),
                Some(c.other_host),
                Some(new_feed(&own)),
            )
            .await;
        assert_eq!(
            (status, body["code"].as_str()),
            (StatusCode::UNPROCESSABLE_ENTITY, Some("CALENDAR_FEED_SELF"))
        );
        for bad in ["http://feeds.test/a.ics", "https://10.0.0.1/a.ics", "nope"] {
            let (status, body, _) = c
                .call(
                    Method::POST,
                    &feeds_uri(c.other_listing),
                    Some(c.other_host),
                    Some(new_feed(bad)),
                )
                .await;
            assert_eq!(
                (status, body["code"].as_str()),
                (
                    StatusCode::UNPROCESSABLE_ENTITY,
                    Some("CALENDAR_FEED_INVALID_URL")
                ),
                "{bad}"
            );
        }
        let (status, body, _) = c
            .call(
                Method::POST,
                &feeds_uri(c.other_listing),
                Some(c.other_host),
                Some(serde_json::json!({ "name": " ", "url": c.feed_url("/ok.ics") })),
            )
            .await;
        assert_eq!(
            (status, body["code"].as_str()),
            (StatusCode::UNPROCESSABLE_ENTITY, Some("VALIDATION_ERROR"))
        );
    }

    #[actix_web::test]
    async fn callers_are_checked() {
        let c = ctx().await;
        let feed = c.insert_feed(c.listing, "/ok.ics").await;
        let cases = [
            (Method::GET, feeds_uri(c.listing), None),
            (
                Method::POST,
                feeds_uri(c.listing),
                Some(new_feed(&c.feed_url("/ok2.ics"))),
            ),
            (
                Method::PATCH,
                format!("{}/{feed}", feeds_uri(c.listing)),
                Some(serde_json::json!({ "name": "X" })),
            ),
            (
                Method::DELETE,
                format!("{}/{feed}", feeds_uri(c.listing)),
                None,
            ),
            (
                Method::POST,
                format!("/api/v1/listings/{}/calendar/sync", c.listing),
                None,
            ),
        ];
        for (method, uri, body) in cases {
            let (status, _, _) = c.call(method.clone(), &uri, None, body.clone()).await;
            assert_eq!(status, StatusCode::UNAUTHORIZED, "{method} {uri}");
            let (status, _, _) = c.call(method.clone(), &uri, Some(c.other_host), body).await;
            assert_eq!(status, StatusCode::FORBIDDEN, "{method} {uri}");
        }
        // A feed of another listing, reached through this one: 404.
        let foreign = c.insert_feed(c.other_listing, "/ok.ics").await;
        let (status, _, _) = c
            .call(
                Method::PATCH,
                &format!("{}/{foreign}", feeds_uri(c.listing)),
                Some(c.owner),
                Some(serde_json::json!({ "name": "X" })),
            )
            .await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        let (status, _, _) = c
            .call(
                Method::DELETE,
                &format!("{}/{foreign}", feeds_uri(c.listing)),
                Some(c.admin),
                None,
            )
            .await;
        assert_eq!(status, StatusCode::NOT_FOUND);
    }

    #[actix_web::test]
    async fn a_url_change_is_validated_resynced_and_keeps_old_blocks_on_failure() {
        let c = ctx().await;
        let (_, body, _) = c
            .call(
                Method::POST,
                &feeds_uri(c.listing),
                Some(c.owner),
                Some(new_feed(&c.feed_url("/ok.ics"))),
            )
            .await;
        let feed = id_of(&body);
        let uri = format!("{}/{feed}", feeds_uri(c.listing));

        let own = format!("https://feeds.test/listings/{}/calendar.ics", c.listing);
        let (status, body, _) = c
            .call(
                Method::PATCH,
                &uri,
                Some(c.owner),
                Some(serde_json::json!({ "url": own })),
            )
            .await;
        assert_eq!(
            (status, body["code"].as_str()),
            (StatusCode::UNPROCESSABLE_ENTITY, Some("CALENDAR_FEED_SELF"))
        );

        // Within a minute of the first sync, a new URL is rate limited.
        let (status, body, headers) = c
            .call(
                Method::PATCH,
                &uri,
                Some(c.owner),
                Some(serde_json::json!({ "url": c.feed_url("/503.ics") })),
            )
            .await;
        assert_eq!(
            (status, body["code"].as_str()),
            (
                StatusCode::TOO_MANY_REQUESTS,
                Some("CALENDAR_SYNC_RATE_LIMITED")
            )
        );
        assert!(headers.contains_key("retry-after"));

        c.forget_attempts().await;
        let (status, body, _) = c
            .call(
                Method::PATCH,
                &uri,
                Some(c.owner),
                Some(serde_json::json!({ "url": c.feed_url("/503.ics") })),
            )
            .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(body["sync"]["outcome"], "failed");
        let generation: i32 =
            sqlx::query_scalar("SELECT sync_generation FROM listing_calendar_feed WHERE id = $1")
                .bind(feed)
                .fetch_one(&c.pool)
                .await
                .expect("generation");
        assert_eq!(generation, 1);
        assert_eq!(
            c.blocks_of(feed).await,
            1,
            "old blocks kept until the new URL syncs"
        );
    }

    #[actix_web::test]
    async fn switching_off_keeps_the_blocks_and_delete_removes_them() {
        let c = ctx().await;
        let (_, body, _) = c
            .call(
                Method::POST,
                &feeds_uri(c.listing),
                Some(c.owner),
                Some(new_feed(&c.feed_url("/ok.ics"))),
            )
            .await;
        let feed = id_of(&body);
        let uri = format!("{}/{feed}", feeds_uri(c.listing));
        let (status, body, _) = c
            .call(
                Method::PATCH,
                &uri,
                Some(c.owner),
                Some(serde_json::json!({ "is_active": false })),
            )
            .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["feed"]["is_active"], false);
        assert!(body["sync"].is_null());
        assert_eq!(c.blocks_of(feed).await, 1);
        let (status, _, _) = c.call(Method::DELETE, &uri, Some(c.admin), None).await;
        assert_eq!(status, StatusCode::NO_CONTENT);
        assert_eq!(c.blocks_of(feed).await, 0);
    }

    #[actix_web::test]
    async fn syncing_again_within_a_minute_is_rate_limited() {
        let c = ctx().await;
        c.insert_feed(c.listing, "/ok.ics").await;
        let sync = format!("/api/v1/listings/{}/calendar/sync", c.listing);
        let (status, body, _) = c
            .call(actix_web::http::Method::POST, &sync, Some(c.owner), None)
            .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(body[0]["outcome"], "ok");
        let (status, body, headers) = c
            .call(actix_web::http::Method::POST, &sync, Some(c.owner), None)
            .await;
        assert_eq!(
            (status, body["code"].as_str()),
            (
                StatusCode::TOO_MANY_REQUESTS,
                Some("CALENDAR_SYNC_RATE_LIMITED")
            )
        );
        let wait: u64 = headers
            .get("retry-after")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.parse().ok())
            .expect("retry-after");
        assert!((1..=60).contains(&wait));
        let (status, _, _) = c
            .call(
                Method::POST,
                &feeds_uri(c.listing),
                Some(c.owner),
                Some(new_feed(&c.feed_url("/ok2.ics"))),
            )
            .await;
        assert_eq!(
            status,
            StatusCode::TOO_MANY_REQUESTS,
            "adding a feed shares the limit"
        );
        // A listing without active feeds syncs nothing.
        let (status, body, _) = c
            .call(
                Method::POST,
                &format!("/api/v1/listings/{}/calendar/sync", c.other_listing),
                Some(c.other_host),
                None,
            )
            .await;
        assert_eq!((status, body), (StatusCode::OK, serde_json::json!([])));
    }

    #[actix_web::test]
    async fn five_feeds_sync_well_within_the_deadline() {
        let c = ctx().await;
        for i in 0..5 {
            c.insert_feed(c.listing, &format!("/many-{i}.ics")).await;
        }
        let started = std::time::Instant::now();
        let (status, body, _) = c
            .call(
                Method::POST,
                &format!("/api/v1/listings/{}/calendar/sync", c.listing),
                Some(c.owner),
                None,
            )
            .await;
        assert_eq!(status, StatusCode::OK);
        let results = body.as_array().expect("array");
        assert_eq!(results.len(), 5);
        assert!(results.iter().all(|r| r["outcome"] == "ok"), "{body}");
        assert!(started.elapsed() < Duration::from_secs(20));
    }

    #[actix_web::test]
    async fn the_deadline_also_applies_to_adding_and_re_pointing_feeds() {
        let c = ctx_with_deadline(Duration::from_millis(300)).await;
        let (status, body, _) = c
            .call(
                Method::POST,
                &feeds_uri(c.listing),
                Some(c.owner),
                Some(new_feed(&c.feed_url("/stall.ics"))),
            )
            .await;
        assert_eq!(status, StatusCode::CREATED);
        assert_eq!(body["sync"]["outcome"], "deferred");
        assert_eq!(body["feed"]["last_status"], "never");
        let feed = id_of(&body);
        c.forget_attempts().await;
        let (status, body, _) = c
            .call(
                Method::PATCH,
                &format!("{}/{feed}", feeds_uri(c.listing)),
                Some(c.owner),
                Some(serde_json::json!({ "url": c.feed_url("/stall.ics?v=2") })),
            )
            .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(body["sync"]["outcome"], "deferred");
    }
}

/// Inserts a confirmed booking on `listing`.
async fn booking(c: &Ctx, listing: Uuid, from: NaiveDate, to: NaiveDate) -> Uuid {
    let guest = user_with(&c.pool, "ARRAY['booker']").await;
    sqlx::query_scalar(
        "INSERT INTO booking (confirmation_code, guest_id, listing_id, status, date_from, date_to,
             currency, daily_rate, number_of_persons, total_days, sub_total_price, total_price,
             cancellation_policy)
         VALUES (upper(substr(md5(random()::text), 1, 10)), $1, $2, 'confirmed', $3, $4, 'USD',
             100, 2, ($4 - $3), 200, 200, 'flexible')
         RETURNING id",
    )
    .bind(guest)
    .bind(listing)
    .bind(from)
    .bind(to)
    .fetch_one(&c.pool)
    .await
    .expect("booking")
}

mod blocks {
    use super::*;
    use actix_web::http::Method;

    fn uri(listing: Uuid) -> String {
        format!("/api/v1/listings/{listing}/calendar/blocks")
    }

    fn block(from: NaiveDate, to: NaiveDate) -> serde_json::Value {
        serde_json::json!({ "date_from": from, "date_to": to, "note": "owner stay" })
    }

    #[actix_web::test]
    async fn manual_blocks_are_created_listed_and_removed() {
        let c = ctx().await;
        let (status, body, _) = c
            .call(
                Method::POST,
                &uri(c.listing),
                Some(c.owner),
                Some(block(future(5), future(8))),
            )
            .await;
        assert_eq!(status, StatusCode::CREATED, "{body}");
        assert_eq!(body["source"], "manual");
        let id = body["id"].as_str().expect("id").to_string();

        let (status, body, _) = c
            .call(
                Method::POST,
                &uri(c.listing),
                Some(c.owner),
                Some(block(future(5), future(8))),
            )
            .await;
        assert_eq!(
            (status, body["code"].as_str()),
            (StatusCode::CONFLICT, Some("CALENDAR_BLOCK_DUPLICATE"))
        );

        let (status, list, _) = c
            .call(Method::GET, &uri(c.listing), Some(c.owner), None)
            .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(list.as_array().map(Vec::len), Some(1));

        let (status, _, _) = c
            .call(
                Method::DELETE,
                &format!("{}/{id}", uri(c.other_listing)),
                Some(c.other_host),
                None,
            )
            .await;
        assert_eq!(status, StatusCode::NOT_FOUND, "another listing's block");
        let (status, _, _) = c
            .call(
                Method::DELETE,
                &format!("{}/{id}", uri(c.listing)),
                Some(c.owner),
                None,
            )
            .await;
        assert_eq!(status, StatusCode::NO_CONTENT);
    }

    #[actix_web::test]
    async fn blocks_over_stays_and_invalid_ranges_are_refused() {
        let c = ctx().await;
        booking(&c, c.listing, future(10), future(15)).await;
        let (status, body, _) = c
            .call(
                Method::POST,
                &uri(c.listing),
                Some(c.owner),
                Some(block(future(12), future(13))),
            )
            .await;
        assert_eq!(
            (status, body["code"].as_str()),
            (StatusCode::CONFLICT, Some("DATE_UNAVAILABLE"))
        );
        for (from, to) in [
            (future(-1), future(2)),
            (future(5), future(5)),
            (future(1), future(400)),
        ] {
            let (status, body, _) = c
                .call(
                    Method::POST,
                    &uri(c.listing),
                    Some(c.owner),
                    Some(block(from, to)),
                )
                .await;
            assert_eq!(
                (status, body["code"].as_str()),
                (StatusCode::UNPROCESSABLE_ENTITY, Some("VALIDATION_ERROR")),
                "{from}..{to}"
            );
        }
        let (status, _, _) = c
            .call(
                Method::GET,
                &format!("{}?from={}&to={}", uri(c.listing), future(0), future(900)),
                Some(c.owner),
                None,
            )
            .await;
        assert_eq!(
            status,
            StatusCode::UNPROCESSABLE_ENTITY,
            "range over 24 months"
        );
    }

    #[actix_web::test]
    async fn imported_blocks_cannot_be_deleted_here() {
        let c = ctx().await;
        let feed = c.insert_feed(c.listing, "/ok.ics").await;
        let id: Uuid = sqlx::query_scalar(
            "INSERT INTO listing_blocked_dates (listing_id, source, feed_id, external_uid, date_from, date_to)
             VALUES ($1, 'ical_feed', $2, 'x@airbnb.com', $3, $4) RETURNING id",
        )
        .bind(c.listing)
        .bind(feed)
        .bind(future(3))
        .bind(future(4))
        .fetch_one(&c.pool)
        .await
        .expect("block");
        let (status, body, _) = c
            .call(
                Method::DELETE,
                &format!("{}/{id}", uri(c.listing)),
                Some(c.owner),
                None,
            )
            .await;
        assert_eq!(
            (status, body["code"].as_str()),
            (StatusCode::CONFLICT, Some("CALENDAR_BLOCK_MANAGED_BY_FEED"))
        );
    }

    #[actix_web::test]
    async fn callers_are_checked() {
        let c = ctx().await;
        let some = Uuid::now_v7();
        for (method, path, body) in [
            (Method::GET, uri(c.listing), None),
            (
                Method::POST,
                uri(c.listing),
                Some(block(future(5), future(8))),
            ),
            (Method::DELETE, format!("{}/{some}", uri(c.listing)), None),
        ] {
            let (status, _, _) = c.call(method.clone(), &path, None, body.clone()).await;
            assert_eq!(status, StatusCode::UNAUTHORIZED, "{method} {path}");
            let (status, _, _) = c
                .call(method.clone(), &path, Some(c.other_host), body)
                .await;
            assert_eq!(status, StatusCode::FORBIDDEN, "{method} {path}");
        }
    }
}

mod conflicts {
    use super::*;
    use actix_web::http::Method;

    fn uri(listing: Uuid) -> String {
        format!("/api/v1/listings/{listing}/calendar/conflicts")
    }

    async fn conflicted(c: &Ctx) {
        booking(c, c.listing, future(11), future(13)).await;
        let feeds = format!("/api/v1/listings/{}/calendar/feeds", c.listing);
        let (status, body, _) = c
            .call(
                Method::POST,
                &feeds,
                Some(c.owner),
                Some(serde_json::json!({ "name": "Airbnb", "url": c.feed_url("/ok.ics") })),
            )
            .await;
        assert_eq!(status, StatusCode::CREATED);
        assert_eq!(body["sync"]["conflicts_opened"], 1, "{body}");
    }

    #[actix_web::test]
    async fn conflicts_are_listed_and_acknowledged_once() {
        let c = ctx().await;
        conflicted(&c).await;
        let (status, body, _) = c
            .call(Method::GET, &uri(c.listing), Some(c.owner), None)
            .await;
        assert_eq!(status, StatusCode::OK);
        let open = body["open"].as_array().expect("open");
        assert_eq!(open.len(), 1);
        assert_eq!(open[0]["kind"], "overlap");
        assert!(open[0]["stale_reason"].is_null());
        let id = open[0]["id"].as_str().expect("id").to_string();

        let other = format!(
            "/api/v1/listings/{}/calendar/conflicts/{id}/acknowledge",
            c.other_listing
        );
        let (status, _, _) = c.call(Method::POST, &other, Some(c.other_host), None).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "another listing's conflict");

        let ack = format!("{}/{id}/acknowledge", uri(c.listing));
        for _ in 0..2 {
            let (status, _, _) = c.call(Method::POST, &ack, Some(c.owner), None).await;
            assert_eq!(status, StatusCode::OK);
        }
        let (_, body, _) = c
            .call(
                Method::GET,
                &format!("{}?page=1", uri(c.listing)),
                Some(c.owner),
                None,
            )
            .await;
        assert_eq!(body["open"].as_array().map(Vec::len), Some(0));
        assert_eq!(body["history"]["total"], 1);
        assert_eq!(body["history"]["per_page"], 20);
        let (_, body, _) = c
            .call(
                Method::GET,
                &format!("{}?page=2", uri(c.listing)),
                Some(c.owner),
                None,
            )
            .await;
        assert_eq!(body["history"]["items"].as_array().map(Vec::len), Some(0));
    }

    #[actix_web::test]
    async fn callers_are_checked() {
        let c = ctx().await;
        let some = Uuid::now_v7();
        for (method, path) in [
            (Method::GET, uri(c.listing)),
            (
                Method::POST,
                format!("{}/{some}/acknowledge", uri(c.listing)),
            ),
        ] {
            let (status, _, _) = c.call(method.clone(), &path, None, None).await;
            assert_eq!(status, StatusCode::UNAUTHORIZED, "{method} {path}");
            let (status, _, _) = c
                .call(method.clone(), &path, Some(c.other_host), None)
                .await;
            assert_eq!(status, StatusCode::FORBIDDEN, "{method} {path}");
        }
    }
}

mod overview {
    use super::*;
    use actix_web::http::Method;

    #[actix_web::test]
    async fn the_overview_is_scoped_to_the_caller() {
        let c = ctx().await;
        booking(&c, c.listing, future(11), future(13)).await;
        c.call(
            Method::POST,
            &format!("/api/v1/listings/{}/calendar/feeds", c.listing),
            Some(c.owner),
            Some(serde_json::json!({ "name": "Airbnb", "url": c.feed_url("/ok.ics") })),
        )
        .await;
        let failing = c.insert_feed(c.other_listing, "/503.ics").await;
        sqlx::query("UPDATE listing_calendar_feed SET consecutive_failures = 5 WHERE id = $1")
            .bind(failing)
            .execute(&c.pool)
            .await
            .expect("failing");

        let (status, _, _) = c
            .call(Method::GET, "/api/v1/calendar/overview", None, None)
            .await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
        let (_, mine, _) = c
            .call(
                Method::GET,
                "/api/v1/calendar/overview",
                Some(c.owner),
                None,
            )
            .await;
        assert_eq!(
            (
                mine["open_conflicts"].as_i64(),
                mine["failing_feeds"].as_i64()
            ),
            (Some(1), Some(0))
        );
        let (_, theirs, _) = c
            .call(
                Method::GET,
                "/api/v1/calendar/overview",
                Some(c.other_host),
                None,
            )
            .await;
        assert_eq!(
            (
                theirs["open_conflicts"].as_i64(),
                theirs["failing_feeds"].as_i64()
            ),
            (Some(0), Some(1))
        );
        let (_, all, _) = c
            .call(
                Method::GET,
                "/api/v1/calendar/overview",
                Some(c.admin),
                None,
            )
            .await;
        assert_eq!(
            (
                all["open_conflicts"].as_i64(),
                all["failing_feeds"].as_i64()
            ),
            (Some(1), Some(1))
        );
        assert_eq!(all["listings"].as_array().map(Vec::len), Some(2));
    }
}

mod export {
    use super::*;
    use actix_web::http::Method;

    async fn get(
        c: &Ctx,
        uri: &str,
        if_none_match: Option<&str>,
    ) -> (StatusCode, String, actix_web::http::header::HeaderMap) {
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(c.pool.clone()))
                .app_data(web::Data::new(settings()))
                .configure(crate::apis::configure_routes),
        )
        .await;
        let mut req = test::TestRequest::get().uri(uri);
        if let Some(tag) = if_none_match {
            req = req.insert_header(("If-None-Match", tag));
        }
        let resp = test::call_service(&app, req.to_request()).await;
        let status = resp.status();
        let headers = resp.headers().clone();
        let body = String::from_utf8(test::read_body(resp).await.to_vec()).expect("utf8");
        (status, body, headers)
    }

    #[actix_web::test]
    async fn the_export_is_busy_only_with_caching_headers() {
        let c = ctx().await;
        let guest: Uuid = sqlx::query_scalar(
            r#"INSERT INTO "user" (email, password_hash, first_name, last_name, slug, phone_number)
               VALUES ('secret.guest@example.com', 'x', 'Zelda', 'Guestname', 'zelda-' || gen_random_uuid(), '+18765550123')
               RETURNING id"#,
        )
        .fetch_one(&c.pool)
        .await
        .expect("guest");
        let code: String = sqlx::query_scalar(
            "INSERT INTO booking (confirmation_code, guest_id, listing_id, status, date_from, date_to,
                 currency, daily_rate, number_of_persons, total_days, sub_total_price, total_price,
                 cancellation_policy, metadata)
             VALUES ('EXPORT1234', $1, $2, 'confirmed', $3, $4, 'USD', 100, 2, 2, 200, 200,
                 'flexible', '{\"num_adults\": 2, \"message_to_host\": \"secret note\"}'::jsonb)
             RETURNING confirmation_code",
        )
        .bind(guest)
        .bind(c.listing)
        .bind(future(5))
        .bind(future(7))
        .fetch_one(&c.pool)
        .await
        .expect("booking");
        let uri = format!("/api/v1/listings/{}/calendar.ics", c.listing);
        let (status, body, headers) = get(&c, &uri, None).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(
            headers.get("content-type").and_then(|v| v.to_str().ok()),
            Some("text/calendar; charset=utf-8")
        );
        assert_eq!(
            headers.get("cache-control").and_then(|v| v.to_str().ok()),
            Some("public, max-age=300")
        );
        assert!(
            headers
                .get("content-disposition")
                .and_then(|v| v.to_str().ok())
                .is_some_and(|v| v.starts_with("inline; filename=\"ourplaces-")
                    && v.ends_with("-calendar.ics\""))
        );
        assert!(body.contains(&format!("Reservation #: {code}")));
        assert!(body.contains(&format!(
            "DTSTART;VALUE=DATE:{}",
            future(5).format("%Y%m%d")
        )));
        for pii in [
            "Zelda",
            "Guestname",
            "secret.guest",
            "5550123",
            "secret note",
        ] {
            assert!(!body.contains(pii), "{pii} leaked");
        }

        let etag = headers
            .get("etag")
            .and_then(|v| v.to_str().ok())
            .expect("etag")
            .to_string();
        let (status, body, _) = get(&c, &uri, Some(&etag)).await;
        assert_eq!(status, StatusCode::NOT_MODIFIED);
        assert!(body.is_empty());
        let (status, _, _) = get(&c, &uri, Some("\"stale\"")).await;
        assert_eq!(status, StatusCode::OK);
        // No user token needed; another host's token changes nothing.
        let (status, _, _) = c.call(Method::GET, &uri, Some(c.other_host), None).await;
        assert_eq!(status, StatusCode::OK);
    }

    #[actix_web::test]
    async fn deleted_unknown_and_malformed_ids_are_404_and_inactive_is_served() {
        let c = ctx().await;
        sqlx::query("UPDATE listing SET is_active = FALSE WHERE id = $1")
            .bind(c.listing)
            .execute(&c.pool)
            .await
            .expect("deactivate");
        let (status, _, _) = get(
            &c,
            &format!("/api/v1/listings/{}/calendar.ics", c.listing),
            None,
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        sqlx::query("UPDATE listing SET deleted_at = NOW() WHERE id = $1")
            .bind(c.other_listing)
            .execute(&c.pool)
            .await
            .expect("delete");
        for uri in [
            format!("/api/v1/listings/{}/calendar.ics", c.other_listing),
            format!("/api/v1/listings/{}/calendar.ics", Uuid::now_v7()),
            "/api/v1/listings/not-a-uuid/calendar.ics".to_string(),
        ] {
            let (status, body, _) = get(&c, &uri, None).await;
            assert_eq!(status, StatusCode::NOT_FOUND, "{uri}");
            assert!(!body.contains("<html"), "plain 404");
        }
    }
}
