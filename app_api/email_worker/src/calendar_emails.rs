//! Calendar sync emails to hosts (#58): conflicts and failing feeds.
//!
//! Every payload value is HTML-escaped. Payloads never hold a feed URL; the
//! only link is the admin calendar page, and only http(s) links render.

use crate::processor::{escape_html, load_template};
use common::email::EmailTemplate;
use std::str::FromStr;

/// A payload value as text; missing and null are empty.
fn field(payload: &serde_json::Value, key: &str) -> String {
    match payload.get(key) {
        Some(serde_json::Value::String(s)) => s.clone(),
        Some(serde_json::Value::Number(n)) => n.to_string(),
        Some(serde_json::Value::Bool(b)) => b.to_string(),
        _ => String::default(),
    }
}

/// The button to the calendar page; anything but an absolute http(s) link is dropped.
fn cta(url: &str) -> String {
    if !(url.starts_with("https://") || url.starts_with("http://")) {
        return String::default();
    }
    format!(
        "    <div style=\"text-align: center; margin: 28px 0;\">\n      <a href=\"{}\" style=\"background-color: #0284c7; color: #ffffff; padding: 12px 24px; text-decoration: none; border-radius: 6px; font-weight: bold; display: inline-block;\">Open the calendar</a>\n    </div>\n",
        escape_html(url)
    )
}

/// Renders a calendar email, or `None` if `template_id` isn't one.
pub fn render(template_id: &str, payload: &serde_json::Value) -> Option<String> {
    let text = |key: &str| escape_html(&field(payload, key));
    match EmailTemplate::from_str(template_id).ok()? {
        EmailTemplate::CalendarConflictHost => {
            let reopened = field(payload, "reopened") == "true";
            let heading = match reopened {
                true => "Calendar conflict reopened",
                false => "Calendar conflict",
            };
            let kind_text = match field(payload, "kind").as_str() {
                "turnover" => "leaves no turnover time around a stay",
                _ => "overlaps a stay",
            };
            Some(
                load_template("calendar_conflict_host.html")
                    .replace("{{heading}}", heading)
                    .replace("{{kind_text}}", kind_text)
                    .replace("{{listing_name}}", &text("listing_name"))
                    .replace("{{feed_name}}", &text("feed_name"))
                    .replace("{{date_from}}", &text("date_from"))
                    .replace("{{date_to}}", &text("date_to"))
                    .replace("{{confirmation_code}}", &text("confirmation_code"))
                    .replace("{{cta}}", &cta(&field(payload, "calendar_url"))),
            )
        }
        EmailTemplate::CalendarFeedFailingHost => Some(
            load_template("calendar_feed_failing_host.html")
                .replace("{{listing_name}}", &text("listing_name"))
                .replace("{{feed_name}}", &text("feed_name"))
                .replace("{{error_code}}", &text("error_code"))
                .replace("{{failures}}", &text("failures"))
                .replace("{{cta}}", &cta(&field(payload, "calendar_url"))),
        ),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::render;

    fn conflict() -> serde_json::Value {
        serde_json::to_value(common::email::CalendarConflictHostPayload {
            listing_name: "Villa Azul".into(),
            feed_name: "Airbnb".into(),
            date_from: "2030-01-12".into(),
            date_to: "2030-01-14".into(),
            confirmation_code: "ABC123".into(),
            kind: "overlap".into(),
            reopened: "false".into(),
            calendar_url: "https://admin.test/admin/listings/1/calendar".into(),
        })
        .expect("payload")
    }

    fn failing() -> serde_json::Value {
        serde_json::to_value(common::email::CalendarFeedFailingHostPayload {
            listing_name: "Villa Azul".into(),
            feed_name: "VRBO".into(),
            error_code: "calendar server returned HTTP 503".into(),
            failures: 4,
            calendar_url: "https://admin.test/admin/listings/1/calendar".into(),
        })
        .expect("payload")
    }

    #[test]
    fn calendar_conflict_renders_every_key() {
        let html = render("calendar_conflict_host", &conflict()).expect("rendered");
        for want in [
            "Villa Azul",
            "Airbnb",
            "2030-01-12",
            "2030-01-14",
            "ABC123",
            "overlaps a stay",
            "Calendar conflict",
            "https://admin.test/admin/listings/1/calendar",
        ] {
            assert!(html.contains(want), "missing {want}");
        }
        assert!(!html.contains("{{"), "unfilled placeholder");
        let mut reopened = conflict();
        reopened["reopened"] = "true".into();
        reopened["kind"] = "turnover".into();
        let html = render("calendar_conflict_host", &reopened).expect("rendered");
        assert!(html.contains("Calendar conflict reopened"));
        assert!(html.contains("no turnover time"));
    }

    #[test]
    fn calendar_feed_failing_renders_every_key() {
        let html = render("calendar_feed_failing_host", &failing()).expect("rendered");
        for want in [
            "Villa Azul",
            "VRBO",
            "HTTP 503",
            "4 times",
            "/admin/listings/1/calendar",
        ] {
            assert!(html.contains(want), "missing {want}");
        }
        assert!(!html.contains("{{"), "unfilled placeholder");
    }

    #[test]
    fn calendar_values_are_escaped() {
        for (template, mut payload) in [
            ("calendar_conflict_host", conflict()),
            ("calendar_feed_failing_host", failing()),
        ] {
            payload["listing_name"] = "<script>alert(1)</script>".into();
            payload["feed_name"] = "<img src=x onerror=alert(2)>".into();
            payload["calendar_url"] = "javascript:alert(3)".into();
            let html = render(template, &payload).expect("rendered");
            assert!(
                !html.contains("<script>") && !html.contains("<img"),
                "{template}"
            );
            assert!(html.contains("&lt;script&gt;"));
            assert!(!html.contains("javascript:"));
        }
    }

    #[test]
    fn calendar_payloads_have_no_url_keys_but_the_calendar_page() {
        for payload in [conflict(), failing()] {
            let keys: Vec<&String> = payload.as_object().expect("object").keys().collect();
            assert!(
                keys.iter()
                    .all(|k| !k.contains("url") || *k == "calendar_url"),
                "{keys:?}"
            );
        }
    }

    #[test]
    fn other_templates_are_not_calendar_emails() {
        assert!(render("booking_confirmation", &conflict()).is_none());
        assert!(render("nope", &conflict()).is_none());
    }
}
