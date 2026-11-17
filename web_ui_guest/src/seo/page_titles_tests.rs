//! Spec 102 §1.5.3: static pages set their own `<title>`. Rendering a guest
//! page needs the asset bundle, so E6 checks the rendered titles; this
//! checks the table and that each page attaches its entry.

use super::titles;

const PAGES: [(&str, &str, &str); 8] = [
    ("listings.rs", "/listings", "LISTINGS"),
    ("about.rs", "/about", "ABOUT"),
    ("policies.rs", "/terms", "TERMS"),
    ("policies.rs", "/cancellation-policy", "CANCELLATION"),
    ("policies.rs", "/privacy-policy", "PRIVACY"),
    ("policies.rs", "/house-rules", "HOUSE_RULES"),
    ("auth.rs", "/login", "SIGN_IN"),
    ("register.rs", "/register", "REGISTER"),
];

#[test]
fn page_titles_end_with_the_site_name() {
    for (title, expected) in [
        (titles::LISTINGS, "Villas in Jamaica · Our Places"),
        (titles::ABOUT, "About Our Places · Our Places"),
        (titles::TERMS, "Terms and conditions · Our Places"),
        (titles::CANCELLATION, "Cancellation policy · Our Places"),
        (titles::PRIVACY, "Privacy policy · Our Places"),
        (titles::HOUSE_RULES, "House rules · Our Places"),
        (titles::SIGN_IN, "Sign in · Our Places"),
        (titles::REGISTER, "Register · Our Places"),
    ] {
        assert_eq!(web_ui_common::site_config::page_title(title), expected);
    }
}

#[test]
fn page_titles_attach_each_title_to_its_page() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/pages");
    for (file, path, constant) in PAGES {
        let src = std::fs::read_to_string(dir.join(file)).unwrap_or_default();
        let expected = format!(r#"#[page("{path}", with(titles::metadata(titles::{constant})))]"#);
        assert!(src.contains(&expected), "{file} lacks {expected}");
    }
}
