use chrono::{DateTime, FixedOffset, Timelike, Utc};
use topcoat::context::Cx;
use topcoat::cookie::{Cookies, cookies};

/// Cookie holding an explicit theme choice (`light`, `dim` or `dark`).
pub const THEME_COOKIE: &str = "op_theme";
/// Cookie holding the visitor's UTC offset in minutes east of UTC, written
/// by `ui.js`.
pub const TZ_OFFSET_COOKIE: &str = "op_tz_offset";

/// `America/Jamaica` is UTC-05:00 all year (no DST). Used on a first visit,
/// before `ui.js` has stored the visitor's offset.
const JAMAICA_OFFSET_MINUTES: i32 = -5 * 60;
/// Widest real-world UTC offsets: -12:00 to +14:00 (allowing ±14:00).
const MAX_OFFSET_MINUTES: i32 = 14 * 60;
/// Local hour at or after which the default theme is dark.
const DARK_FROM_HOUR: u32 = 18;

/// One of the three portal themes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThemeChoice {
    /// The default light theme (`:root`).
    Light,
    /// The light theme one shade lower (`.dim`).
    Dim,
    /// The dark theme (`.dark`).
    Dark,
}

impl ThemeChoice {
    /// Parses a saved choice; `None` for anything unrecognised.
    pub fn from_cookie(value: &str) -> Option<Self> {
        match value {
            "light" => Some(Self::Light),
            "dim" => Some(Self::Dim),
            "dark" => Some(Self::Dark),
            _ => None,
        }
    }

    /// The class to render on `<html>` (empty for light).
    pub fn html_class(self) -> &'static str {
        match self {
            Self::Light => "",
            Self::Dim => "dim",
            Self::Dark => "dark",
        }
    }

    /// The cookie value for this choice.
    pub fn cookie_value(self) -> &'static str {
        match self {
            Self::Light => "light",
            Self::Dim => "dim",
            Self::Dark => "dark",
        }
    }

    /// The theme the toggle switches to next: light → dim → dark → light.
    pub fn next(self) -> Self {
        match self {
            Self::Light => Self::Dim,
            Self::Dim => Self::Dark,
            Self::Dark => Self::Light,
        }
    }

    /// Accessible label for a toggle that switches *to* this theme.
    pub fn toggle_label(self) -> &'static str {
        match self {
            Self::Light => "Switch to light theme",
            Self::Dim => "Switch to dim theme",
            Self::Dark => "Switch to dark theme",
        }
    }
}

/// Parses an `op_tz_offset` value: whole minutes within ±14:00.
pub fn parse_tz_offset(raw: &str) -> Option<i32> {
    raw.trim()
        .parse::<i32>()
        .ok()
        .filter(|m| m.abs() <= MAX_OFFSET_MINUTES)
}

/// Picks the theme for a request: a valid saved choice wins; otherwise dark
/// at or after 18:00 local time, else light. Local time uses the visitor's
/// stored offset, or Jamaica time on a first visit. Dim is never a default.
pub fn resolve_theme(
    theme_cookie: Option<&str>,
    tz_offset_cookie: Option<&str>,
    now_utc: DateTime<Utc>,
) -> ThemeChoice {
    theme_cookie
        .and_then(ThemeChoice::from_cookie)
        .unwrap_or_else(|| {
            let offset_minutes = tz_offset_cookie
                .and_then(parse_tz_offset)
                .unwrap_or(JAMAICA_OFFSET_MINUTES);
            let local_hour = FixedOffset::east_opt(offset_minutes * 60)
                .map(|tz| now_utc.with_timezone(&tz).hour())
                .unwrap_or_else(|| now_utc.hour());
            if local_hour >= DARK_FROM_HOUR {
                ThemeChoice::Dark
            } else {
                ThemeChoice::Light
            }
        })
}

/// Resolves the theme for the current request from its cookies. Requires
/// the router's cookie layer (`.cookies()`).
pub fn theme_for_request(cx: &Cx) -> ThemeChoice {
    let jar = cookies(cx);
    let theme = jar.get(THEME_COOKIE);
    let offset = jar.get(TZ_OFFSET_COOKIE);
    resolve_theme(
        theme.as_ref().map(|c| c.value()),
        offset.as_ref().map(|c| c.value()),
        Utc::now(),
    )
}
