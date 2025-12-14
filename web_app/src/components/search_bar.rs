use leptos::*;
use leptos::prelude::*;

#[component]
pub fn SearchBar() -> impl IntoView {
    view! {
        <div class="search-bar-container bg-white shadow-xl p-6 flex flex-col md:flex-row items-end md:items-center gap-8 max-w-6xl mx-auto -mt-12 relative z-20 rounded-3xl">
            
            // Location
            <div class="flex-1 w-full border-b border-gray-200 pb-3 flex items-center gap-3">
                <span class="text-gray-400 text-lg">"📍"</span> 
                <div class="flex-1">
                    <label class="block text-xs font-bold text-gray-500 uppercase mb-1">"Location"</label>
                    <input 
                        type="text" 
                        placeholder="Choose Location" 
                        class="w-full text-gray-700 font-medium outline-none bg-transparent placeholder-gray-400"
                    />
                </div>
            </div>

            // Dates
            <div class="flex-1 w-full border-b border-gray-200 pb-3 flex items-center gap-3">
                <span class="text-gray-400 text-lg">"📅"</span>
                <div class="flex-1">
                    <label class="block text-xs font-bold text-gray-500 uppercase mb-1">"Dates"</label>
                    <input 
                        type="text" 
                        placeholder="Add Dates" 
                        class="w-full text-gray-700 font-medium outline-none bg-transparent placeholder-gray-400"
                    />
                </div>
            </div>

            // Guests
             <div class="w-full md:w-auto min-w-[160px] border-b border-gray-200 pb-3 flex items-center justify-between gap-3">
                <div class="flex items-center gap-2">
                    <span class="text-gray-400 text-lg">"👥"</span>
                    <div class="flex flex-col">
                        <label class="block text-xs font-bold text-gray-500 uppercase mb-1">"Guests"</label>
                        <span class="text-gray-700 font-medium">"1 Guest"</span>
                    </div>
                </div>
                <div class="flex items-center gap-3 text-[var(--accent-blue)] text-xl font-bold">
                    <button class="hover:opacity-75 transition px-1">"+"</button>
                    <button class="hover:opacity-75 transition px-1">"−"</button>
                 </div>
            </div>

            // Bedrooms
             <div class="w-full md:w-auto min-w-[170px] border-b border-gray-200 pb-3 flex items-center justify-between gap-3">
                <div class="flex items-center gap-2">
                     <span class="text-gray-400 text-lg">"🛏️"</span>
                     <div class="flex flex-col">
                        <label class="block text-xs font-bold text-gray-500 uppercase mb-1">"Bedrooms"</label>
                        <span class="text-gray-700 font-medium">"1 Bedroom"</span>
                    </div>
                </div>
                 <div class="flex items-center gap-3 text-[var(--accent-blue)] text-xl font-bold">
                    <button class="hover:opacity-75 transition px-1">"+"</button>
                    <button class="hover:opacity-75 transition px-1">"−"</button>
                 </div>
            </div>

            // Search Button
            <button class="bg-[var(--accent-blue)] hover:bg-[var(--accent-hover)] text-white font-bold py-4 px-8 transition shadow-md w-full md:w-auto uppercase tracking-wider text-sm rounded-none">
                "SEARCH VILLAS"
            </button>
        </div>
    }
}
