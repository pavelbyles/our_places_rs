//! booking_api-specific settings read from the environment.
//! Service settings (server, database, application) come from `api_core::settings`.

use serde::Deserialize;

/// Tax compliance settings identifying the Merchant of Record.
#[derive(Debug, Clone, Deserialize)]
pub struct TaxSettings {
    /// Supplier's tax registration number (TRN) with Tax Administration Jamaica.
    pub taj_supplier_trn: String,
    /// Legal name of the supplier.
    pub supplier_name: String,
    /// Registered address of the supplier.
    pub supplier_address: String,
}

impl TaxSettings {
    /// Loads and validates statutory tax settings from environment variables.
    /// Fails fast if any mandatory supplier field is missing or empty.
    pub fn from_env() -> Result<Self, String> {
        Self::from_lookup(|k| std::env::var(k).ok())
    }

    /// Loads and validates statutory tax settings from a key-value lookup function.
    /// Strictly enforces that `TAJ_SUPPLIER_TRN`, `TAJ_SUPPLIER_NAME`, and
    /// `TAJ_SUPPLIER_ADDRESS` are present and non-empty without default fallbacks.
    pub fn from_lookup<F>(lookup: F) -> Result<Self, String>
    where
        F: Fn(&str) -> Option<String>,
    {
        let trn = lookup("TAJ_SUPPLIER_TRN").ok_or_else(|| {
            "TAJ_SUPPLIER_TRN is required for statutory tax invoicing".to_string()
        })?;
        if trn.trim().is_empty() {
            return Err("TAJ_SUPPLIER_TRN cannot be empty".to_string());
        }

        let supplier_name = lookup("TAJ_SUPPLIER_NAME").ok_or_else(|| {
            "TAJ_SUPPLIER_NAME is required for statutory tax invoicing".to_string()
        })?;
        if supplier_name.trim().is_empty() {
            return Err("TAJ_SUPPLIER_NAME cannot be empty".to_string());
        }

        let supplier_address = lookup("TAJ_SUPPLIER_ADDRESS").ok_or_else(|| {
            "TAJ_SUPPLIER_ADDRESS is required for statutory tax invoicing".to_string()
        })?;
        if supplier_address.trim().is_empty() {
            return Err("TAJ_SUPPLIER_ADDRESS cannot be empty".to_string());
        }

        Ok(Self {
            taj_supplier_trn: trn,
            supplier_name,
            supplier_address,
        })
    }

    /// Converts into `db_core::models::SupplierInfo`.
    pub fn to_supplier_info(&self) -> db_core::models::SupplierInfo {
        db_core::models::SupplierInfo {
            supplier_name: self.supplier_name.clone(),
            supplier_address: self.supplier_address.clone(),
            supplier_trn: self.taj_supplier_trn.clone(),
        }
    }
}

/// Typed settings for internal cron job authentication.
#[derive(Debug, Clone, Deserialize)]
pub struct CronSettings {
    /// Shared secret that cron requests must send.
    pub cron_secret: String,
}

impl CronSettings {
    /// Loads and validates cron settings from environment variables.
    /// Strictly requires `CRON_SECRET` with no default fallbacks.
    pub fn from_env() -> Result<Self, String> {
        Self::from_lookup(|k| std::env::var(k).ok())
    }

    /// Loads and validates cron settings from a key-value lookup function.
    pub fn from_lookup<F>(lookup: F) -> Result<Self, String>
    where
        F: Fn(&str) -> Option<String>,
    {
        let secret = lookup("CRON_SECRET")
            .ok_or_else(|| "CRON_SECRET is required for scheduled cron endpoints".to_string())?;
        if secret.trim().is_empty() {
            return Err("CRON_SECRET cannot be empty".to_string());
        }
        Ok(Self {
            cron_secret: secret,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tax_settings_validation() {
        let missing_or_empty: &[(&str, &[(&str, &str)], &str)] = &[
            ("missing trn", &[], "TAJ_SUPPLIER_TRN is required"),
            (
                "empty trn",
                &[("TAJ_SUPPLIER_TRN", "   ")],
                "TAJ_SUPPLIER_TRN cannot be empty",
            ),
            (
                "missing name",
                &[("TAJ_SUPPLIER_TRN", "123-456-789")],
                "TAJ_SUPPLIER_NAME is required",
            ),
            (
                "empty name",
                &[
                    ("TAJ_SUPPLIER_TRN", "123-456-789"),
                    ("TAJ_SUPPLIER_NAME", "   "),
                ],
                "TAJ_SUPPLIER_NAME cannot be empty",
            ),
            (
                "missing address",
                &[
                    ("TAJ_SUPPLIER_TRN", "123-456-789"),
                    ("TAJ_SUPPLIER_NAME", "Our Places Jamaica Limited"),
                ],
                "TAJ_SUPPLIER_ADDRESS is required",
            ),
            (
                "empty address",
                &[
                    ("TAJ_SUPPLIER_TRN", "123-456-789"),
                    ("TAJ_SUPPLIER_NAME", "Our Places Jamaica Limited"),
                    ("TAJ_SUPPLIER_ADDRESS", "   "),
                ],
                "TAJ_SUPPLIER_ADDRESS cannot be empty",
            ),
        ];

        for (label, vars, expected_err) in missing_or_empty {
            let err = TaxSettings::from_lookup(|k| {
                vars.iter()
                    .find(|(vk, _)| *vk == k)
                    .map(|(_, v)| (*v).to_string())
            })
            .unwrap_err();
            assert!(err.contains(expected_err), "{label}: got {err}");
        }

        let settings = TaxSettings::from_lookup(|k| match k {
            "TAJ_SUPPLIER_TRN" => Some("123-456-789".to_string()),
            "TAJ_SUPPLIER_NAME" => Some("Our Places Jamaica Limited".to_string()),
            "TAJ_SUPPLIER_ADDRESS" => Some("Main Street, Ocho Rios, Jamaica".to_string()),
            _ => None,
        })
        .expect("valid tax settings");
        assert_eq!(settings.taj_supplier_trn, "123-456-789");
        assert_eq!(settings.supplier_name, "Our Places Jamaica Limited");
        assert_eq!(settings.supplier_address, "Main Street, Ocho Rios, Jamaica");
        assert_eq!(settings.to_supplier_info().supplier_trn, "123-456-789");
    }

    #[test]
    fn test_cron_settings_validation() {
        for (label, val, expected_err) in [
            ("missing", None, "CRON_SECRET is required"),
            ("empty", Some("   "), "CRON_SECRET cannot be empty"),
        ] {
            let err = CronSettings::from_lookup(|k| {
                if k == "CRON_SECRET" {
                    val.map(str::to_string)
                } else {
                    None
                }
            })
            .unwrap_err();
            assert!(err.contains(expected_err), "{label}: got {err}");
        }

        let settings = CronSettings::from_lookup(|k| {
            (k == "CRON_SECRET").then(|| "super-secret-cron-token".to_string())
        })
        .expect("valid cron settings");
        assert_eq!(settings.cron_secret, "super-secret-cron-token");
    }
}
