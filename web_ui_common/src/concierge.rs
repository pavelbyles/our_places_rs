//! The guest footer's concierge contact (spec-122 §2.9), read once from the
//! environment. Every value is optional; the footer leaves out what is not
//! set, and the whole column when nothing is.

use std::env;
use std::sync::OnceLock;

/// WhatsApp number, digits with country code (e.g. `18765550142`).
static WHATSAPP_ENV: &str = "GUEST_CONCIERGE_WHATSAPP";
/// Phone number as shown (e.g. `+1 876 555 0142`).
static PHONE_ENV: &str = "GUEST_CONCIERGE_PHONE";
/// Email address.
static EMAIL_ENV: &str = "GUEST_CONCIERGE_EMAIL";
/// Instagram handle, with or without `@`.
static INSTAGRAM_ENV: &str = "GUEST_CONCIERGE_INSTAGRAM";

/// How guests reach the concierge.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Concierge {
    /// WhatsApp number, digits only.
    pub whatsapp: Option<String>,
    /// Phone number as shown.
    pub phone: Option<String>,
    /// Email address.
    pub email: Option<String>,
    /// Instagram handle without `@`.
    pub instagram: Option<String>,
}

impl Concierge {
    /// Reads the values through `lookup` (the environment in production).
    pub fn from_lookup(lookup: impl Fn(&str) -> Option<String>) -> Self {
        let get = |key: &str| {
            lookup(key)
                .map(|v| v.trim().to_string())
                .filter(|v| !v.is_empty())
        };
        Self {
            whatsapp: get(WHATSAPP_ENV)
                .map(|v| v.chars().filter(char::is_ascii_digit).collect::<String>())
                .filter(|d| d.len() >= 7),
            phone: get(PHONE_ENV),
            email: get(EMAIL_ENV).filter(|e| e.contains('@')),
            instagram: get(INSTAGRAM_ENV).map(|h| h.trim_start_matches('@').to_string()),
        }
    }

    /// Whether any contact is set.
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }

    /// `(label, href)` for each contact that is set, in footer order.
    pub fn links(&self) -> Vec<(String, String)> {
        [
            self.whatsapp
                .as_ref()
                .map(|n| ("WhatsApp".to_string(), format!("https://wa.me/{n}"))),
            self.phone.as_ref().map(|p| {
                let dial: String = p
                    .chars()
                    .filter(|c| c.is_ascii_digit() || *c == '+')
                    .collect();
                (p.clone(), format!("tel:{dial}"))
            }),
            self.email
                .as_ref()
                .map(|e| (e.clone(), format!("mailto:{e}"))),
            self.instagram
                .as_ref()
                .map(|h| (format!("@{h}"), format!("https://www.instagram.com/{h}/"))),
        ]
        .into_iter()
        .flatten()
        .collect()
    }
}

/// The concierge contact from the environment, read once.
pub fn concierge() -> &'static Concierge {
    static CONCIERGE: OnceLock<Concierge> = OnceLock::new();
    CONCIERGE.get_or_init(|| Concierge::from_lookup(|key| env::var(key).ok()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unset_or_blank_values_are_left_out() {
        let c = Concierge::from_lookup(|_| Some("  ".into()));
        assert!(c.is_empty());
        assert!(c.links().is_empty());
    }

    #[test]
    fn values_become_links() {
        let c = Concierge::from_lookup(|key| match key {
            "GUEST_CONCIERGE_WHATSAPP" => Some("+1 (876) 555-0142".into()),
            "GUEST_CONCIERGE_PHONE" => Some("+1 876 555 0142".into()),
            "GUEST_CONCIERGE_EMAIL" => Some("concierge@example.com".into()),
            "GUEST_CONCIERGE_INSTAGRAM" => Some("@ourplacesvillas".into()),
            _ => None,
        });
        assert_eq!(
            c.links(),
            [
                (
                    "WhatsApp".to_string(),
                    "https://wa.me/18765550142".to_string()
                ),
                (
                    "+1 876 555 0142".to_string(),
                    "tel:+18765550142".to_string()
                ),
                (
                    "concierge@example.com".to_string(),
                    "mailto:concierge@example.com".to_string()
                ),
                (
                    "@ourplacesvillas".to_string(),
                    "https://www.instagram.com/ourplacesvillas/".to_string()
                ),
            ]
        );
    }

    #[test]
    fn malformed_whatsapp_and_email_are_dropped() {
        let c = Concierge::from_lookup(|key| match key {
            "GUEST_CONCIERGE_WHATSAPP" => Some("call us".into()),
            "GUEST_CONCIERGE_EMAIL" => Some("not-an-email".into()),
            _ => None,
        });
        assert!(c.is_empty());
    }
}
