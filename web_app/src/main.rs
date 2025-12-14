use leptos::*;
use leptos::prelude::*;
use components::navbar::Navbar;
use components::hero::Hero;
use components::featured::Featured;

pub mod components;

fn main() {
    _ = console_log::init_with_level(log::Level::Debug);
    console_error_panic_hook::set_once();
    
    mount_to_body(|| view! {
        <div class="min-h-screen bg-[var(--bg-secondary)] font-sans text-gray-900">
            <Navbar />
            <Hero />
            <Featured />
            
            // Simple Footer
            <footer class="bg-gray-50 border-t border-gray-200 mt-16 py-12 text-center text-gray-500 text-sm">
                <p>"© 2025 Our Places. All rights reserved."</p>
                <div class="mt-2 flex justify-center gap-4">
                    <a href="#" class="hover:text-gray-900">"Privacy"</a>
                    <a href="#" class="hover:text-gray-900">"Terms"</a>
                    <a href="#" class="hover:text-gray-900">"Contact"</a>
                </div>
            </footer>
        </div>
    })
}
