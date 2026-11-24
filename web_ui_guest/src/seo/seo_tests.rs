use std::path::Path;

use topcoat::context::BaseUrl;
use web_ui_common::security_headers::robots_rule_matches;

use super::*;

fn base() -> BaseUrl {
    match BaseUrl::new("https://ourplaces.io") {
        Ok(b) => b,
        Err(e) => panic!("test base URL: {e}"),
    }
}

// ---- U2: robots.txt ----

#[test]
fn robots_disallows_everything_when_not_indexable() {
    assert_eq!(robots_txt(&base(), false), "User-agent: *\nDisallow: /\n");
}

#[test]
fn robots_disallows_every_private_prefix_when_indexable() {
    let body = robots_txt(&base(), true);
    assert!(body.starts_with("User-agent: *\n"), "{body}");
    for prefix in PRIVATE_PREFIXES {
        assert!(
            body.lines().any(|l| l == format!("Disallow: {prefix}")),
            "{prefix} missing:\n{body}"
        );
    }
    assert!(body.lines().any(|l| l == "Allow: /"), "{body}");
    assert!(!body.lines().any(|l| l == "Disallow: /"), "{body}");
}

#[test]
fn robots_names_the_absolute_sitemap_when_indexable() {
    let body = robots_txt(&base(), true);
    assert!(
        body.lines()
            .any(|l| l == "Sitemap: https://ourplaces.io/sitemap.xml"),
        "{body}"
    );
    assert!(!robots_txt(&base(), false).contains("Sitemap:"));
}

#[test]
fn robots_never_blocks_public_pages_or_assets() {
    for path in [
        "/",
        "/listings",
        "/listings/villa-x",
        "/about",
        "/terms",
        "/sitemap.xml",
        "/assets/app-1a2b.css",
    ] {
        assert!(
            !PRIVATE_PREFIXES
                .iter()
                .any(|rule| robots_rule_matches(rule, path)),
            "{path} is blocked"
        );
    }
}

// ---- U5: every guarded page is disallowed ----

/// `(path, handler body)` for each `#[page(..)]` / `#[route(..)]` in `src`.
fn routes(src: &str) -> Vec<(String, String)> {
    let starts: Vec<usize> = src
        .match_indices("#[page(")
        .chain(src.match_indices("#[route("))
        .map(|(at, _)| at)
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect();
    starts
        .iter()
        .enumerate()
        .filter_map(|(n, &start)| {
            let end = starts.get(n + 1).copied().unwrap_or(src.len());
            let attr = &src[start..end];
            let path = attr.split('"').nth(1)?;
            path.starts_with('/')
                .then(|| (path.to_string(), attr.to_string()))
        })
        .collect()
}

/// `/bookings/{id}/cancel` → `/bookings/x/cancel`.
fn concrete(path: &str) -> String {
    path.split('/')
        .map(|seg| if seg.starts_with('{') { "x" } else { seg })
        .collect::<Vec<_>>()
        .join("/")
}

fn is_guarded(body: &str) -> bool {
    body.contains("guest(cx)") || body.contains("require_guest(")
}

#[test]
fn every_guarded_guest_page_is_disallowed() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/pages");
    let mut guarded = Vec::new();
    let entries = std::fs::read_dir(&dir).map_err(|e| e.to_string());
    for entry in entries.into_iter().flatten().flatten() {
        let file = entry.path();
        if file.extension().and_then(|e| e.to_str()) != Some("rs") {
            continue;
        }
        let src = std::fs::read_to_string(&file).unwrap_or_default();
        guarded.extend(
            routes(&src)
                .into_iter()
                .filter(|(_, body)| is_guarded(body))
                .map(|(path, _)| path),
        );
    }
    // The scan must actually find the account pages, or it proves nothing.
    assert!(guarded.iter().any(|p| p == "/bookings"), "{guarded:?}");
    assert!(
        guarded.iter().any(|p| p.starts_with("/checkout/")),
        "{guarded:?}"
    );
    for path in &guarded {
        let path = concrete(path);
        assert!(
            PRIVATE_PREFIXES
                .iter()
                .any(|rule| robots_rule_matches(rule, &path)),
            "guarded page {path} is not in PRIVATE_PREFIXES"
        );
    }
}

#[test]
fn route_scan_finds_paths_and_bodies() {
    let src = r#"
#[page("/a")]
async fn a(cx: &Cx) { let g = guest(cx).await?; }
#[page(POST "/b/{id}")]
async fn b(cx: &Cx) {}
"#;
    let found = routes(src);
    assert_eq!(found.len(), 2);
    assert_eq!(found[0].0, "/a");
    assert!(is_guarded(&found[0].1));
    assert_eq!(found[1].0, "/b/{id}");
    assert!(!is_guarded(&found[1].1));
    assert_eq!(concrete("/b/{id}/c"), "/b/x/c");
}
