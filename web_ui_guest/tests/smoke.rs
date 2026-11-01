//! Smoke tests for the portal crate.

#[test]
fn default_port_is_3010() {
    assert_eq!(web_ui_guest::DEFAULT_PORT, 3010);
}
