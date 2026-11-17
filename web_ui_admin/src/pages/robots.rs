//! `/robots.txt` for the admin portal (spec 102 §1.3): always disallow
//! everything. Public on purpose (no sign-in guard): crawlers must read it.
//! Every admin response also carries `X-Robots-Tag: noindex, nofollow`
//! (`SecurityHeaders::admin`).

use topcoat::{
    Result,
    context::Cx,
    router::{error::internal_server_error, response::Response, route},
};

/// The admin robots.txt body.
pub const ADMIN_ROBOTS_TXT: &str = "User-agent: *\nDisallow: /\n";

/// The robots.txt response: plain text, cacheable for an hour.
pub fn robots_response() -> Result<Response> {
    Response::builder()
        .status(200)
        .header("content-type", "text/plain; charset=utf-8")
        .header("cache-control", "public, max-age=3600")
        .body(ADMIN_ROBOTS_TXT.to_string().into())
        .map_err(|e| internal_server_error(e).into())
}

/// Disallow everything, in every environment.
#[route(GET "/robots.txt")]
pub async fn robots(cx: &Cx) -> Result<Response> {
    let _ = cx;
    robots_response()
}

#[cfg(test)]
mod robots_tests {
    use super::*;

    #[test]
    fn admin_robots_disallows_everything() {
        assert_eq!(ADMIN_ROBOTS_TXT, "User-agent: *\nDisallow: /\n");
    }

    #[test]
    fn admin_robots_is_plain_cacheable_text() {
        let res = robots_response();
        let res = match res {
            Ok(r) => r,
            Err(e) => panic!("robots response: {e:?}"),
        };
        assert_eq!(res.status(), 200);
        assert_eq!(res.headers()["content-type"], "text/plain; charset=utf-8");
        assert_eq!(res.headers()["cache-control"], "public, max-age=3600");
    }
}
