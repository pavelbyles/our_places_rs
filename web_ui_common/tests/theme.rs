//! Tests for the server-side theme choice (spec FR-3.6, D7).

use chrono::{DateTime, TimeZone, Utc};
use web_ui_common::theme::{ThemeChoice, parse_tz_offset, resolve_theme};

fn utc(h: u32, m: u32) -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2027, 3, 10, h, m, 0)
        .single()
        .unwrap_or_default()
}

#[test]
fn saved_choice_always_wins() {
    for (cookie, want) in [
        ("light", ThemeChoice::Light),
        ("dim", ThemeChoice::Dim),
        ("dark", ThemeChoice::Dark),
    ] {
        assert_eq!(resolve_theme(Some(cookie), Some("540"), utc(3, 0)), want);
        assert_eq!(resolve_theme(Some(cookie), None, utc(23, 0)), want);
    }
}

#[test]
fn default_follows_local_time_of_day() {
    // UTC offset 0, so UTC time is local time.
    for (h, m, want) in [
        (0, 0, ThemeChoice::Dark),
        (5, 59, ThemeChoice::Dark),
        (6, 0, ThemeChoice::Light),
        (15, 59, ThemeChoice::Light),
        (16, 0, ThemeChoice::Dim),
        (18, 59, ThemeChoice::Dim),
        (19, 0, ThemeChoice::Dark),
        (23, 59, ThemeChoice::Dark),
    ] {
        assert_eq!(
            resolve_theme(None, Some("0"), utc(h, m)),
            want,
            "{h:02}:{m:02}"
        );
    }
}

#[test]
fn first_visit_uses_jamaica_time() {
    // Jamaica is UTC-05:00 with no DST.
    // 10:59 UTC = 05:59, 11:00 UTC = 06:00.
    assert_eq!(resolve_theme(None, None, utc(10, 59)), ThemeChoice::Dark);
    assert_eq!(resolve_theme(None, None, utc(11, 0)), ThemeChoice::Light);
    // 20:59 UTC = 15:59, 21:00 UTC = 16:00.
    assert_eq!(resolve_theme(None, None, utc(20, 59)), ThemeChoice::Light);
    assert_eq!(resolve_theme(None, None, utc(21, 0)), ThemeChoice::Dim);
    // 23:59 UTC = 18:59, 00:00 UTC = 19:00.
    assert_eq!(resolve_theme(None, None, utc(23, 59)), ThemeChoice::Dim);
    assert_eq!(resolve_theme(None, None, utc(0, 0)), ThemeChoice::Dark);
}

#[test]
fn visitor_offset_overrides_jamaica() {
    // UTC+09:00: 10:00 UTC is 19:00 local; 09:59 UTC is 18:59.
    assert_eq!(
        resolve_theme(None, Some("540"), utc(10, 0)),
        ThemeChoice::Dark
    );
    assert_eq!(
        resolve_theme(None, Some("540"), utc(9, 59)),
        ThemeChoice::Dim
    );
    // 01:00 UTC is 10:00 at UTC+09:00 (light) but 20:00 in Jamaica (dark).
    assert_eq!(
        resolve_theme(None, Some("540"), utc(1, 0)),
        ThemeChoice::Light
    );
    // UTC-14:00 extreme still resolves (wraps to the previous day):
    // 08:00 UTC is 18:00 local.
    assert_eq!(
        resolve_theme(None, Some("-840"), utc(8, 0)),
        ThemeChoice::Dim
    );
}

#[test]
fn invalid_cookies_fall_back() {
    assert_eq!(
        resolve_theme(Some("purple"), Some("9999"), utc(1, 0)),
        ThemeChoice::Dark
    );
    assert_eq!(
        resolve_theme(Some(""), Some("abc"), utc(15, 0)),
        ThemeChoice::Light
    );
    assert_eq!(parse_tz_offset("841"), None);
    assert_eq!(parse_tz_offset("-841"), None);
    assert_eq!(parse_tz_offset(" 60 "), Some(60));
    assert_eq!(parse_tz_offset("1.5"), None);
}

#[test]
fn html_class_and_cycle() {
    assert_eq!(ThemeChoice::Light.html_class(), "");
    assert_eq!(ThemeChoice::Dim.html_class(), "dim");
    assert_eq!(ThemeChoice::Dark.html_class(), "dark");
    assert_eq!(ThemeChoice::Light.next(), ThemeChoice::Dim);
    assert_eq!(ThemeChoice::Dim.next(), ThemeChoice::Dark);
    assert_eq!(ThemeChoice::Dark.next(), ThemeChoice::Light);
    assert_eq!(
        ThemeChoice::Light.next().toggle_label(),
        "Switch to dim theme"
    );
}
