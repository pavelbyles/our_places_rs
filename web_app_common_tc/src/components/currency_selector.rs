use topcoat::{
    Result,
    view::{component, view},
};

#[component]
pub async fn currency_selector(current_currency: Option<String>) -> Result {
    let active = current_currency
        .unwrap_or_else(|| "USD".to_string())
        .to_uppercase();

    view! {
        <div class="dropdown dropdown-end">
            <div tabindex="0" role="button" class="btn btn-ghost btn-sm font-semibold gap-1">
                <svg xmlns="http://www.w3.org/2000/svg" class="h-4 w-4 text-base-content/70" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                    <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 8c-1.657 0-3 .895-3 2s1.343 2 3 2 3 .895 3 2-1.343 2-3 2m0-8c1.11 0 2.08.402 2.599 1M12 8V7m0 1v8m0 0v1m0-1c-1.11 0-2.08-.402-2.599-1M21 12a9 9 0 11-18 0 9 9 0 0118 0z" />
                </svg>
                <span>(active.clone())</span>
                <svg xmlns="http://www.w3.org/2000/svg" class="h-3 w-3 opacity-60" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                    <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 9l-7 7-7-7" />
                </svg>
            </div>
            <ul tabindex="0" class="dropdown-content menu p-2 shadow-lg bg-base-100 rounded-box w-32 border border-base-200 z-50">
                for curr in ["USD", "JMD", "EUR", "GBP", "CAD"] {
                    <li>
                        <a
                            href=(format!("?currency={}", curr))
                            class=(if curr == active { "active font-bold" } else { "" })
                        >
                            (curr)
                        </a>
                    </li>
                }
            </ul>
        </div>
    }
}
