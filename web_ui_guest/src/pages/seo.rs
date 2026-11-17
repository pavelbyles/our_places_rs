//! `/robots.txt` and `/sitemap.xml` (spec 102 §1.2–1.3). Formatting lives in
//! [`crate::seo`]; these routes only gather inputs and build responses.

use topcoat::{
    Result,
    context::Cx,
    router::{
        error::internal_server_error,
        header::{HeaderName, HeaderValue},
        response::{IntoResponse, Response, response_headers},
        route,
    },
};
use web_client::get_api_client;
use web_ui_common::site_config::site_config;

use crate::seo::{DISALLOW_ALL, collect_listings, robots_txt, sitemap_for};

/// Crawlers may cache robots.txt and the sitemap for an hour; neither
/// varies by cookie or host.
pub const SEO_CACHE_CONTROL: &str = "public, max-age=3600";

fn text_response(content_type: &str, body: String) -> Result<Response> {
    Response::builder()
        .status(200)
        .header("content-type", content_type)
        .header("cache-control", SEO_CACHE_CONTROL)
        .body(body.into())
        .map_err(|e| internal_server_error(e).into())
}

/// The guest robots.txt. Without a site configuration (never the case in a
/// running portal) it disallows everything: fail closed.
#[route(GET "/robots.txt")]
pub async fn robots(cx: &Cx) -> Result<Response> {
    let body = site_config(cx)
        .map(|site| robots_txt(site.base_url(), site.indexable()))
        .unwrap_or_else(|| DISALLOW_ALL.to_string());
    text_response("text/plain; charset=utf-8", body)
}

/// How long a crawler should wait before retrying a failed sitemap.
const SITEMAP_RETRY_AFTER_SECONDS: &str = "300";

/// The sitemap (spec 102 §1.2), built with Topcoat's `Sitemap`. If any page
/// of the listing search fails, the reply is a 503 to retry later: never a
/// partial sitemap, which would tell crawlers the missing listings are gone.
#[route(GET "/sitemap.xml")]
pub async fn sitemap(cx: &Cx) -> Result<Response> {
    match collect_listings(get_api_client(cx)).await {
        Ok(listings) => {
            response_headers(cx).append(
                HeaderName::from_static("cache-control"),
                HeaderValue::from_static(SEO_CACHE_CONTROL),
            );
            sitemap_for(listings).into_response(cx)
        }
        Err(e) => {
            tracing::error!(error = %e, "sitemap unavailable");
            Response::builder()
                .status(503)
                .header("content-type", "text/plain; charset=utf-8")
                .header("cache-control", "no-store")
                .header("retry-after", SITEMAP_RETRY_AFTER_SECONDS)
                .body("Sitemap temporarily unavailable".to_string().into())
                .map_err(|e| internal_server_error(e).into())
        }
    }
}
