//! Self-hosted fonts (bundled as assets, so `font-src 'self'` holds):
//! Newsreader for display headings and Inter for body text
//! (docs/design/guest-theme.md §2).

use topcoat::font::{Font, fontsource::fontsource_font};

/// Newsreader, light and regular, Latin subset.
pub const NEWSREADER: Font = fontsource_font!(
    NEWSREADER,
    weight: [300, 400],
    style: Normal,
    subset: [Latin],
    host: Asset,
);

/// Inter, regular / medium / semibold, Latin subset.
pub const INTER: Font = fontsource_font!(
    INTER,
    weight: [400, 500, 600],
    style: Normal,
    subset: [Latin],
    host: Asset,
);
