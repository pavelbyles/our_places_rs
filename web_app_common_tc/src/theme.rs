use topcoat::{
    Result,
    view::{component, view},
};

/// Inline script that executes synchronously before DOM paint to prevent FOUC.
/// Evaluates `localStorage` for manual preference, or defaults based on time of day (>= 18:00 is dark grey-blue 'night').
pub fn theme_init_script() -> &'static str {
    r#"
    (function() {
        try {
            var stored = localStorage.getItem('theme');
            var theme = stored;
            if (!theme) {
                var hour = new Date().getHours();
                theme = ([18, 19, 20, 21, 22, 23, 0, 1, 2, 3, 4, 5].indexOf(hour) !== -1) ? 'night' : 'emerald';
            }
            if (theme === 'sunset') theme = 'night'; // migrate legacy sunset to dark slate-navy night
            document.documentElement.setAttribute('data-theme', theme);
            if (theme === 'night') {
                document.documentElement.classList.add('dark');
            } else {
                document.documentElement.classList.remove('dark');
            }
        } catch (e) {
            document.documentElement.setAttribute('data-theme', 'emerald');
        }
    })();
    "#
}

/// Client-side script to toggle between `emerald` (light) and `night` (dark grey-blue) themes.
pub fn theme_toggle_script() -> &'static str {
    r#"
    function toggleAppTheme() {
        try {
            var current = document.documentElement.getAttribute('data-theme') || 'emerald';
            var next = (current === 'emerald') ? 'night' : 'emerald';
            localStorage.setItem('theme', next);
            document.documentElement.setAttribute('data-theme', next);
            if (next === 'night') {
                document.documentElement.classList.add('dark');
            } else {
                document.documentElement.classList.remove('dark');
            }
        } catch (e) {
            console.error('Failed to toggle theme:', e);
        }
    }
    "#
}

/// Theme Toggle UI Component visible on all pages with sleek Sun/Moon pill switch.
#[component]
pub async fn theme_toggle() -> Result {
    view! {
        <button
            type="button"
            class="flex items-center gap-1 px-2.5 py-1 rounded-full border border-base-content/20 bg-base-100/70 hover:bg-base-200 hover:border-primary/40 transition-all shadow-sm cursor-pointer"
            onclick="toggleAppTheme()"
            aria-label="Toggle light/dark theme"
            title="Toggle Light / Dark Slate-Navy Mode"
        >
            <span class="text-sm">"☀️"</span>
            <span class="text-[10px] text-base-content/40 font-bold">"|"</span>
            <span class="text-sm">"🌙"</span>
        </button>
    }
}
