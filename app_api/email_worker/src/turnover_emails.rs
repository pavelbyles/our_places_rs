//! Transactional emails for staff onboarding, password recovery, and turnover comment threads (#66).

use crate::processor::{escape_html, load_template};
use common::email::EmailTemplate;
use std::str::FromStr;

/// Extracts a scalar payload field as text; missing or non-scalar values become empty strings.
fn field(payload: &serde_json::Value, key: &str) -> String {
    match payload.get(key) {
        Some(serde_json::Value::String(s)) => s.clone(),
        Some(serde_json::Value::Number(n)) => n.to_string(),
        Some(serde_json::Value::Bool(b)) => b.to_string(),
        _ => String::default(),
    }
}

/// Renders an escaped HTML list of listing names from `listing_names` payload array.
fn listing_names_html(payload: &serde_json::Value) -> String {
    let items: Vec<String> = payload
        .get("listing_names")
        .and_then(|v| v.as_array())
        .into_iter()
        .flatten()
        .filter_map(|item| item.as_str())
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(|name| format!("<li>{}</li>", escape_html(name)))
        .collect();

    if items.is_empty() {
        "<li>Assigned properties (view on the dispatch board)</li>".to_string()
    } else {
        items.join("\n        ")
    }
}

/// Validates CTA links via `url::Url::parse`, permitting only `http://` or `https://` links.
fn validated_cta(url: &str, label: &str) -> String {
    let Ok(parsed) = url::Url::parse(url.trim()) else {
        return String::default();
    };
    if !matches!(parsed.scheme(), "http" | "https") {
        return String::default();
    }
    let escaped_url = escape_html(parsed.as_str());
    let escaped_label = escape_html(label);
    format!(
        "    <div style=\"text-align: center; margin: 28px 0;\">\n      <a href=\"{escaped_url}\" style=\"background-color: #0284c7; color: #ffffff; padding: 12px 24px; text-decoration: none; border-radius: 6px; font-weight: bold; display: inline-block;\">{escaped_label}</a>\n    </div>\n"
    )
}

/// Renders a staff/turnover email body, returning `None` when `template_id` is handled elsewhere.
pub fn render(template_id: &str, payload: &serde_json::Value) -> Option<String> {
    let text = |key: &str| escape_html(&field(payload, key));
    let template = EmailTemplate::from_str(template_id).ok()?;

    match template {
        EmailTemplate::StaffInvite => Some(
            load_template("staff_invite.html")
                .replace("{{first_name}}", &text("first_name"))
                .replace("{{host_name}}", &text("host_name"))
                .replace("{{listing_items}}", &listing_names_html(payload))
                .replace("{{expires_hours}}", &text("expires_hours"))
                .replace(
                    "{{cta}}",
                    &validated_cta(&field(payload, "setup_url"), "Set Password & Open Dispatch"),
                ),
        ),
        EmailTemplate::StaffAcceptInvite => Some(
            load_template("staff_accept_invite.html")
                .replace("{{first_name}}", &text("first_name"))
                .replace("{{host_name}}", &text("host_name"))
                .replace("{{listing_items}}", &listing_names_html(payload))
                .replace("{{expires_hours}}", &text("expires_hours"))
                .replace(
                    "{{cta}}",
                    &validated_cta(&field(payload, "accept_url"), "Accept Invitation"),
                ),
        ),
        EmailTemplate::StaffListingAssigned => Some(
            load_template("staff_listing_assigned.html")
                .replace("{{first_name}}", &text("first_name"))
                .replace("{{host_name}}", &text("host_name"))
                .replace("{{listing_items}}", &listing_names_html(payload))
                .replace(
                    "{{cta}}",
                    &validated_cta(&field(payload, "dispatch_url"), "Open Turnover Dispatch"),
                ),
        ),
        EmailTemplate::PasswordResetLink => Some(
            load_template("password_reset_link.html")
                .replace("{{first_name}}", &text("first_name"))
                .replace("{{expires_minutes}}", &text("expires_minutes"))
                .replace(
                    "{{cta}}",
                    &validated_cta(&field(payload, "reset_url"), "Reset Password"),
                ),
        ),
        EmailTemplate::TurnoverCommentNotification => Some(
            load_template("turnover_comment_notification.html")
                .replace("{{listing_name}}", &text("listing_name"))
                .replace("{{checkout_date}}", &text("checkout_date"))
                .replace("{{author_name}}", &text("author_name"))
                .replace("{{author_role}}", &text("author_role"))
                .replace(
                    "{{comment_body}}",
                    &text("comment_body").replace('\n', "<br>"),
                )
                .replace(
                    "{{cta}}",
                    &validated_cta(&field(payload, "turnover_url"), "Reply on Turnover Board"),
                ),
        ),
        EmailTemplate::TurnoverCreated => {
            let raw_title = field(payload, "title");
            let title_row = if raw_title.trim().is_empty() {
                String::default()
            } else {
                format!(
                    "<p style=\"margin: 0 0 12px 0;\"><strong>Task:</strong> {}</p>",
                    escape_html(raw_title.trim())
                )
            };
            Some(
                load_template("turnover_created.html")
                    .replace("{{listing_name}}", &text("listing_name"))
                    .replace("{{checkout_date}}", &text("checkout_date"))
                    .replace("{{source_label}}", &text("source_label"))
                    .replace("{{title_row}}", &title_row)
                    .replace(
                        "{{cta}}",
                        &validated_cta(&field(payload, "turnover_url"), "View Turnover Details"),
                    ),
            )
        }
        _ => None,
    }
}
