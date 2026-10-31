//! User administration (parity A29–A31): administrators only; hosts get
//! 403 on every route.

use serde::Deserialize;
use web_ui_common::components::domain::currency_selector::DISPLAY_CURRENCIES;

/// Archive, purge and restore.
pub mod actions;
/// Create and edit forms.
pub mod form;
/// The user directory.
pub mod list;

/// Field label classes shared by the user forms.
pub(crate) const LABEL: &str = "block text-xs font-medium text-muted-foreground";
/// Input classes shared by the user forms.
pub(crate) const INPUT: &str =
    "mt-1 block h-10 w-full rounded-lg border border-border bg-card px-3 text-sm text-foreground";

/// The create / edit form. Checkboxes arrive only when ticked.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct UserForm {
    /// First name.
    #[serde(default)]
    pub first_name: String,
    /// Last name.
    #[serde(default)]
    pub last_name: String,
    /// Email address.
    #[serde(default)]
    pub email: String,
    /// Password (required on create; blank keeps it on edit).
    #[serde(default)]
    pub password: String,
    /// Phone number (optional).
    #[serde(default)]
    pub phone_number: String,
    /// Display currency.
    #[serde(default)]
    pub default_currency: String,
    /// Admin role.
    pub role_admin: Option<String>,
    /// Host role.
    pub role_host: Option<String>,
    /// Booker (guest) role.
    pub role_booker: Option<String>,
    /// May manage listings (hosts and admins).
    pub can_manage_listings: Option<String>,
    /// May manage bookings (hosts and admins).
    pub can_manage_bookings: Option<String>,
    /// May configure rates (admins only).
    pub can_configure_rates: Option<String>,
    /// May manage users (admins only).
    pub can_manage_users: Option<String>,
    /// Account is active.
    pub is_active: Option<String>,
    /// Email is verified (edit only).
    pub is_verified: Option<String>,
    /// Public handle (edit only; new accounts get the database default).
    #[serde(default)]
    pub slug: String,
}

/// A form that passed [`UserForm::validate`].
#[derive(Debug, Clone, PartialEq)]
pub struct ValidUser {
    /// Trimmed first name.
    pub first_name: String,
    /// Trimmed last name.
    pub last_name: String,
    /// Trimmed email.
    pub email: String,
    /// New password, if one was entered.
    pub password: Option<String>,
    /// Phone, if entered.
    pub phone_number: Option<String>,
    /// A supported display currency.
    pub default_currency: String,
    /// Selected role names.
    pub roles: Vec<String>,
    /// Capability flags stored in the user's `attributes`.
    pub permissions: serde_json::Map<String, serde_json::Value>,
    /// Account is active.
    pub is_active: bool,
    /// Email is verified.
    pub is_verified: bool,
    /// Cleaned-up slug, if one was entered.
    pub slug: Option<String>,
}

/// Minimum password length for accounts created here.
const MIN_PASSWORD_LEN: usize = 8;

impl UserForm {
    /// A blank create form (booker, active, USD).
    pub fn blank() -> Self {
        Self {
            default_currency: "USD".to_string(),
            role_booker: Some("on".to_string()),
            is_active: Some("on".to_string()),
            ..Self::default()
        }
    }

