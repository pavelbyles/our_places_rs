//! Tests for open-redirect protection.

use web_client::redirect::safe_redirect;

#[test]
fn keeps_same_origin_relative_paths() {
    assert_eq!(safe_redirect("/bookings"), "/bookings");
    assert_eq!(
        safe_redirect("/checkout/villa?check_in=2027-01-02"),
        "/checkout/villa?check_in=2027-01-02"
    );
}

#[test]
fn rejects_everything_else() {
    let long = format!("/{}", "a".repeat(512));
    for bad in [
        "",
        "bookings",
        "//evil.com",
        "/\\evil.com",
        "/\t/evil.com",
        "https://evil.com",
        "javascript:alert(1)",
        "/ok\r\nSet-Cookie: x=1",
        long.as_str(),
    ] {
        assert_eq!(safe_redirect(bad), "/", "{bad:?} should be rejected");
    }
}
