//! JSON-LD structured data in a `<script type="application/ld+json">`
//! (spec 102 §1.4.7).
//!
//! A script element's content is raw text: the browser does not decode
//! entities there, so Topcoat's normal text escaping (`&` to `&amp;`) would
//! corrupt names. The JSON is made safe instead: every character that could
//! end the element or change the parser's state becomes its JSON escape, so
//! the output contains no raw `<` at all (no `</script`, `<!--` or
//! `<script`). This module is the only place in the portals that writes
//! unescaped markup.

use topcoat::{
    Result,
    context::Cx,
    view::{Unescaped, View, component, view},
};

/// LINE SEPARATOR and PARAGRAPH SEPARATOR: valid in JSON strings, but line
/// terminators to older JavaScript parsers.
const SEPARATORS: [u32; 2] = [0x2028, 0x2029];

/// `json` made safe inside `<script type="application/ld+json">`: each `<`,
/// `>`, `&`, U+2028 and U+2029 becomes its six-character JSON escape (a
/// backslash, `u`, four lowercase hex digits). This is applied to
/// serialized JSON, where every literal backslash is already doubled, so
/// the result parses back to the same value.
pub fn escape_json_for_script(json: &str) -> String {
    json.chars()
        .fold(String::with_capacity(json.len()), |mut out, c| {
            if matches!(c, '<' | '>' | '&') || SEPARATORS.contains(&(c as u32)) {
                out.push('\\');
                out.push('u');
                out.push_str(&format!("{:04x}", c as u32));
            } else {
                out.push(c);
            }
            out
        })
}

/// A `<script type="application/ld+json">` holding `value`. If `value` can't
/// be serialized, nothing is rendered (and a warning is logged): the page
/// works without structured data.
#[component]
pub async fn json_ld(
    cx: &Cx,
    /// The structured data.
    value: serde_json::Value,
) -> Result<impl View> {
    let _ = cx;
    let body = serde_json::to_string(&value)
        .map(|json| escape_json_for_script(&json))
        .inspect_err(|e| tracing::warn!(error = %e, "JSON-LD not rendered"))
        .ok();
    Ok(view! {
        if let Some(body) = body {
            <script type="application/ld+json">(Unescaped::new_unchecked(body))</script>
        }
    })
}
