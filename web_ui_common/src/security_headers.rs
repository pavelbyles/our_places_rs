use topcoat::context::Cx;
use topcoat::router::header::{HeaderName, HeaderValue};
use topcoat::router::response::response_headers;
use topcoat::router::{Body, Layer, LayerFuture, Next, Path};

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

    fn content_security_policy(&self) -> HeaderValue {
        let base = if self.admin {
            ADMIN_CONTENT_SECURITY_POLICY
        } else {
            CONTENT_SECURITY_POLICY
        };
        self.storage_origin
            .as_deref()
            .and_then(|origin| csp_with_storage_origin(base, origin))
            .and_then(|csp| HeaderValue::from_str(&csp).ok())
            .unwrap_or_else(|| HeaderValue::from_static(base))
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
