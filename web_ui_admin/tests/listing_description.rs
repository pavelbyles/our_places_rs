//! Description limits and API rejection messages on the listing studio form.

use uuid::Uuid;
use web_ui_admin::pages::listings::form::{
    ListingFields, MAX_DESCRIPTION_CHARS, api_rejection_messages,
};

fn fields(description: &str) -> ListingFields {
    ListingFields {
        name: "Sea Grape Villa".into(),
        price_per_night: "450".into(),
        description: description.into(),
        ..ListingFields::blank(Uuid::now_v7())
    }
}

#[test]
fn rejects_description_over_the_api_limit() {
    let errors = fields(&"x".repeat(MAX_DESCRIPTION_CHARS + 1))
        .validate(None, false)
        .err()
        .unwrap_or_default();
    assert!(
        errors
            .iter()
            .any(|e| e.contains("at most 2000 characters (it is 2001)")),
        "{errors:?}"
    );
}

#[test]
fn counts_browser_crlf_newlines_once() {
    // 1000 lines of "x" = 1999 chars with LF, 2998 with CRLF.
    let text = vec!["x"; 1000].join("\r\n");
    let description = fields(&text)
        .validate(None, false)
        .map(|v| v.into_new().description)
        .map_err(|e| e.join("; "));
    assert_eq!(description, Ok(Some(vec!["x"; 1000].join("\n"))));
}

#[test]
fn reads_validator_field_messages() {
    let err = r#"Failed to create listing (400 Bad Request): {"description":[{"code":"length","message":"Description cannot be longer than 2000 characters","params":{"max":2000}}]}"#;
    assert_eq!(
        api_rejection_messages(&err),
        Some(vec![
            "Description cannot be longer than 2000 characters".to_string()
        ])
    );
}

#[test]
fn falls_back_to_field_and_code_without_a_message() {
    let err = r#"Failed (400): {"name":[{"code":"length","params":{}}]}"#;
    assert_eq!(
        api_rejection_messages(&err),
        Some(vec!["name: length".to_string()])
    );
}

#[test]
fn reads_error_and_message_bodies() {
    let err = r#"Failed (400): {"error":"Owner has no host profile"}"#;
    assert_eq!(
        api_rejection_messages(&err),
        Some(vec!["Owner has no host profile".to_string()])
    );
    let err = r#"Failed (422): {"code":"FX_RATE_UNAVAILABLE","message":"No rate"}"#;
    assert_eq!(
        api_rejection_messages(&err),
        Some(vec!["No rate".to_string()])
    );
}

#[test]
fn none_without_a_json_body() {
    assert_eq!(
        api_rejection_messages(&"Failed to connect to listing service"),
        None
    );
    assert_eq!(api_rejection_messages(&"Failed (500): "), None);
}
