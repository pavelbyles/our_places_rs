//! Enquiry and request-to-book emails (#105).
//!
//! Every event shares one layout (`request_notification.html`); only the
//! heading, intro, extra blocks and call to action differ, and they live here.
//! Every payload value is user or business data, so all of it is escaped.

use crate::processor::{escape_html, load_template};
use common::email::EmailTemplate;
use std::str::FromStr;

/// The copy for one event. `{listing}`, `{guest}` and `{expires}` in `intro`
/// are replaced with the escaped payload values.
struct Copy {
    heading: &'static str,
    intro: &'static str,
    cta: &'static str,
}

fn copy(template: EmailTemplate) -> Option<Copy> {
    use EmailTemplate as T;
    let (heading, intro, cta) = match template {
        T::EnquirySentGuest => (
            "Your question was sent",
            "We've sent your question about {listing} to the host. We'll email you when they reply.",
            "View conversation",
        ),
        T::EnquiryReceivedHost => (
            "New enquiry",
            "{guest} has a question about {listing}. Enquiries don't hold any dates.",
            "Reply",
        ),
        T::RequestSubmittedGuest => (
            "Request sent",
            "Your request to book {listing} has been sent. The host has until {expires} to respond, and your card hasn't been charged.",
            "View request",
        ),
        T::RequestReceivedHost => (
            "New request to book",
            "{guest} would like to book {listing}. Please accept, decline or send a quote by {expires}, or the request expires and the dates are released.",
            "Respond to request",
        ),
        T::RequestAcceptedGuest => (
            "Your request was accepted",
            "The host accepted your request for {listing}. Proceed to checkout by {expires} to secure your stay.",
            "Proceed to checkout",
        ),
        T::RequestAcceptedHost => (
            "You accepted a request",
            "You accepted {guest}'s request for {listing}. The dates are held until {expires} for them to pay.",
            "View request",
        ),
        T::RequestDeclinedGuest => (
            "Your request was declined",
            "The host declined your request for {listing}.",
            "Find another stay",
        ),
        T::RequestDeclinedHost => (
            "Request declined",
            "{guest}'s request for {listing} was declined, and the dates are free again.",
            "View request",
        ),
        T::QuoteSentGuest => (
            "You have a custom quote",
            "The host sent you a custom quote for {listing}. Accept it by {expires} to go to checkout.",
            "Review quote",
        ),
        T::QuoteSentHost => (
            "Quote sent",
            "Your quote for {guest}'s stay at {listing} was sent. It's valid until {expires}.",
            "View request",
        ),
        T::RequestConvertedGuest => (
            "Complete your booking",
            "Your stay at {listing} is on hold for you until {expires}. Complete payment to confirm it.",
            "Pay now",
        ),
        T::RequestConvertedHost => (
            "Your guest is checking out",
            "{guest} accepted the offer for {listing} and is completing payment.",
            "View booking",
        ),
        T::RequestExpiredGuest => (
            "Your request expired",
            "Your request for {listing} has expired, and the dates were released.",
            "Find another stay",
        ),
        T::RequestExpiredHost => (
            "Request expired",
            "{guest}'s request for {listing} has expired, and the dates were released.",
            "Open inbox",
        ),
        T::RequestWithdrawnGuest => (
            "Request withdrawn",
            "Your request for {listing} was withdrawn, and the dates were released.",
            "View request",
        ),
        T::RequestWithdrawnHost => (
            "Request withdrawn",
            "{guest}'s request for {listing} was withdrawn. The dates are free again.",
            "Open inbox",
        ),
        T::RequestHostReminder => (
            "A request is about to expire",
            "{guest}'s request for {listing} expires at {expires}. Please respond before then, or the dates are released.",
            "Respond to request",
        ),
        _ => return None,
    };
    Some(Copy {
        heading,
        intro,
        cta,
    })
}

fn field(payload: &serde_json::Value, key: &str) -> String {
    payload
        .get(key)
        .map(|v| match v {
            serde_json::Value::String(s) => s.clone(),
            serde_json::Value::Null => String::new(),
            other => other.to_string(),
        })
        .unwrap_or_default()
}

fn detail_row(label: &str, value: &str) -> String {
    if value.is_empty() {
        return String::new();
    }
    format!(
        "      <p style=\"margin: 0 0 8px 0;\"><strong>{label}:</strong> {}</p>\n",
        escape_html(value)
    )
}

fn block(label: &str, text: &str, accent: &str) -> String {
    if text.trim().is_empty() {
        return String::new();
    }
    // Line breaks in user text become <br>, after escaping.
    let body = escape_html(text).replace('\n', "<br>");
    format!(
        "    <div style=\"background-color: #f1f5f9; border-left: 4px solid {accent}; padding: 12px 16px; margin: 20px 0;\">\n      <strong>{label}:</strong>\n      <p style=\"margin: 4px 0 0 0;\">{body}</p>\n    </div>\n"
    )
}

/// Only absolute http(s) links become buttons; anything else is dropped.
fn cta(url: &str, label: &str) -> String {
    if !(url.starts_with("https://") || url.starts_with("http://")) {
        return String::new();
    }
    format!(
        "    <div style=\"text-align: center; margin: 28px 0;\">\n      <a href=\"{}\" style=\"background-color: #0284c7; color: #ffffff; padding: 12px 24px; text-decoration: none; border-radius: 6px; font-weight: bold; display: inline-block;\">{label}</a>\n    </div>\n",
        escape_html(url)
    )
}

