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
                        <div class="bg-primary text-primary-content rounded-full w-16 text-2xl font-bold">
                            <span>"JD"</span>
                        </div>
                    </div>
                    <div>
                        <h1 class="text-2xl font-extrabold tracking-tight">"Jane Doe"</h1>
                        <p class="text-xs text-base-content/60">"jane.doe@example.com · Verified Guest"</p>
                    </div>
                </div>

                <div class="divider"></div>

                <form action="/profile" method="GET" class="space-y-4">
                    <div class="grid grid-cols-1 sm:grid-cols-2 gap-4">
                        <div>
                            <label class="label text-xs font-bold uppercase text-base-content/70">"First Name"</label>
                            <input type="text" name="first_name" value="Jane" class="input input-bordered w-full" />
                        </div>
                        <div>
                            <label class="label text-xs font-bold uppercase text-base-content/70">"Last Name"</label>
                            <input type="text" name="last_name" value="Doe" class="input input-bordered w-full" />
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
                        <button type="submit" class="btn btn-primary">
                            "Save Changes"
                        </button>
                        <a href="/login" class="btn btn-ghost text-error">"Log Out"</a>
                    </div>
                </form>
            </div>
        </div>
    }
}
