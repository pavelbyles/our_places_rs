use crate::auth::Login;
use leptos::form::ActionForm;
use leptos::prelude::*;

#[component]
#[allow(non_snake_case)]
pub fn LoginPage() -> impl IntoView {
    let login_action = ServerAction::<Login>::new();
    let value = login_action.value();

    view! {
        <div class="hero min-h-screen bg-base-200">
            <div class="hero-content flex-col lg:flex-row-reverse">
                <div class="text-center lg:text-left">
                    <h1 class="text-5xl font-bold">"Login"</h1>
                    <p class="py-6">
                        "Access the admin dashboard to manage listings, bookings, and users."
                    </p>
                </div>
                <div class="card shrink-0 w-full max-w-sm shadow-2xl bg-base-100">
                    <ActionForm action=login_action attr:class="card-body">
                        <div class="form-control">
                            <label class="input validator">
                                <svg class="h-[1em] opacity-50" xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24">
                                    <g
                                        stroke-linejoin="round"
                                        stroke-linecap="round"
                                        stroke-width="2.5"
                                        fill="none"
                                        stroke="currentColor"
                                    >
                                        <rect width="20" height="16" x="2" y="4" rx="2"></rect>
                                        <path d="m22 7-8.97 5.7a1.94 1.94 0 0 1-2.06 0L2 7"></path>
                                    </g>
                                </svg>
                                <input type="email" name="email" placeholder="user@domain.com" required />
                            </label>
                            <div class="validator-hint hidden">"Enter valid email address"</div>
                        </div>
                        <div class="form-control">
                            <label class="label">
                                <span class="label-text">"Password"</span>
                            </label>
                            <input type="password" name="password" placeholder="password" class="input input-bordered" required />
                            <label class="label">
                                <a href="#" class="label-text-alt link link-hover">"Forgot password?"</a>
                            </label>
                        </div>
                        <div class="form-control mt-6">
                            <button type="submit" class="btn btn-primary" disabled=move || login_action.pending().get()>"Login"</button>
                        </div>

                        {move || value.get().map(|v: Result<(), ServerFnError>| match v {
                            Err(e) => view! { <div class="alert alert-error mt-4"><span>{e.to_string()}</span></div> }.into_any(),
                            Ok(_) => ().into_any()
                        })}

                        <div class="divider">"OR"</div>
                         <div class="text-center">
                            <a href="/home" class="link link-hover">"Back to Home"</a>
                        </div>
                    </ActionForm>
                </div>
            </div>
        </div>
    }
}
