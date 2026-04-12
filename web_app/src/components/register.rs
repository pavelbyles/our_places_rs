use crate::auth::Register;
use leptos::prelude::*;

#[component]
pub fn RegisterPage() -> impl IntoView {
    let register_action = ServerAction::<Register>::new();

    view! {
        <div class="min-h-screen flex items-center justify-center bg-base-200">
            <div class="card w-full max-w-md bg-base-100 shadow-xl overflow-hidden">
                <div class="card-body p-0">
                    <div class="bg-secondary text-secondary-content p-8 text-center">
                        <h2 class="text-3xl font-bold">"Create Account"</h2>
                        <p class="mt-2 opacity-90">"Join Our Places today"</p>
                    </div>
                    
                    <div class="p-8">
                        // Social Shortcuts
                        <div class="grid grid-cols-2 gap-3">
                            <button class="btn btn-outline gap-2">
                                <svg class="w-5 h-5" viewBox="0 0 24 24">
                                    <path fill="currentColor" d="M22.56 12.25c0-.78-.07-1.53-.2-2.25H12v4.26h5.92c-.26 1.37-1.04 2.53-2.21 3.31v2.77h3.57c2.08-1.92 3.28-4.74 3.28-8.09z" />
                                    <path fill="currentColor" d="M12 23c2.97 0 5.46-.98 7.28-2.66l-3.57-2.77c-.98.66-2.23 1.06-3.71 1.06-2.86 0-5.29-1.93-6.16-4.53H2.18v2.84C3.99 20.53 7.7 23 12 23z" />
                                    <path fill="currentColor" d="M5.84 14.09c-.22-.66-.35-1.36-.35-2.09s.13-1.43.35-2.09V7.07H2.18C1.43 8.55 1 10.22 1 12s.43 3.45 1.18 4.93l3.66-2.84z" />
                                    <path fill="currentColor" d="M12 5.38c1.62 0 3.06.56 4.21 1.64l3.15-3.15C17.45 2.09 14.97 1 12 1 7.7 1 3.99 3.47 2.18 7.07l3.66 2.84c.87-2.6 3.3-4.53 6.16-4.53z" />
                                </svg>
                                "Google"
                            </button>
                            <button class="btn btn-outline gap-2">
                                <svg class="w-5 h-5" fill="currentColor" viewBox="0 0 24 24">
                                    <path d="M24 12.073c0-6.627-5.373-12-12-12s-12 5.373-12 12c0 5.99 4.388 10.954 10.125 11.854v-8.385H7.078v-3.47h3.047V9.43c0-3.007 1.792-4.669 4.533-4.669 1.312 0 2.686.235 2.686.235v2.953H15.83c-1.491 0-1.956.925-1.956 1.874v2.25h3.328l-.532 3.47h-2.796v8.385C19.612 23.027 24 18.062 24 12.073z" />
                                </svg>
                                "Facebook"
                            </button>
                        </div>

                        <div class="divider my-6">"OR"</div>

                        // Registration Form
                        <ActionForm action=register_action>
                            <div class="grid grid-cols-2 gap-4">
                                <div class="form-control w-full">
                                    <label class="label"><span class="label-text">"First Name"</span></label>
                                    <input type="text" name="first_name" placeholder="John" class="input input-bordered w-full" required />
                                </div>
                                <div class="form-control w-full">
                                    <label class="label"><span class="label-text">"Last Name"</span></label>
                                    <input type="text" name="last_name" placeholder="Doe" class="input input-bordered w-full" required />
                                </div>
                            </div>
                            
                            <div class="form-control w-full mt-4">
                                <label class="label"><span class="label-text">"Email Address"</span></label>
                                <input type="email" name="email" placeholder="you@example.com" class="input input-bordered w-full" required />
                            </div>

                            <div class="form-control w-full mt-4">
                                <label class="label"><span class="label-text">"Phone Number"</span></label>
                                <input type="tel" name="phone_number" placeholder="+1 (555) 000-0000" class="input input-bordered w-full" />
                            </div>
                            
                            <div class="collapse collapse-arrow bg-base-200 mt-6 overflow-visible opacity-50">
                                <input type="checkbox" disabled /> 
                                <div class="collapse-title text-sm font-medium">
                                    "Set up Passwordless (Coming Soon)"
                                </div>
                            </div>

                            <div class="divider my-4">"OR"</div>

                            <div class="form-control w-full">
                                <label class="label"><span class="label-text">"Password"</span></label>
                                <input type="password" name="password" placeholder="••••••••" class="input input-bordered w-full" required />
                            </div>

                            <button type="submit" class="btn btn-primary w-full mt-8" disabled=move || register_action.pending().get()>
                                {move || if register_action.pending().get() { "Creating Account..." } else { "Create Account" }}
                            </button>

                            {move || register_action.value().get().map(|res| match res {
                                Err(e) => view! {
                                    <div class="alert alert-error mt-4 shadow-md text-sm">
                                        <span>{e.to_string()}</span>
                                    </div>
                                }.into_any(),
                                Ok(_) => view! {
                                    <div class="alert alert-success mt-4 shadow-md text-sm">
                                        <span>"Registration successful! Redirecting to verification..."</span>
                                    </div>
                                }.into_any(),
                            })}
                        </ActionForm>

                        <div class="text-center mt-6">
                            <span class="opacity-70">"Already have an account?"</span>
                            <a href="/login" class="link link-secondary ml-2 font-semibold">"Login"</a>
                        </div>
                    </div>
                </div>
            </div>
        </div>
    }
}
