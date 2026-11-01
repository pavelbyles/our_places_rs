//! booking_api-specific settings read from the environment.
//! Service settings (server, database, application) come from `api_core::settings`.

use serde::Deserialize;

/// Tax compliance settings identifying the Merchant of Record.
#[derive(Debug, Clone, Deserialize)]
pub struct TaxSettings {
    pub taj_supplier_trn: String,
    pub supplier_name: String,
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
    fn test_tax_settings_missing_trn_fails() {
        let err = TaxSettings::from_lookup(|_| None).unwrap_err();
        assert!(err.contains("TAJ_SUPPLIER_TRN is required"));
    }

    #[test]
    fn test_tax_settings_empty_trn_fails() {
        let err = TaxSettings::from_lookup(|k| {
            if k == "TAJ_SUPPLIER_TRN" {
                Some("   ".to_string())
            } else {
                None
            }
        })
        .unwrap_err();
        assert!(err.contains("TAJ_SUPPLIER_TRN cannot be empty"));
    }

    #[test]
    fn test_tax_settings_missing_name_fails() {
        let err = TaxSettings::from_lookup(|k| {
            if k == "TAJ_SUPPLIER_TRN" {
                Some("123-456-789".to_string())
            } else {
                None
            }
        })
        .unwrap_err();
        assert!(err.contains("TAJ_SUPPLIER_NAME is required"));
    }

    #[test]
    fn test_tax_settings_empty_name_fails() {
        let err = TaxSettings::from_lookup(|k| match k {
            "TAJ_SUPPLIER_TRN" => Some("123-456-789".to_string()),
            "TAJ_SUPPLIER_NAME" => Some("   ".to_string()),
            _ => None,
        })
        .unwrap_err();
        assert!(err.contains("TAJ_SUPPLIER_NAME cannot be empty"));
    }

    #[test]
    fn test_tax_settings_missing_address_fails() {
        let err = TaxSettings::from_lookup(|k| match k {
            "TAJ_SUPPLIER_TRN" => Some("123-456-789".to_string()),
            "TAJ_SUPPLIER_NAME" => Some("Our Places Jamaica Limited".to_string()),
            _ => None,
        })
        .unwrap_err();
        assert!(err.contains("TAJ_SUPPLIER_ADDRESS is required"));
    }

    #[test]
    fn test_tax_settings_empty_address_fails() {
        let err = TaxSettings::from_lookup(|k| match k {
            "TAJ_SUPPLIER_TRN" => Some("123-456-789".to_string()),
            "TAJ_SUPPLIER_NAME" => Some("Our Places Jamaica Limited".to_string()),
            "TAJ_SUPPLIER_ADDRESS" => Some("   ".to_string()),
            _ => None,
        })
        .unwrap_err();
        assert!(err.contains("TAJ_SUPPLIER_ADDRESS cannot be empty"));
    }

    #[test]
    fn test_tax_settings_valid_succeeds() {
        let settings = TaxSettings::from_lookup(|k| match k {
            "TAJ_SUPPLIER_TRN" => Some("123-456-789".to_string()),
            "TAJ_SUPPLIER_NAME" => Some("Our Places Jamaica Limited".to_string()),
            "TAJ_SUPPLIER_ADDRESS" => Some("Main Street, Ocho Rios, Jamaica".to_string()),
            _ => None,
        })
        .unwrap();
        assert_eq!(settings.taj_supplier_trn, "123-456-789");
        assert_eq!(settings.supplier_name, "Our Places Jamaica Limited");
        assert_eq!(settings.supplier_address, "Main Street, Ocho Rios, Jamaica");
        let supplier = settings.to_supplier_info();
        assert_eq!(supplier.supplier_trn, "123-456-789");
    }

    #[test]
    fn test_cron_settings_missing_secret_fails() {
        let err = CronSettings::from_lookup(|_| None).unwrap_err();
        assert!(err.contains("CRON_SECRET is required"));
    }

    #[test]
    fn test_cron_settings_empty_secret_fails() {
        let err = CronSettings::from_lookup(|k| {
            if k == "CRON_SECRET" {
                Some("   ".to_string())
            } else {
                None
            }
        })
        .unwrap_err();
        assert!(err.contains("CRON_SECRET cannot be empty"));
    }

    #[test]
    fn test_cron_settings_valid_succeeds() {
        let settings = CronSettings::from_lookup(|k| {
            if k == "CRON_SECRET" {
                Some("super-secret-cron-token".to_string())
            } else {
                None
            }
        })
        .unwrap();
        assert_eq!(settings.cron_secret, "super-secret-cron-token");
    }
}
