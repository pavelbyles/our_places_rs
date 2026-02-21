use leptos::prelude::*;
use leptos_router::hooks::use_location;
use leptos_router::hooks::use_navigate;

#[component]
pub fn Layout(children: Children) -> impl IntoView {
    let location = use_location();
    let navigate = use_navigate();
    let navigate_inner = navigate.clone();
    let navigate_fallback = navigate.clone();

    let auth_status = Resource::new(
        move || location.pathname.get(),
        |_| async move { crate::auth::get_current_user().await },
    );

    view! {
        <div class="drawer lg:drawer-open" >
            <input id="my-drawer-4" type="checkbox" class="drawer-toggle" />
            <div class="drawer-content">
                // Navbar
                <nav class="navbar w-full bg-base-300">
                    <label for="my-drawer-4" aria-label="open sidebar" class="btn btn-square btn-ghost">
                        <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" strokeLinejoin="round" strokeLinecap="round" strokeWidth="2" fill="none" stroke="currentColor" className="my-1.5 inline-block size-4"><path d="M4 4m0 2a2 2 0 0 1 2 -2h12a2 2 0 0 1 2 2v12a2 2 0 0 1 -2 2h-12a2 2 0 0 1 -2 -2z"></path><path d="M9 4v16"></path><path d="M14 10l2 2l-2 2"></path></svg>
                    </label>
                    <div class="px-4">Admin</div>
                </nav>
                <div class="p-4">
                {children()}
                </div>
            </div>

            <div class="drawer-side is-drawer-close:overflow-visible">
                <label for="my-drawer-4" aria-label="close sidebar" class="drawer-overlay"></label>
                <div class="flex min-h-full flex-col items-start bg-base-200 is-drawer-close:w-14 is-drawer-open:w-64">
                    // Sidebar content here
                    <ul class="menu w-full grow">
                        // List item
                        <li>
                            <button on:click={ let navigate = navigate.clone(); move |_| { navigate("/home", Default::default()); } } class="is-drawer-close:tooltip is-drawer-close:tooltip-right" data-tip="Homepage">
                                <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" stroke-linejoin="round" stroke-linecap="round" stroke-width="2" fill="none" stroke="currentColor" class="my-1.5 inline-block size-4"><path d="M15 21v-8a1 1 0 0 0-1-1h-4a1 1 0 0 0-1 1v8"></path><path d="M3 10a2 2 0 0 1 .709-1.528l7-5.999a2 2 0 0 1 2.582 0l7 5.999A2 2 0 0 1 21 10v9a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z"></path></svg>
                                <span class="is-drawer-close:hidden">Homepage</span>
                            </button>
                        </li>
                        <Transition
                            fallback=move || view! {
                                <li>
                                    <button on:click={ let navigate = navigate_fallback.clone(); move |_| { navigate("/login", Default::default()); } } class="is-drawer-close:tooltip is-drawer-close:tooltip-right" data-tip="Login">
                                        <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" stroke-linejoin="round" stroke-linecap="round" stroke-width="2" fill="none" stroke="currentColor" class="my-1.5 inline-block size-4"><path stroke-linecap="round" stroke-linejoin="round" d="M15.75 9V5.25A2.25 2.25 0 0 0 13.5 3h-6a2.25 2.25 0 0 0-2.25 2.25v13.5A2.25 2.25 0 0 0 7.5 21h6a2.25 2.25 0 0 0 2.25-2.25V15M12 9l-3 3m0 0 3 3m-3-3h12.75" /></svg>
                                        <span class="is-drawer-close:hidden">Login</span>
                                    </button>
                                </li>
                            }
                        >
                            { move || {
                                let navigate = navigate_inner.clone();
                                auth_status.get().map(move |status| {
                                    match status {
                                        Ok(Some(name)) => {
                                            let logout_action = ServerAction::<crate::auth::Logout>::new();
                                            view! {
                                                <li>
                                                    <button on:click={ let navigate = navigate.clone(); move |_| { navigate("/admin", Default::default()); } } class="is-drawer-close:tooltip is-drawer-close:tooltip-right" data-tip="Admin">
                                                        <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" stroke-linejoin="round" stroke-linecap="round" stroke-width="2" fill="none" stroke="currentColor" class="my-1.5 inline-block size-4"><path stroke-linecap="round" stroke-linejoin="round" d="M9 17.25v1.007a3 3 0 0 1-.879 2.122L7.5 21h9l-.621-.621A3 3 0 0 1 15 18.257V17.25m6-12V15a2.25 2.25 0 0 1-2.25 2.25H5.25A2.25 2.25 0 0 1 3 15V5.25m18 0A2.25 2.25 0 0 0 18.75 3H5.25A2.25 2.25 0 0 0 3 5.25m18 0V12a2.25 2.25 0 0 1-2.25 2.25H5.25A2.25 2.25 0 0 1 3 12V5.25" /></svg>
                                                        <span class="is-drawer-close:hidden">Admin</span>
                                                    </button>
                                                </li>
                                                <li>
                                                    <button on:click={ let navigate = navigate.clone(); move |_| { navigate("/admin/users", Default::default()); } } class="is-drawer-close:tooltip is-drawer-close:tooltip-right" data-tip="Users">
                                                        <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" stroke-linejoin="round" stroke-linecap="round" stroke-width="2" fill="none" stroke="currentColor" class="my-1.5 inline-block size-4"><path stroke-linecap="round" stroke-linejoin="round" d="M15.75 6a3.75 3.75 0 1 1-7.5 0 3.75 3.75 0 0 1 7.5 0ZM4.501 20.118a7.5 7.5 0 0 1 14.998 0A17.933 17.933 0 0 1 12 21.75c-2.676 0-5.216-.584-7.499-1.632Z" /></svg>
                                                        <span class="is-drawer-close:hidden">Users</span>
                                                    </button>
                                                </li>
                                                <li>
                                                    <button on:click={ let navigate = navigate.clone(); move |_| { navigate("/admin/listings", Default::default()); } } class="is-drawer-close:tooltip is-drawer-close:tooltip-right" data-tip="Listings">
                                                        <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" stroke-linejoin="round" stroke-linecap="round" stroke-width="2" fill="none" stroke="currentColor" class="my-1.5 inline-block size-4"><path stroke-linecap="round" stroke-linejoin="round" d="M8.25 21v-4.875c0-.621.504-1.125 1.125-1.125h2.25c.621 0 1.125.504 1.125 1.125V21m0 0h4.5V3.545M12.75 21h7.5V10.75M2.25 21h1.5m18 0h-18M2.25 9l4.5-1.636M18.75 3l-1.5.545m0 6.205 3 1m1.5.5-1.5-.5M6.75 7.364V3h-3v18m3-13.636 10.5-3.819" /></svg>
                                                        <span class="is-drawer-close:hidden">Listings</span>
                                                    </button>
                                                </li>
                                                <li>
                                                    <button on:click=move |_| { logout_action.dispatch(crate::auth::Logout {}); } class="is-drawer-close:tooltip is-drawer-close:tooltip-right w-full text-start" data-tip="Logout">
                                                        <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" stroke-linejoin="round" stroke-linecap="round" stroke-width="2" fill="none" stroke="currentColor" class="my-1.5 inline-block size-4"><path stroke-linecap="round" stroke-linejoin="round" d="M8.25 9V5.25A2.25 2.25 0 0 1 10.5 3h6a2.25 2.25 0 0 1 2.25 2.25v13.5A2.25 2.25 0 0 1 16.5 21h-6a2.25 2.25 0 0 1-2.25-2.25V15m-3 0-3-3m0 0 3-3m-3 3H15" /></svg>
                                                        <span class="is-drawer-close:hidden">{format!("Logout ({})", name)}</span>
                                                    </button>
                                                </li>
                                            }.into_any()
                                        },
                                        _ => view! {
                                            <li>
                                                <button on:click=move |_| { navigate("/login", Default::default()); } class="is-drawer-close:tooltip is-drawer-close:tooltip-right" data-tip="Login">
                                                    <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" stroke-linejoin="round" stroke-linecap="round" stroke-width="2" fill="none" stroke="currentColor" class="my-1.5 inline-block size-4"><path stroke-linecap="round" stroke-linejoin="round" d="M15.75 9V5.25A2.25 2.25 0 0 0 13.5 3h-6a2.25 2.25 0 0 0-2.25 2.25v13.5A2.25 2.25 0 0 0 7.5 21h6a2.25 2.25 0 0 0 2.25-2.25V15M12 9l-3 3m0 0 3 3m-3-3h12.75" /></svg>
                                                    <span class="is-drawer-close:hidden">Login</span>
                                                </button>
                                            </li>
                                        }.into_any(),
                                    }
                                })
                            }}
                        </Transition>
                    </ul>
                </div>
            </div>
        </div>

    }
}
