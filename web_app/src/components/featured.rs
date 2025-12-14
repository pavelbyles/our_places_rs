use leptos::*;
use leptos::prelude::*;
use crate::components::card::VillaCard;

#[component]
pub fn Featured() -> impl IntoView {
    view! {
        <section class="container py-16">
            <div class="flex justify-between items-end mb-8">
                <div>
                    <h2 class="text-3xl font-bold text-[var(--text-primary)] mb-2">"Featured Villas"</h2>
                    <p class="text-[var(--text-secondary)]">"Handpicked luxury for your next escape"</p>
                </div>
                <a href="/villas" class="text-[var(--accent-blue)] font-bold hover:text-[var(--accent-hover)] transition">
                    "View All Villas →"
                </a>
            </div>

            <div class="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-8">
                <VillaCard 
                    name="Seaside Paradise" 
                    location="Malibu, CA" 
                    beds=4 
                    min_stay=3 
                    image_url="https://images.unsplash.com/photo-1512917774080-9991f1c4c750?q=80&w=2070&auto=format&fit=crop"
                />
                <VillaCard 
                    name="Mountain Retreat" 
                    location="Aspen, CO" 
                    beds=3 
                    min_stay=2 
                    image_url="https://images.unsplash.com/photo-1622396481328-9b1b78cdd9fd?q=80&w=1974&auto=format&fit=crop"
                />
                <VillaCard 
                    name="Tropical Oasis" 
                    location="Bali, Indonesia" 
                    beds=5 
                    min_stay=4 
                    image_url="https://images.unsplash.com/photo-1571896349842-6e5a513e610a?q=80&w=2070&auto=format&fit=crop"
                />
            </div>
        </section>
    }
}
