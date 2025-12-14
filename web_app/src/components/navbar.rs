use leptos::*;
use leptos::prelude::*;

#[component]
pub fn Navbar() -> impl IntoView {
    view! {
        <nav class="navbar w-full flex justify-between items-center py-4 px-12 bg-white/90 backdrop-blur">
            // Logo
            <div class="flex items-center gap-4">
                <a href="/" class="text-2xl font-bold flex items-center gap-2 text-[var(--accent-blue)]">
                    <span class="text-3xl">"⛱️"</span>
                    <span>"Our Places"</span>
                </a>
            </div>

            // Center Links
            <div class="hidden md:flex items-center gap-8">
                <a href="/villas" class="font-medium text-gray-600 hover:text-[var(--accent-blue)] transition">"Villas"</a>
                <a href="/concierge" class="font-medium text-gray-600 hover:text-[var(--accent-blue)] transition">"Concierge"</a>
                <a href="/about" class="font-medium text-gray-600 hover:text-[var(--accent-blue)] transition">"About Us"</a>
            </div>

            // Right Actions
            <div class="flex items-center gap-6">
                <button class="font-medium text-gray-600 hover:text-[var(--accent-blue)]">"Sign In"</button>
                <button class="border border-[var(--accent-blue)] text-[var(--accent-blue)] font-medium px-4 py-2 rounded-full hover:bg-blue-50 transition">"Register"</button>
                
                // Settings
                <div class="flex items-center gap-3 border-l pl-4 ml-2 border-gray-300">
                    <select class="bg-transparent text-sm font-medium outline-none text-gray-600 cursor-pointer">
                        <option value="en">"EN"</option>
                        <option value="es">"ES"</option>
                    </select>
                    <button class="p-2 rounded-full hover:bg-gray-100 text-gray-600" title="Toggle Theme">
                        "🌙"
                    </button>
                </div>
            </div>
        </nav>
    }
}
