use leptos::*;
use leptos::prelude::*;

#[component]
pub fn VillaCard(
    name: &'static str,
    location: &'static str,
    beds: u8,
    min_stay: u8,
    image_url: &'static str,
) -> impl IntoView {
    view! {
        <div class="villa-card rounded-2xl bg-white shadow-sm hover:shadow-xl transition-all duration-300 group">
            // Image Section (Full Width, No Top Border)
            <div class="relative h-64 w-full overflow-hidden">
                <img 
                    src=image_url 
                    alt=name 
                    class="w-full h-full object-cover group-hover:scale-105 transition-transform duration-700"
                />
                
                // Image Navigation Arrows (Visible on Hover - Logic placeholder)
                <button class="absolute left-2 top-1/2 -translate-y-1/2 bg-white/80 p-1.5 rounded-full opacity-0 group-hover:opacity-100 transition shadow-sm hover:bg-white text-gray-700">
                    "❮"
                </button>
                <button class="absolute right-2 top-1/2 -translate-y-1/2 bg-white/80 p-1.5 rounded-full opacity-0 group-hover:opacity-100 transition shadow-sm hover:bg-white text-gray-700">
                    "❯"
                </button>

                // Favorite Heart Icon
                <button class="absolute top-4 right-4 text-white hover:text-red-500 drop-shadow-md text-2xl">
                    "♡"
                </button>
            </div>

            // Details Section
            <div class="p-5">
                <div class="flex justify-between items-start mb-2">
                    <div>
                        <h3 class="font-bold text-lg text-[var(--text-primary)] mb-1">{name}</h3>
                        <p class="text-sm text-[var(--text-secondary)] flex items-center gap-1">
                            <span>"📍"</span> {location}
                        </p>
                    </div>
                </div>

                <div class="flex items-center gap-4 mt-4 pt-4 border-t border-gray-100 text-sm text-[var(--text-secondary)]">
                    <span class="flex items-center gap-1">
                        "🛏️" {beds} " Beds"
                    </span>
                    <span class="flex items-center gap-1">
                        "🌙" "Min " {min_stay} " Nights"
                    </span>
                </div>
            </div>
        </div>
    }
}
