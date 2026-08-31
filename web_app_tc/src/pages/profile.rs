use topcoat::{
    Result,
    context::Cx,
    router::page,
    view::view,
};

#[page("/profile")]
pub async fn profile_page(_cx: &Cx) -> Result {
    view! {
        <div class="max-w-3xl mx-auto px-2 py-8 space-y-8">
            <div class="card bg-base-100 border border-base-200 shadow-md p-8 rounded-3xl space-y-6">
                <div class="flex items-center gap-4">
                    <div class="avatar placeholder">
                        <div class="bg-primary text-primary-content rounded-full w-16 text-2xl font-bold flex items-center justify-center">
                            <span id="profile-avatar-initials">"JD"</span>
                        </div>
                    </div>
                    <div>
                        <h1 class="text-2xl font-extrabold tracking-tight text-base-content" id="profile-display-name">
                            "Jane Doe"
                        </h1>
                        <p class="text-xs text-base-content/60" id="profile-display-email">
                            "jane.doe@example.com · Verified Guest"
                        </p>
                    </div>
                </div>

                <div class="divider"></div>

                <form
                    action="/profile"
                    method="GET"
                    class="space-y-4"
                    onsubmit="event.preventDefault(); var name = this.first_name.value + ' ' + this.last_name.value; try { localStorage.setItem('op_auth_user', JSON.stringify({ name: name, email: document.getElementById('profile-display-email').innerText.split(' · ')[0], role: 'guest' })); } catch(e){} alert('Profile preferences saved successfully!');"
                >
                    <div class="grid grid-cols-1 sm:grid-cols-2 gap-4">
                        <div>
                            <label class="label text-xs font-bold uppercase text-base-content/70">"First Name"</label>
                            <input type="text" name="first_name" id="profile-input-first-name" value="Jane" class="input input-bordered w-full" />
                        </div>
                        <div>
                            <label class="label text-xs font-bold uppercase text-base-content/70">"Last Name"</label>
                            <input type="text" name="last_name" id="profile-input-last-name" value="Doe" class="input input-bordered w-full" />
                        </div>
                    </div>

                    <div>
                        <label class="label text-xs font-bold uppercase text-base-content/70">"Preferred Currency"</label>
                        <select name="currency" class="select select-bordered w-full">
                            <option value="USD" selected="selected">"USD — United States Dollar ($)"</option>
                            <option value="JMD">"JMD — Jamaican Dollar (J$)"</option>
                            <option value="EUR">"EUR — Euro (€)"</option>
                            <option value="GBP">"GBP — British Pound (£)"</option>
                            <option value="CAD">"CAD — Canadian Dollar (C$)"</option>
                        </select>
                    </div>

                    <div class="pt-4 flex justify-between items-center">
                        <button type="submit" class="btn btn-primary font-bold">
                            "Save Changes"
                        </button>
                        <a href="/logout" class="btn btn-ghost text-error font-bold">
                            "Log Out"
                        </a>
                    </div>
                </form>
            </div>
        </div>

        <script>
            r#"
            (function() {
                try {
                    var userJson = localStorage.getItem('op_auth_user');
                    if (userJson) {
                        var user = JSON.parse(userJson);
                        var name = user.name || 'Jane Doe';
                        var email = user.email || 'jane.doe@example.com';
                        var parts = name.trim().split(' ');
                        var first = parts[0] || 'Jane';
                        var last = (parts.length === 1) ? '' : (parts[1] || 'Doe');
                        
                        var nameEl = document.getElementById('profile-display-name');
                        var emailEl = document.getElementById('profile-display-email');
                        var initialsEl = document.getElementById('profile-avatar-initials');
                        var firstInput = document.getElementById('profile-input-first-name');
                        var lastInput = document.getElementById('profile-input-last-name');
                        
                        if (nameEl) nameEl.innerText = name;
                        if (emailEl) emailEl.innerText = email + ' · Verified Guest';
                        if (firstInput) firstInput.value = first;
                        if (lastInput) lastInput.value = last;
                        if (initialsEl) {
                            var firstChar = first ? first[0] : 'J';
                            var lastChar = last ? last[0] : '';
                            initialsEl.innerText = (firstChar + lastChar).toUpperCase();
                        }
                    }
                } catch (e) {
                    console.error('Failed to populate profile fields:', e);
                }
            })();
            "#
        </script>
    }
}