    /// The edit form for an existing account.
    pub fn from_user(user: &common::models::UserResponse) -> Self {
        let has = |name: &str| user.roles.iter().any(|r| r.eq_ignore_ascii_case(name));
        let flag = |b: bool| b.then(|| "on".to_string());
        let attr = |key: &str| {
            flag(
                user.attributes
                    .get(key)
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false),
            )
        };
        Self {
            first_name: user.first_name.clone(),
            last_name: user.last_name.clone(),
            email: user.email.clone(),
            password: String::new(),
            phone_number: user.phone_number.clone().unwrap_or_default(),
            default_currency: user.default_currency.clone(),
            role_admin: flag(has("admin")),
            role_host: flag(has("host")),
            role_booker: flag(has("booker")),
            can_manage_listings: attr("can_manage_listings"),
            can_manage_bookings: attr("can_manage_bookings"),
            can_configure_rates: attr("can_configure_rates"),
            can_manage_users: attr("can_manage_users"),
            is_active: flag(user.is_active),
            is_verified: flag(user.is_verified),
            slug: user.slug.clone(),
        }
    }

    /// Checks the form. `creating` makes the password required. Applies the
    /// legacy capability rules: bookers hold no capabilities, and hosts may
    /// only manage listings and bookings.
    pub fn validate(&self, creating: bool) -> Result<ValidUser, Vec<String>> {
        let mut errors = Vec::new();
        let (first, last, email) = (
            self.first_name.trim(),
            self.last_name.trim(),
            self.email.trim(),
        );
        if first.is_empty() || last.is_empty() {
            errors.push("First and last name are required.".to_string());
        }
        if !email.contains('@') || email.len() < 3 {
            errors.push("Enter a valid email address.".to_string());
        }
        let password = Some(self.password.clone()).filter(|p| !p.is_empty());
        match &password {
            None if creating => errors.push("A password is required.".to_string()),
            Some(p) if p.chars().count() < MIN_PASSWORD_LEN => errors.push(format!(
                "Passwords need at least {MIN_PASSWORD_LEN} characters."
            )),
            _ => {}
        }
        if !DISPLAY_CURRENCIES.contains(&self.default_currency.as_str()) {
            errors.push("Choose a supported currency.".to_string());
        }
        let slug = Some(self.slug.trim())
            .filter(|s| !s.is_empty())
            .and_then(|s| {
                common::slug::user_slug(s)
                    .map_err(|message| errors.push(message))
                    .ok()
            });
        let (admin, host, booker) = (
            self.role_admin.is_some(),
            self.role_host.is_some(),
            self.role_booker.is_some(),
        );
        let roles: Vec<String> = [(admin, "admin"), (host, "host"), (booker, "booker")]
            .into_iter()
            .filter(|(on, _)| *on)
            .map(|(_, name)| name.to_string())
            .collect();
        if roles.is_empty() {
            errors.push("Choose at least one role.".to_string());
        }
        let caps = [
            (
                "can_manage_listings",
                self.can_manage_listings.is_some(),
                false,
            ),
            (
                "can_manage_bookings",
                self.can_manage_bookings.is_some(),
                false,
            ),
            (
                "can_configure_rates",
                self.can_configure_rates.is_some(),
                true,
            ),
            ("can_manage_users", self.can_manage_users.is_some(), true),
        ];
        let any_cap = caps.iter().any(|(_, on, _)| *on);
        if !admin && !host && any_cap {
            errors.push("Booker accounts cannot hold host or admin capabilities.".to_string());
        } else if !admin && caps.iter().any(|(_, on, admin_only)| *on && *admin_only) {
            errors.push("Hosts may only manage their listings and bookings.".to_string());
        }
        if !errors.is_empty() {
            return Err(errors);
        }
        let permissions = caps
            .iter()
            .filter(|(_, _, admin_only)| admin || (host && !admin_only))
            .map(|(key, on, _)| (key.to_string(), serde_json::Value::Bool(*on)))
            .collect();
        Ok(ValidUser {
            first_name: first.to_string(),
            last_name: last.to_string(),
            email: email.to_string(),
            password,
            phone_number: Some(self.phone_number.trim().to_string()).filter(|p| !p.is_empty()),
            default_currency: self.default_currency.clone(),
            roles,
            permissions,
            is_active: self.is_active.is_some(),
            is_verified: self.is_verified.is_some(),
            slug,
        })
    }
}

/// Whether a stored account is an administrator (those cannot be archived
/// or purged).
pub(crate) fn is_admin_account(user: &common::models::UserResponse) -> bool {
    user.roles
        .iter()
        .any(|r| r.eq_ignore_ascii_case("admin") || r.eq_ignore_ascii_case("superadmin"))
}

#[cfg(test)]
mod tests {
    use super::UserForm;

    fn on() -> Option<String> {
        Some("on".to_string())
    }

    fn valid() -> UserForm {
        UserForm {
            first_name: " Avery ".into(),
            last_name: "Created".into(),
            email: "avery@example.com".into(),
            password: "longenough".into(),
            ..UserForm::blank()
        }
    }

    #[test]
    fn accepts_a_booker_and_trims_names() {
        let user = valid().validate(true).map_err(|e| e.join(" "));
        assert_eq!(user.as_ref().map(|u| u.first_name.as_str()), Ok("Avery"));
        assert_eq!(
            user.as_ref().map(|u| u.roles.clone()),
            Ok(vec!["booker".to_string()])
        );
        assert_eq!(user.map(|u| u.permissions.is_empty()), Ok(true));
    }

    #[test]
    fn requires_a_password_only_when_creating() {
        let form = UserForm {
            password: String::new(),
            ..valid()
        };
        assert!(form.validate(true).is_err());
        assert_eq!(form.validate(false).map(|u| u.password), Ok(None));
    }

    #[test]
    fn rejects_short_passwords_and_missing_roles() {
        let form = UserForm {
            password: "short".into(),
            role_booker: None,
            ..valid()
        };
        assert_eq!(form.validate(true).map_err(|e| e.len()), Err(2));
    }

    #[test]
    fn bookers_cannot_hold_capabilities() {
        let form = UserForm {
            can_manage_listings: on(),
            ..valid()
        };
        assert!(form.validate(true).is_err());
    }

    #[test]
    fn hosts_cannot_hold_admin_capabilities() {
        let host = UserForm {
            role_host: on(),
            can_manage_bookings: on(),
            ..valid()
        };
        let perms = host
            .validate(true)
            .map(|u| u.permissions)
            .unwrap_or_default();
        assert_eq!(
            perms.get("can_manage_bookings"),
            Some(&serde_json::Value::Bool(true))
        );
        assert!(!perms.contains_key("can_manage_users"));
        let overreach = UserForm {
            role_host: on(),
            can_manage_users: on(),
            ..valid()
        };
        assert!(overreach.validate(true).is_err());
    }

    #[test]
    fn admins_may_hold_every_capability() {
        let admin = UserForm {
            role_admin: on(),
            can_manage_users: on(),
            can_configure_rates: on(),
            ..valid()
        };
        assert_eq!(admin.validate(true).map(|u| u.permissions.len()), Ok(4));
    }
}
