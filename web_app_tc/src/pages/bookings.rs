use topcoat::{
    Result,
    context::Cx,
    router::page,
    view::view,
};

#[page("/bookings")]
pub async fn bookings_page(_cx: &Cx) -> Result {
    view! {
        <div class="max-w-5xl mx-auto px-2 py-8 space-y-8">
            <div class="flex flex-col sm:flex-row justify-between items-start sm:items-center gap-4">
                <div>
                    <h1 class="text-3xl font-extrabold tracking-tight">"My Bookings & Stays"</h1>
                    <p class="text-base-content/70 text-sm mt-1">"Manage your upcoming reservations and view past stay receipts."</p>
                </div>
                <a href="/listings" class="btn btn-primary btn-sm">"+ Book Another Stay"</a>
            </div>

            // Active / Upcoming Reservation Card
            <div class="card bg-base-100 border border-base-300 shadow-md p-6 rounded-3xl space-y-6">
                <div class="flex flex-wrap justify-between items-center gap-2 border-b border-base-200 pb-4">
                    <div class="flex items-center gap-3">
                        <span class="badge badge-success font-bold">"Confirmed Stay"</span>
                        <span class="text-xs text-base-content/60 font-mono">"Booking #OP-2026-8841"</span>
                    </div>
                    <span class="text-xs text-base-content/70">"Booked on Aug 28, 2026"</span>
                </div>

                <div class="grid grid-cols-1 md:grid-cols-4 gap-6 items-center">
                    <img
                        src="https://images.unsplash.com/photo-1580587771525-78b9dba3b914?auto=format&fit=crop&w=600&q=80"
                        alt="Villa Serenity"
                        class="w-full h-36 rounded-2xl object-cover"
                    />

                    <div class="md:col-span-2 space-y-2">
                        <h3 class="text-xl font-bold">"Villa Serenity — Montego Bay"</h3>
                        <p class="text-xs text-base-content/70">"12 Rose Hall Drive, Montego Bay, Jamaica"</p>
                        <div class="grid grid-cols-2 gap-2 text-xs pt-2">
                            <div>
                                <span class="text-base-content/50 font-bold block">"Check-in:"</span>
                                <span class="font-semibold">"Sep 10, 2026 (3:00 PM)"</span>
                            </div>
                            <div>
                                <span class="text-base-content/50 font-bold block">"Check-out:"</span>
                                <span class="font-semibold">"Sep 15, 2026 (11:00 AM)"</span>
                            </div>
                        </div>
                    </div>

                    <div class="flex flex-col gap-2">
                        <a href="/listings/villa-serenity-montego-bay" class="btn btn-outline btn-sm">"View Villa"</a>
                        <a href="/reviews/submit?token=018e0000-0000-7000-8000-000000000099" class="btn btn-primary btn-sm">
                            "★ Write Review"
                        </a>
                    </div>
                </div>
            </div>

            // Past Stays Section
            <div class="space-y-4 pt-4">
                <h2 class="text-xl font-bold tracking-tight">"Past Trips"</h2>
                
                <div class="card bg-base-100 border border-base-200 p-6 rounded-2xl opacity-80 space-y-4">
                    <div class="flex justify-between items-center text-xs">
                        <span class="badge badge-neutral">"Completed"</span>
                        <span class="text-base-content/60">"Jan 10 – Jan 17, 2026"</span>
                    </div>
                    <div class="flex justify-between items-center">
                        <div>
                            <h4 class="font-bold text-base">"Blue Lagoon Sanctuary — Port Antonio"</h4>
                            <p class="text-xs text-base-content/60">"7 nights · Total: USD $4,186.00 (GCT 15% incl.)"</p>
                        </div>
                        <a href="/listings/blue-lagoon-sanctuary-port-antonio" class="btn btn-ghost btn-sm">"Book Again"</a>
                    </div>
                </div>
            </div>
        </div>
    }
}
