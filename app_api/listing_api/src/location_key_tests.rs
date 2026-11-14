use super::*;

fn env_with(value: Option<&str>) -> impl Fn(&str) -> Option<String> {
    let value = value.map(str::to_string);
    move |key: &str| match key == LOCATION_OFFSET_KEY_ENV {
        true => value.clone(),
        false => None,
    }
}

#[test]
fn a_missing_blank_or_short_key_is_refused_by_name() {
    let short = "s".repeat(31);
    for (value, reason) in [
        (None, LocationKeyError::Missing),
        (Some("   "), LocationKeyError::Missing),
        (Some(short.as_str()), LocationKeyError::TooShort),
    ] {
        match load(env_with(value)) {
            Ok(_) => panic!("{value:?} was accepted"),
            Err(e) => {
                assert_eq!(e.reason, reason);
                let message = e.to_string();
                assert!(message.contains(LOCATION_OFFSET_KEY_ENV), "{message}");
                if let Some(v) = value.filter(|v| !v.trim().is_empty()) {
                    assert!(!message.contains(v), "the value leaked: {message}");
                }
            }
        }
    }
}

#[test]
fn a_key_of_32_bytes_or_more_loads() {
    assert!(load(env_with(Some(&"k".repeat(32)))).is_ok());
    assert!(load(env_with(Some(TEST_KEY))).is_ok());
}
