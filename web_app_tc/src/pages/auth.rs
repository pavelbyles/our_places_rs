use topcoat::{
    Result,
    context::Cx,
    router::page,
    view::view,
};

#[page("/login")]
pub async fn login_page(_cx: &Cx) -> Result {
    view! {
        <div class="min-h-[70vh] flex items-center justify-center py-10 px-4">
            <div class="card w-full max-w-md bg-base-100 shadow-xl border border-base-200 overflow-hidden rounded-3xl">
                <div class="bg-primary text-primary-content p-8 text-center">
                    <h2 class="text-3xl font-extrabold">"Welcome Back"</h2>
                    <p class="mt-2 text-sm opacity-90">"Log in to manage your bookings and saved stays"</p>
                </div>

                <div class="p-8 space-y-6">
                    <form action="/profile" method="GET" class="space-y-4">
                        <div>
                            <label class="label text-xs font-bold uppercase text-base-content/70">"Email Address"</label>
                            <input type="email" name="email" placeholder="you@example.com" class="input input-bordered w-full" required=(true) />
                        </div>

                        <div>
                            <label class="label text-xs font-bold uppercase text-base-content/70">"Password"</label>
                            <input type="password" name="password" placeholder="••••••••" class="input input-bordered w-full" required=(true) />
                        </div>

                        <button type="submit" class="btn btn-primary w-full mt-4">
                            "Log In"
                        </button>
                    </form>

                    <div class="divider my-4">"OR"</div>

                    <a href="/verify" class="btn btn-outline w-full">
                        "Log In with Email Code (Passwordless)"
                    </a>

                    <div class="text-center text-sm pt-4">
                        <span class="text-base-content/70">"Don't have an account?"</span>
                        <a href="/register" class="link link-primary ml-2 font-semibold">"Sign Up"</a>
                    </div>
                </div>
            </div>
        </div>
    }
}

#[page("/register")]
pub async fn register_page(_cx: &Cx) -> Result {
    view! {
        <div class="min-h-[70vh] flex items-center justify-center py-10 px-4">
            <div class="card w-full max-w-md bg-base-100 shadow-xl border border-base-200 overflow-hidden rounded-3xl">
                <div class="bg-primary text-primary-content p-8 text-center">
                    <h2 class="text-3xl font-extrabold">"Create Account"</h2>
                    <p class="mt-2 text-sm opacity-90">"Join Our Places to unlock exclusive villa rates"</p>
                </div>

                <div class="p-8 space-y-6">
                    <form action="/verify" method="GET" class="space-y-4">
                        <div class="grid grid-cols-2 gap-4">
                            <div>
                                <label class="label text-xs font-bold uppercase text-base-content/70">"First Name"</label>
                                <input type="text" name="first_name" placeholder="John" class="input input-bordered w-full" required=(true) />
                            </div>
                            <div>
                                <label class="label text-xs font-bold uppercase text-base-content/70">"Last Name"</label>
                                <input type="text" name="last_name" placeholder="Doe" class="input input-bordered w-full" required=(true) />
                            </div>
                        </div>

                        <div>
                            <label class="label text-xs font-bold uppercase text-base-content/70">"Email Address"</label>
                            <input type="email" name="email" placeholder="you@example.com" class="input input-bordered w-full" required=(true) />
                        </div>

                        <div>
                            <label class="label text-xs font-bold uppercase text-base-content/70">"Password"</label>
                            <input type="password" name="password" placeholder="••••••••" class="input input-bordered w-full" required=(true) />
                        </div>

                        <button type="submit" class="btn btn-primary w-full mt-4">
                            "Create Account"
                        </button>
                    </form>

                    <div class="text-center text-sm pt-4">
                        <span class="text-base-content/70">"Already have an account?"</span>
                        <a href="/login" class="link link-primary ml-2 font-semibold">"Log In"</a>
                    </div>
                </div>
            </div>
        </div>
    }
}

#[page("/verify")]
pub async fn verify_page(_cx: &Cx) -> Result {
    view! {
        <div class="min-h-[70vh] flex items-center justify-center py-10 px-4">
            <div class="card w-full max-w-md bg-base-100 shadow-xl border border-base-200 overflow-hidden rounded-3xl">
                <div class="bg-primary text-primary-content p-8 text-center">
                    <h2 class="text-3xl font-extrabold">"Verify Email"</h2>
                    <p class="mt-2 text-sm opacity-90">"Enter the 6-digit verification code sent to your email"</p>
                </div>

                <div class="p-8 space-y-6">
                    <form action="/profile" method="GET" class="space-y-4">
                        <div>
                            <label class="label text-xs font-bold uppercase text-center w-full block text-base-content/70">"6-Digit Code"</label>
                            <input
                                type="text"
                                name="code"
                                placeholder="123456"
                                maxlength="6"
                                class="input input-bordered w-full text-center text-2xl tracking-[0.5em] font-mono font-bold"
                                required=(true)
                            />
                        </div>

                        <button type="submit" class="btn btn-primary w-full mt-4">
                            "Verify & Continue"
                        </button>
                    </form>

                    <div class="text-center text-xs text-base-content/60 pt-2">
                        <span>"Didn't receive code? "</span>
                        <a href="#" class="link link-primary">"Resend Code"</a>
                    </div>
                </div>
            </div>
        </div>
    }
}