/// Renders an enquiry or request email, or `None` if `template_id` isn't one.
///
/// Payload keys (all optional strings): `recipient_name`, `listing_name`,
/// `guest_name`, `date_from`, `date_to`, `guests`, `total`, `expires_at`,
/// `message_text`, `terms`, `decline_reason`, `decline_note`,
/// `promo_dropped` (`"true"`), `action_url`.
pub fn render(template_id: &str, payload: &serde_json::Value) -> Option<String> {
    let template = EmailTemplate::from_str(template_id).ok()?;
    let copy = copy(template)?;

    let listing = escape_html(&field(payload, "listing_name"));
    let guest = escape_html(&field(payload, "guest_name"));
    let expires = escape_html(&field(payload, "expires_at"));
    let intro = copy
        .intro
        .replace("{listing}", &format!("<strong>{listing}</strong>"))
        .replace("{guest}", &guest)
        .replace("{expires}", &expires);

    let dates = match (field(payload, "date_from"), field(payload, "date_to")) {
        (from, to) if !from.is_empty() && !to.is_empty() => format!("{from} to {to}"),
        _ => String::new(),
    };
    let details = [
        detail_row("Property", &field(payload, "listing_name")),
        detail_row("Dates", &dates),
        detail_row("Guests", &field(payload, "guests")),
        detail_row("Total", &field(payload, "total")),
    ]
    .concat();

    let reason = [
        field(payload, "decline_reason"),
        field(payload, "decline_note"),
    ]
    .into_iter()
    .filter(|s| !s.trim().is_empty())
    .collect::<Vec<_>>()
    .join("\n");
    let promo_notice = if field(payload, "promo_dropped") == "true" {
        "Your promo code doesn't apply to a custom quote. The quoted price is final."
    } else {
        ""
    };
    let blocks = [
        block("Message", &field(payload, "message_text"), "#0284c7"),
        block("Special terms", &field(payload, "terms"), "#0284c7"),
        block("Reason", &reason, "#ef4444"),
        block("Please note", promo_notice, "#f59e0b"),
    ]
    .concat();

    Some(
        load_template("request_notification.html")
            .replace("{{heading}}", copy.heading)
            .replace(
                "{{recipient_name}}",
                &escape_html(&field(payload, "recipient_name")),
            )
            .replace("{{intro}}", &intro)
            .replace("{{details}}", &details)
            .replace("{{blocks}}", &blocks)
            .replace("{{cta}}", &cta(&field(payload, "action_url"), copy.cta)),
    )
}

#[cfg(test)]
mod request_template_tests {
    use super::*;

    fn payload() -> serde_json::Value {
        serde_json::json!({
            "recipient_name": "Alex",
            "listing_name": "Sea Grape Villa",
            "guest_name": "Sam",
            "date_from": "2030-08-01",
            "date_to": "2030-08-08",
            "guests": "6",
            "total": "USD 1,172.00",
            "expires_at": "2030-07-02 12:00 UTC",
            "message_text": "Is the pool heated?\nThanks!",
            "terms": "Wedding allowed until 10pm.",
            "decline_reason": "Events aren't permitted",
            "decline_note": "Sorry!",
            "promo_dropped": "true",
            "action_url": "https://ourplaces.example/requests/1",
        })
    }

    #[test]
    fn every_request_email_renders_without_placeholders() {
        for template in EmailTemplate::REQUEST_EMAILS {
            let body = match render(template.as_str(), &payload()) {
                Some(body) => body,
                None => panic!("{template:?} has no copy"),
            };
            assert!(!body.contains("{{"), "{template:?}: {body}");
            assert!(!body.contains("{listing}") && !body.contains("{guest}"));
            assert!(!body.contains("{expires}"), "{template:?}");
            assert!(body.contains("Sea Grape Villa"), "{template:?}");
            assert!(body.contains("Hello <strong>Alex</strong>"), "{template:?}");
            assert!(
                body.contains("href=\"https://ourplaces.example/requests/1\""),
                "{template:?}"
            );
        }
    }

    #[test]
    fn other_templates_are_not_request_emails() {
        assert!(render("booking_confirmation", &payload()).is_none());
        assert!(render("guest_host_message_notification", &payload()).is_none());
        assert!(render("no_such_template", &payload()).is_none());
    }

    #[test]
    fn user_text_is_escaped_everywhere() {
        let script = "<script>alert('x')</script>";
        let mut p = payload();
        for key in [
            "recipient_name",
            "listing_name",
            "guest_name",
            "message_text",
            "terms",
            "decline_note",
            "expires_at",
            "total",
        ] {
            p[key] = serde_json::json!(script);
        }
        for template in EmailTemplate::REQUEST_EMAILS {
            let body = render(template.as_str(), &p).unwrap_or_default();
            assert!(!body.contains("<script>"), "{template:?}");
            assert!(body.contains("&lt;script&gt;"), "{template:?}");
        }
    }

    #[test]
    fn non_http_links_are_dropped() {
        let mut p = payload();
        p["action_url"] = serde_json::json!("javascript:alert(1)");
        let body = render("request_received_host", &p).unwrap_or_default();
        assert!(!body.contains("javascript:"));
        assert!(!body.contains("<a href"));
    }

    #[test]
    fn quote_email_shows_terms_and_promo_notice_and_decline_shows_the_reason() {
        let quote = render("quote_sent_guest", &payload()).unwrap_or_default();
        assert!(quote.contains("Wedding allowed until 10pm."));
        assert!(quote.contains("doesn&#x27;t apply to a custom quote"));
        assert!(quote.contains("USD 1,172.00"));

        let mut p = payload();
        p["promo_dropped"] = serde_json::json!("false");
        p["terms"] = serde_json::json!("");
        p["message_text"] = serde_json::json!("");
        let declined = render("request_declined_guest", &p).unwrap_or_default();
        assert!(declined.contains("Events aren&#x27;t permitted<br>Sorry!"));
        assert!(!declined.contains("Special terms"));
        assert!(!declined.contains("custom quote"));
    }
}
