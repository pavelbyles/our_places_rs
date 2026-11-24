//! The public calendar feed channels subscribe to (spec §1.6):
//! `GET /listings/{id}/calendar.ics`.
//!
//! It asks `listing_api` with the service identity only and never forwards an
//! inbound header. Answers are cached in process for 60 s, 404s included (in
//! their own, smaller store, so random ids can't push real listings out).
//! Upstream errors aren't cached and give a plain 503.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use topcoat::{
    Result,
    context::Cx,
    router::{path_param, request::headers, response::Response, route},
};
use uuid::Uuid;
use web_client::TopcoatApiClient;
use web_client::calendar::IcsFetch;

path_param!(slug);

/// How long a cached answer is served.
pub const CACHE_TTL: Duration = Duration::from_secs(60);
/// Most cached calendars.
pub const MAX_CACHED: usize = 1_000;
/// Most cached 404s.
pub const MAX_CACHED_MISSING: usize = 200;
/// `Cache-Control` of a served calendar.
pub const CACHE_CONTROL: &str = "public, max-age=300";

#[derive(Clone)]
struct Cached {
    body: String,
    etag: Option<String>,
    slug: Option<String>,
    at: Instant,
}

/// The in-process export cache.
#[derive(Default)]
pub struct ExportCache {
    found: Mutex<HashMap<Uuid, Cached>>,
    missing: Mutex<HashMap<Uuid, Instant>>,
}

/// Inserts into a bounded map, evicting the oldest entry when full.
fn bounded_insert<V>(
    map: &mut HashMap<Uuid, V>,
    key: Uuid,
    value: V,
    cap: usize,
    at: impl Fn(&V) -> Instant,
) {
    if !map.contains_key(&key) && map.len() >= cap {
        let oldest = map.iter().min_by_key(|(_, v)| at(v)).map(|(k, _)| *k);
        if let Some(oldest) = oldest {
            map.remove(&oldest);
        }
    }
    map.insert(key, value);
}

impl ExportCache {
    /// The process-wide cache.
    pub fn global() -> &'static ExportCache {
        static CACHE: OnceLock<ExportCache> = OnceLock::new();
        CACHE.get_or_init(ExportCache::default)
    }

    fn fresh_missing(&self, id: Uuid) -> bool {
        self.missing
            .lock()
            .map(|m| m.get(&id).is_some_and(|at| at.elapsed() < CACHE_TTL))
            .unwrap_or(false)
    }

    fn get(&self, id: Uuid) -> Option<Cached> {
        self.found.lock().ok().and_then(|m| m.get(&id).cloned())
    }

    fn put(&self, id: Uuid, entry: Cached) {
        if let Ok(mut m) = self.found.lock() {
            bounded_insert(&mut m, id, entry, MAX_CACHED, |c| c.at);
        }
        if let Ok(mut m) = self.missing.lock() {
            m.remove(&id);
        }
    }

    fn put_missing(&self, id: Uuid) {
        if let Ok(mut m) = self.missing.lock() {
            bounded_insert(&mut m, id, Instant::now(), MAX_CACHED_MISSING, |at| *at);
        }
        if let Ok(mut m) = self.found.lock() {
            m.remove(&id);
        }
    }
}

/// What to answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExportReply {
    /// 200 with the calendar.
    Calendar {
        /// The `.ics` body.
        body: String,
        /// Its ETag.
        etag: Option<String>,
        /// The listing, for the file name.
        listing_id: Uuid,
        /// The listing slug, for the file name.
        slug: Option<String>,
    },
    /// 304: the subscriber's copy is current.
    NotModified {
        /// The ETag.
        etag: Option<String>,
    },
    /// 404, plain text.
    NotFound,
    /// 503, plain text, not cached.
    Unavailable,
}

fn etag_matches(if_none_match: Option<&str>, etag: Option<&str>) -> bool {
    match (if_none_match, etag) {
        (Some(inm), Some(tag)) => inm.split(',').any(|t| t.trim() == tag || t.trim() == "*"),
        _ => false,
    }
}

