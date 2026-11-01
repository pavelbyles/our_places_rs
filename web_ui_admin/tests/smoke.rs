//! Smoke tests for the portal crate.

#[test]
fn default_port_is_3012() {
    assert_eq!(web_ui_admin::DEFAULT_PORT, 3012);
}
