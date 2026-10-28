//! Guards the guest theme against drifting from the locked design reference.

use std::collections::BTreeMap;

const DESIGN_DOC: &str = include_str!("../../docs/design/guest-theme.md");
const GUEST_CSS: &str = include_str!("../src/style/guest.css");

/// Collects `--name: value;` declarations per selector block (`:root`,
/// `.dim`, `.dark`), lower-casing values and dropping comments.
fn tokens(css: &str) -> BTreeMap<String, BTreeMap<String, String>> {
    let mut blocks = BTreeMap::new();
    let mut current: Option<String> = None;
    for line in css.lines() {
        let line = line.split("/*").next().unwrap_or("").trim();
        if let Some(sel) = line.strip_suffix('{').map(str::trim) {
            current = [":root", ".dim", ".dark"]
                .contains(&sel)
                .then(|| sel.to_string());
        } else if line == "}" {
            current = None;
        } else if let (Some(sel), Some((name, value))) = (&current, line.split_once(':')) {
            if let Some(name) = name.trim().strip_prefix("--") {
                let entry: &mut BTreeMap<String, String> = blocks.entry(sel.clone()).or_default();
                entry.insert(
                    name.to_string(),
                    value.trim().trim_end_matches(';').trim().to_lowercase(),
                );
            }
        }
    }
    blocks
}

fn design_css() -> &'static str {
    let start = DESIGN_DOC.find("```css").map(|i| i + "```css".len());
    let end = start.and_then(|s| DESIGN_DOC[s..].find("```").map(|e| s + e));
    match (start, end) {
        (Some(s), Some(e)) => &DESIGN_DOC[s..e],
        _ => "",
    }
}

#[test]
fn guest_tokens_match_the_design_reference() {
    let design = tokens(design_css());
    let guest = tokens(GUEST_CSS);
    assert_eq!(
        design.len(),
        3,
        "design doc should define :root, .dim and .dark"
    );
    for (selector, expected) in &design {
        let actual = guest.get(selector).cloned().unwrap_or_default();
        for (name, value) in expected {
            assert_eq!(
                actual.get(name),
                Some(value),
                "{selector} --{name} differs from docs/design/guest-theme.md"
            );
        }
    }
}

#[test]
fn all_three_guest_themes_set_every_light_token() {
    let guest = tokens(GUEST_CSS);
    let root = guest.get(":root").cloned().unwrap_or_default();
    for theme in [".dim", ".dark"] {
        let set = guest.get(theme).cloned().unwrap_or_default();
        for name in root
            .keys()
            .filter(|n| !["link", "shadow-xs", "shadow-sm"].contains(&n.as_str()))
        {
            assert!(set.contains_key(name), "{theme} is missing --{name}");
        }
    }
}
