# Guest Portal Design: "Shoreline Mist"

Locked design reference for the redesigned guest portal (`web_ui_guest`, issue #96).
Approved mockups: [Our Places Villa Designs canvas](https://claude.ai/artifact/9bcyME8GY21LR5zrjrZVHy), artboards **D · Shoreline Mist · Light**, **· Dim** and **· Dusk (dark)**. The canvas is private to the owner.

The direction is **A's layout with B's colours**: editorial serif headlines, a wide landscape hero with the search bar laid over it, portrait villa photography, a mist and deep-teal palette, and softly rounded corners. The feel to aim for is clean, modern and calm.

## 1. Theme tokens (Topcoat UI `styles.css`)

Token names follow the Topcoat UI `neutral` theme, so registry components pick these values up without edits. There are **three themes**: light (`:root`), dim (`.dim` on `<html>`) and dark (`.dark` on `<html>`). Only one of `.dim` / `.dark` is ever set. Tailwind's `dark:` variant stays bound to `.dark` only, because dim is a lowered *light* theme, not a dark one.

```css
:root {
  --background: #EEF3F4;          /* mist page ground */
  --foreground: #0F2A33;          /* deep ink */
  --card: #FFFFFF;                /* search bar, panels, dialogs */
  --card-foreground: #0F2A33;
  --popover: #FFFFFF;
  --popover-foreground: #0F2A33;
  --muted-foreground: #4E6670;    /* captions, meta, labels */
  --primary: #245A6D;             /* deep teal: primary actions, links */
  --primary-foreground: #FFFFFF;
  --destructive: #B42318;
  --destructive-foreground: #FFFFFF;
  --border: #D3DEE2;              /* hairlines, outlined pills */
  --ring: #245A6D;
  --sidebar: #FFFFFF;
  --sidebar-foreground: #0F2A33;
  --sidebar-primary: #245A6D;
  --sidebar-primary-foreground: #FFFFFF;
  --sidebar-accent: #E3EEF1;
  --sidebar-accent-foreground: #0F2A33;
  --sidebar-border: #D3DEE2;
  --sidebar-ring: #245A6D;

  /* Guest-only extensions */
  --divider: #E3EAEC;             /* field separators inside the search bar */
  --chip: #E3EEF1;                /* soft pill / secondary button fill */
  --photo-placeholder: #D4E3E7;   /* image skeleton before load */
}

.dim {
  --background: #D9E2E5;          /* the light theme, one shade lower */
  --foreground: #0F2A33;
  --card: #EAF0F2;
  --card-foreground: #0F2A33;
  --popover: #EAF0F2;
  --popover-foreground: #0F2A33;
  --muted-foreground: #41575F;
  --primary: #1E5062;
  --primary-foreground: #FFFFFF;
  --destructive: #A11F15;
  --destructive-foreground: #FFFFFF;
  --border: #BBC9CF;
  --ring: #1E5062;
  --sidebar: #EAF0F2;
  --sidebar-foreground: #0F2A33;
  --sidebar-primary: #1E5062;
  --sidebar-primary-foreground: #FFFFFF;
  --sidebar-accent: #CCDCE1;
  --sidebar-accent-foreground: #0F2A33;
  --sidebar-border: #BBC9CF;
  --sidebar-ring: #1E5062;

  --divider: #CDD8DC;
  --chip: #CCDCE1;
  --photo-placeholder: #BFD2D8;
}

.dark {
  --background: #34404F;          /* dusky slate-blue (NOT near-black) */
  --foreground: #EEF2F6;
  --card: #3E4A5B;
  --card-foreground: #EEF2F6;
  --popover: #3E4A5B;
  --popover-foreground: #EEF2F6;
  --muted-foreground: #BAC4D0;
  --primary: #AECBEB;             /* pale periwinkle-sky */
  --primary-foreground: #1E2A38;  /* dark text on light accent */
  --destructive: #F4A79D;
  --destructive-foreground: #3A1410;
  --border: #4D5A6C;
  --ring: #AECBEB;
  --sidebar: #3E4A5B;
  --sidebar-foreground: #EEF2F6;
  --sidebar-primary: #AECBEB;
  --sidebar-primary-foreground: #1E2A38;
  --sidebar-accent: #465365;
  --sidebar-accent-foreground: #EEF2F6;
  --sidebar-border: #4D5A6C;
  --sidebar-ring: #AECBEB;

  --divider: #465365;
  --chip: #465365;
  --photo-placeholder: #4A5970;
}
```

Links use `--primary` in light and dim. In dark they use `#BCD5F0`, a step lighter than `--primary`, for comfortable reading on `--background`.

**Contrast:** body text, `--muted-foreground` text and button labels all meet WCAG AA (≥ 4.5:1) on their backgrounds in all three themes. Re-check any new token pairing before use.

## 2. Typography

| Role | Family | Weight | Size |
|---|---|---|---|
| Display (hero `h1`) | Newsreader | 300 | `clamp(48px, 7vw, 96px)`, line-height 1.02, tracking -0.025em |
| Section heading (`h2`) | Newsreader | 400 | 40px, tracking -0.015em (32px for the trust strip) |
| Card title (`h3`) | Newsreader | 400 | 22–24px |
| Wordmark | Newsreader | 400 | 26px header / 20px footer |
| Body | Inter | 400 | 16px, line-height 1.55; lead paragraphs 18px |
| UI labels (form field labels) | Inter | 600 | 12px, uppercase, tracking 0.06em, `--muted-foreground` |
| Buttons / nav | Inter | 500 | 15px |

Load both fonts via `topcoat-font` (Fontsource), not a Google Fonts `<link>`, so that no third-party request is needed and the CSP stays strict.

## 3. Shape & spacing

| Element | Radius |
|---|---|
| Primary action button (e.g. "Search villas") | 10px |
| Search bar, hero image, dialogs/sheets | 16px |
| Destination tiles, villa photos, cards | 12px |
| Nav pills: Sign in, currency, theme toggle | 999px (full pill) |
| Inputs inside the search bar | borderless, separated by 1px `--divider` |

- Content max-width **1540px**, side gutter **24px**.
- Vertical rhythm between sections: **112px** (128px before the trust strip and footer).
- Grids use `repeat(auto-fit, minmax(…, 1fr))`: villa cards min 300px, destination tiles min 190px, trust items min 260px. They stack at phone width.
- Touch targets ≥ 44px. Shadows are minimal (the search bar only: `0 1px 2px rgb(15 42 51 / 0.08)`).

## 4. Home page structure (reference layout)

1. **Header:** wordmark left. Nav right: Villas, Destinations, For hosts, currency pill (`USD`), theme toggle (moon icon, 44px pill), and a filled **Sign in** pill (`--primary`).
2. **Hero:** two-part intro (large serif headline left, lead paragraph right), then a full-width landscape photo (560px tall, 16px radius) with the **search bar overlaid** at the bottom. Fields: Where · Check in · Check out · Guests · [Search villas].
3. **"Where would you like to wake up?":** square destination tiles (12px radius) with serif names: Ocho Rios, Montego Bay, Negril, Port Antonio, Kingston.
4. **"This season's stays":** three portrait villa cards (4:5 photo, 12px radius). Below each: serif name, `location · bedrooms · guests` in muted text, and "from [rate] per night" on the right. A "View all villas →" link sits top right.
5. **Trust strip "Booking, without the noise.":** a hairline rule above, then three items: verified reviews, the 15-minute date hold, prices in your currency.
6. **Footer:** a hairline rule, the wordmark, and links: List your villa, Help, Terms, Privacy.

## 5. Theme behaviour (light / dim / dark)

- Three themes: **light** (default), **dim** (light lowered one shade, easier on the eyes without going dark) and **dark** (the dusky slate-blue palette above).
- The header theme button (44px pill, moon icon) **cycles light → dim → dark → light**. Its `aria-label` names the next theme, e.g. "Switch to dim theme".
- Default with no saved choice, by local time: light 06:00–15:59, dim 16:00–18:59, dark 19:00–05:59.
- The choice is remembered in a cookie the server reads, so the right class (`.dim` / `.dark` / none) is rendered into the HTML with no flash and no inline script. `ui.js` handles the button and writes the cookie.
- In every theme, photography stand-ins and image skeletons use `--photo-placeholder`. Real photos are shown unfiltered.

## 6. Admin portal

The admin portal (`web_ui_admin`) uses the Topcoat UI **`neutral`** theme, swapping only `--primary` / `--ring` / `--sidebar-primary` to the brand teal `#245A6D` (dim: `#1E5062`; dark: `#AECBEB` with `#1E2A38` text), so the two portals visibly belong together. Admin offers the same three-way toggle.

## 7. Out of scope for this reference

Listing detail, checkout, account and messaging pages are not mocked up yet. They follow the tokens, type and shape rules above. Any new pattern they introduce should be added to this file.
