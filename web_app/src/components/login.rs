use crate::auth::LoginTraditional;
use leptos::prelude::*;

#[component]
pub fn LoginPage() -> impl IntoView {
    let login_action = ServerAction::<LoginTraditional>::new();
    let auth = use_context::<crate::app::AuthContext>().expect("AuthContext should be provided");

    Effect::new(move || {
        if let Some(Ok(_)) = login_action.value().get() {
            auth.refresh();
        }
    });

    view! {
        <div class="min-h-screen flex items-center justify-center bg-base-200">
            <div class="card w-full max-w-md bg-base-100 shadow-xl overflow-hidden">
                <div class="card-body p-0">
                    <div class="bg-primary text-primary-content p-8 text-center">
                        <h2 class="text-3xl font-bold">"Welcome Back"</h2>
                        <p class="mt-2 opacity-90">"Log in to your account"</p>
                    </div>
                    
                    <div class="p-8">
                        // Social Login
                        <div class="flex flex-col gap-3">
                            <button class="btn btn-outline gap-2">
                                <svg class="w-5 h-5" viewBox="0 0 24 24">
                                    <path fill="currentColor" d="M22.56 12.25c0-.78-.07-1.53-.2-2.25H12v4.26h5.92c-.26 1.37-1.04 2.53-2.21 3.31v2.77h3.57c2.08-1.92 3.28-4.74 3.28-8.09z" />
                                    <path fill="currentColor" d="M12 23c2.97 0 5.46-.98 7.28-2.66l-3.57-2.77c-.98.66-2.23 1.06-3.71 1.06-2.86 0-5.29-1.93-6.16-4.53H2.18v2.84C3.99 20.53 7.7 23 12 23z" />
                                    <path fill="currentColor" d="M5.84 14.09c-.22-.66-.35-1.36-.35-2.09s.13-1.43.35-2.09V7.07H2.18C1.43 8.55 1 10.22 1 12s.43 3.45 1.18 4.93l3.66-2.84z" />
                                    <path fill="currentColor" d="M12 5.38c1.62 0 3.06.56 4.21 1.64l3.15-3.15C17.45 2.09 14.97 1 12 1 7.7 1 3.99 3.47 2.18 7.07l3.66 2.84c.87-2.6 3.3-4.53 6.16-4.53z" />
                                </svg>
                                "Continue with Google"
                            </button>
                            <button class="btn btn-outline gap-2">
                                <svg class="w-5 h-5" fill="currentColor" viewBox="0 0 24 24">
                                    <path d="M24 12.073c0-6.627-5.373-12-12-12s-12 5.373-12 12c0 5.99 4.388 10.954 10.125 11.854v-8.385H7.078v-3.47h3.047V9.43c0-3.007 1.792-4.669 4.533-4.669 1.312 0 2.686.235 2.686.235v2.953H15.83c-1.491 0-1.956.925-1.956 1.874v2.25h3.328l-.532 3.47h-2.796v8.385C19.612 23.027 24 18.062 24 12.073z" />
                                </svg>
                                "Continue with Facebook"
                            </button>
                        </div>

                        <div class="divider my-8">"OR"</div>

                        // Tabs for Login Methods
                        <div role="tablist" class="tabs tabs-bordered mb-6">
                            <input type="radio" name="login_tabs" role="tab" class="tab" aria-label="Traditional" checked="checked" />
                            <div role="tabpanel" class="tab-content pt-6">
                                <ActionForm action=login_action>
                                    <div class="form-control w-full">
                                        <label class="label"><span class="label-text">"Email Address"</span></label>
                                        <input type="email" name="email" placeholder="you@example.com" class="input input-bordered w-full" required />
                                    </div>
                                    <div class="form-control w-full mt-4">
                                        <label class="label"><span class="label-text">"Password"</span></label>
                                        <input type="password" name="password" placeholder="••••••••" class="input input-bordered w-full" required />
                                    </div>
                                    <button type="submit" class="btn btn-primary w-full mt-6" disabled=move || login_action.pending().get()>
                                        {move || if login_action.pending().get() { "Logging In..." } else { "Login" }}
                                    </button>
                                </ActionForm>
                            </div>

                            <input type="radio" name="login_tabs" role="tab" class="tab" aria-label="Passwordless" />
                            <div role="tabpanel" class="tab-content pt-6">
                                <div class="form-control w-full">
                                    <label class="label"><span class="label-text">"Email Address"</span></label>
                                    <input type="email" placeholder="you@example.com" class="input input-bordered w-full" />
                                </div>
                                <div class="form-control w-full mt-4">
                                    <label class="label"><span class="label-text">"Auth Code (TOTP)"</span></label>
                                    <input type="text" placeholder="123 456" class="input input-bordered w-full text-center tracking-[0.5em] font-mono" />
                                </div>
                                <button class="btn btn-secondary w-full mt-6">"Log In with Code"</button>
                                <p class="text-xs text-center mt-4 opacity-60">
                                    "Use the code from your Google Authenticator app."
                                </p>
                            </div>
                        </div>

                        {move || login_action.value().get().map(|res| match res {
                            Err(e) => {
                                let error_msg = e.to_string();
                                let is_unverified = error_msg.contains("Account not verified");
                                
                                view! {
                                    <div class="alert alert-error mt-4 shadow-md text-sm flex flex-col items-start gap-2">
                                        <div class="flex items-center gap-2">
                                            <svg xmlns="http://www.w3.org/2000/svg" class="stroke-current shrink-0 h-6 w-6" fill="none" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M10 14l2-2m0 0l2-2m-2 2l-2-2m2 2l2 2m7-2a9 9 0 11-18 0 9 9 0 0118 0z" /></svg>
                                            <span>{error_msg}</span>
                                        </div>
                                        {if is_unverified {
                                            view! {
                                                <a href="/verify" class="btn btn-xs btn-outline btn-ghost border-white/30 text-white hover:bg-white/10">"Verify Account Now"</a>
                                            }.into_any()
                                        } else {
                                            view! { <div></div> }.into_any()
                                        }}
                                    </div>
                                }.into_any()
                            },
                            Ok(_) => view! {
                                <div class="alert alert-success mt-4 shadow-md text-sm">
                                    <span>"Login successful! Redirecting..."</span>
                                </div>
                            }.into_any(),
                        })}

                        <div class="text-center mt-6">
                            <span class="opacity-70">"Don't have an account?"</span>
                            <a href="/register" class="link link-primary ml-2 font-semibold">"Sign Up"</a>
                        </div>
                    </div>
                </div>
            </div>
        </div>
    }
}
