//! `/robots.txt` and `/sitemap.xml` (spec 102 §1.2–1.3). Formatting lives in
//! [`crate::seo`]; these routes only gather inputs and build responses.

use topcoat::{
    Result,
    context::Cx,
    router::{error::internal_server_error, response::Response, route},
};
use web_ui_common::site_config::site_config;

use crate::seo::{DISALLOW_ALL, robots_txt};

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
