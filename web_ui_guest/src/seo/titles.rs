//! `<title>`s of the guest's static pages (spec 102 §1.5.3). The site name
//! is appended by [`page_title`]; `/` keeps the router's default.

use topcoat::metadata::Metadata;
use web_ui_common::site_config::page_title;

/// `/listings`.
pub const LISTINGS: &str = "Villas in Jamaica";
/// `/about`.
pub const ABOUT: &str = "About Our Places";
/// `/terms`.
pub const TERMS: &str = "Terms and conditions";
/// `/cancellation-policy`.
pub const CANCELLATION: &str = "Cancellation policy";
/// `/privacy-policy`.
pub const PRIVACY: &str = "Privacy policy";
/// `/house-rules`.
pub const HOUSE_RULES: &str = "House rules";
/// `/login`.
pub const SIGN_IN: &str = "Sign in";
/// `/register`.
pub const REGISTER: &str = "Register";

/// Page metadata setting `title` (with the site name appended).
pub fn metadata(title: &str) -> Metadata {
    Metadata::new().title(page_title(title))
}