/// Decides the answer for `raw_id`. `if_none_match` is the subscriber's
/// header, compared here and never sent upstream.
pub async fn export_reply(
    client: &TopcoatApiClient,
    cache: &ExportCache,
    raw_id: &str,
    if_none_match: Option<&str>,
) -> ExportReply {
    let Ok(listing_id) = Uuid::parse_str(raw_id) else {
        return ExportReply::NotFound;
    };
    if cache.fresh_missing(listing_id) {
        return ExportReply::NotFound;
    }
    let cached = cache.get(listing_id);
    let entry = match cached {
        Some(c) if c.at.elapsed() < CACHE_TTL => c,
        stale => {
            let ours = stale.as_ref().and_then(|c| c.etag.as_deref());
            match client.get_calendar_ics(listing_id, ours).await {
                Ok(IcsFetch::Ok { body, etag, slug }) => {
                    let fresh = Cached {
                        body,
                        etag,
                        slug,
                        at: Instant::now(),
                    };
                    cache.put(listing_id, fresh.clone());
                    fresh
                }
                Ok(IcsFetch::NotModified { etag }) => match stale {
                    Some(old) => {
                        let fresh = Cached {
                            etag: etag.or(old.etag),
                            slug: old.slug,
                            at: Instant::now(),
                            body: old.body,
                        };
                        cache.put(listing_id, fresh.clone());
                        fresh
                    }
                    None => {
                        tracing::error!(%listing_id, "calendar export: 304 without a cached copy");
                        return ExportReply::Unavailable;
                    }
                },
                Ok(IcsFetch::NotFound) => {
                    cache.put_missing(listing_id);
                    return ExportReply::NotFound;
                }
                Err(e) => {
                    tracing::error!(%listing_id, error = %e, "calendar export unavailable");
                    return ExportReply::Unavailable;
                }
            }
        }
    };
    match etag_matches(if_none_match, entry.etag.as_deref()) {
        true => ExportReply::NotModified { etag: entry.etag },
        false => ExportReply::Calendar {
            body: entry.body,
            etag: entry.etag,
            listing_id,
            slug: entry.slug,
        },
    }
}

fn plain(status: u16, text: &'static str) -> Result<Response> {
    Response::builder()
        .status(status)
        .header("content-type", "text/plain; charset=utf-8")
        .body(text.into())
        .map_err(|e| web_client::app_error(e).into())
}

/// The listing's public calendar.
#[route(GET "/listings/{slug}/calendar.ics")]
pub async fn calendar_ics(cx: &Cx) -> Result<Response> {
    let raw: &str = slug(cx);
    let if_none_match = headers(cx)
        .get("if-none-match")
        .and_then(|v| v.to_str().ok());
    let client = web_client::get_api_client(cx);
    match export_reply(client, ExportCache::global(), raw, if_none_match).await {
        ExportReply::Calendar {
            body,
            etag,
            listing_id,
            slug,
        } => {
            let fallback = listing_id.to_string();
            let file_slug = slug.as_deref().unwrap_or(&fallback);
            let mut builder = Response::builder()
                .status(200)
                .header("content-type", "text/calendar; charset=utf-8")
                .header("cache-control", CACHE_CONTROL)
                .header(
                    "content-disposition",
                    format!("inline; filename=\"ourplaces-{file_slug}-calendar.ics\""),
                );
            if let Some(tag) = etag {
                builder = builder.header("etag", tag);
            }
            builder
                .body(body.into())
                .map_err(|e| web_client::app_error(e).into())
        }
        ExportReply::NotModified { etag } => {
            let mut builder = Response::builder()
                .status(304)
                .header("cache-control", CACHE_CONTROL);
            if let Some(tag) = etag {
                builder = builder.header("etag", tag);
            }
            builder
                .body(String::new().into())
                .map_err(|e| web_client::app_error(e).into())
        }
        ExportReply::NotFound => plain(404, "Not found"),
        ExportReply::Unavailable => plain(503, "Calendar temporarily unavailable"),
    }
}
