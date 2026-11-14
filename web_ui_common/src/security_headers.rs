use topcoat::context::Cx;
use topcoat::router::header::{HeaderName, HeaderValue};
use topcoat::router::response::response_headers;
use topcoat::router::{Body, Layer, LayerFuture, Next, Path};

use crate::map_config::MapConfig;

/// Content-Security-Policy for both portals. No inline scripts or styles and
/// no `eval` (the Topcoat runtime is not used; spec Phase 0 spike results).
/// Listing photos are served from Cloud Storage.
pub const CONTENT_SECURITY_POLICY: &str = "default-src 'self'; \
script-src 'self'; \
style-src 'self'; \
img-src 'self' https://storage.googleapis.com data:; \
font-src 'self'; \
connect-src 'self'; \
frame-ancestors 'none'; \
base-uri 'self'; \
form-action 'self'; \
object-src 'none'";

/// Content-Security-Policy for the admin portal: as [`CONTENT_SECURITY_POLICY`],
/// plus Cloud Storage in `connect-src` for direct signed-URL photo uploads.
pub const ADMIN_CONTENT_SECURITY_POLICY: &str = "default-src 'self'; \
script-src 'self'; \
style-src 'self'; \
img-src 'self' https://storage.googleapis.com data:; \
font-src 'self'; \
connect-src 'self' https://storage.googleapis.com; \
frame-ancestors 'none'; \
base-uri 'self'; \
form-action 'self'; \
object-src 'none'";

/// `Cache-Control` for HTML: pages vary by session and theme cookies, so
/// shared caches must not store them.
pub const HTML_CACHE_CONTROL: &str = "private, no-cache";

const HSTS: &str = "max-age=31536000; includeSubDomains";

/// Router layer that adds the CSP and hardening headers to every response
/// (including error pages), and private caching plus `Vary: Cookie` to HTML.
///
/// Cross-site request forgery is handled by Topcoat's default origin policy,
/// which rejects cross-origin state-changing requests.
#[derive(Debug, Clone, Default)]
pub struct SecurityHeaders {
    hsts: bool,
    admin: bool,
    storage_origin: Option<String>,
    map: Option<MapConfig>,
}

/// `csp` with `origin` allowed wherever Cloud Storage is (`img-src`, and
/// `connect-src` for the admin portal). Origins that could break out of a
/// directive are refused.
pub fn csp_with_storage_origin(csp: &str, origin: &str) -> Option<String> {
    Some(origin)
        .filter(|o| !o.is_empty() && !o.contains([';', ',', '\'', ' ', '\n']))
        .map(|o| {
            csp.replace(
                "https://storage.googleapis.com",
                &format!("https://storage.googleapis.com {o}"),
            )
        })
}

/// `csp` with the map tiles allowed (spec 103 §1.12): the tile bucket path
/// in `connect-src` (style, tiles, glyphs and sprites are all fetched), and
/// `worker-src 'self'` for MapLibre's module worker. `img-src` is unchanged
/// (Phase 0 spike, spec §7).
///
/// Apply after [`csp_with_storage_origin`], which rewrites every
/// `https://storage.googleapis.com` and would otherwise widen the bucket
/// path to the whole host.
pub fn csp_with_map_origin(csp: &str, map: &MapConfig) -> String {
    csp.split("; ")
        .map(|directive| {
            if directive.starts_with("connect-src ") {
                format!("{directive} {}", map.origin())
            } else {
                directive.to_string()
            }
        })
        .chain(std::iter::once("worker-src 'self'".to_string()))
        .collect::<Vec<_>>()
        .join("; ")
}

impl SecurityHeaders {
    /// Headers without HSTS (local development over plain HTTP).
    pub fn new() -> Self {
        Self::default()
    }

    /// The admin portal's headers ([`ADMIN_CONTENT_SECURITY_POLICY`]).
    pub fn admin() -> Self {
        Self {
            admin: true,
            ..Self::default()
        }
    }

    /// Also send `Strict-Transport-Security` (production, behind HTTPS).
    pub fn with_hsts(self, hsts: bool) -> Self {
        Self { hsts, ..self }
    }

    /// Also allow a local Cloud Storage emulator's origin (development only;
    /// see [`common::gcs::emulator_endpoint`]). `None` changes nothing.
    pub fn with_storage_origin(self, origin: Option<String>) -> Self {
        Self {
            storage_origin: origin,
            ..self
        }
    }

    /// Also allow the map tiles' bucket path ([`csp_with_map_origin`]).
    /// `None` (maps off) changes nothing.
    pub fn with_map_origin(self, map: Option<&MapConfig>) -> Self {
        Self {
            map: map.cloned(),
            ..self
        }
    }

    fn content_security_policy(&self) -> HeaderValue {
        let base = if self.admin {
            ADMIN_CONTENT_SECURITY_POLICY
        } else {
            CONTENT_SECURITY_POLICY
        };
        if self.storage_origin.is_none() && self.map.is_none() {
            return HeaderValue::from_static(base);
        }
        // Storage origin first: it rewrites every Cloud Storage occurrence.
        let csp = self
            .storage_origin
            .as_deref()
            .and_then(|origin| csp_with_storage_origin(base, origin))
            .unwrap_or_else(|| base.to_string());
        let csp = self
            .map
            .as_ref()
            .map(|map| csp_with_map_origin(&csp, map))
            .unwrap_or(csp);
        HeaderValue::from_str(&csp).unwrap_or_else(|_| HeaderValue::from_static(base))
    }

    fn static_headers(&self) -> impl Iterator<Item = (&'static str, &'static str)> + '_ {
        [
            ("x-content-type-options", "nosniff"),
            ("x-frame-options", "DENY"),
            ("referrer-policy", "strict-origin-when-cross-origin"),
            (
                "permissions-policy",
                "camera=(), microphone=(), geolocation=()",
            ),
            ("cross-origin-opener-policy", "same-origin"),
        ]
        .into_iter()
        .chain(self.hsts.then_some(("strict-transport-security", HSTS)))
    }
}

impl Layer for SecurityHeaders {
    fn path(&self) -> Option<&Path> {
        None
    }

    fn handle<'a>(&'a self, cx: &'a Cx, body: Body, next: Next<'a>) -> LayerFuture<'a> {
        Box::pin(async move {
            let queued = response_headers(cx);
            queued.append(
                HeaderName::from_static("content-security-policy"),
                self.content_security_policy(),
            );
            for (name, value) in self.static_headers() {
                queued.append(
                    HeaderName::from_static(name),
                    HeaderValue::from_static(value),
                );
            }
            let mut response = next.run(cx, body).await?;
            let is_html = response
                .headers()
                .get("content-type")
                .and_then(|v| v.to_str().ok())
                .is_some_and(|v| v.starts_with("text/html"));
            if is_html {
                let headers = response.headers_mut();
                headers.insert(
                    HeaderName::from_static("cache-control"),
                    HeaderValue::from_static(HTML_CACHE_CONTROL),
                );
                headers.append(
                    HeaderName::from_static("vary"),
                    HeaderValue::from_static("Cookie"),
                );
            }
            Ok(response)
        })
    }
}
