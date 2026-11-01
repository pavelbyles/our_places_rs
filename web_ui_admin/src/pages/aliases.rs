//! Legacy unprefixed console paths, redirected permanently (308 keeps the
//! method, so legacy POSTs still land) to their `/admin/...` pages.

use topcoat::{
    Result,
    context::Cx,
    router::{error::redirect_permanent, page, path_param},
};

path_param!(id);

fn to(target: String) -> Result<()> {
    Err(redirect_permanent(target).into())
}

/// `/listings` → `/admin/listings`.
#[page("/listings")]
pub async fn listings_alias(cx: &Cx) -> Result<()> {
    let _ = cx;
    to("/admin/listings".into())
}

/// `/listings/{id}` → `/admin/listings/{id}`.
#[page("/listings/{id}")]
pub async fn listing_alias(cx: &Cx) -> Result<()> {
    to(format!("/admin/listings/{}", path_param::<Id>(cx)))
}

/// `/listings/{id}/edit` → `/admin/listings/{id}/edit`.
#[page("/listings/{id}/edit")]
pub async fn listing_edit_alias(cx: &Cx) -> Result<()> {
    to(format!("/admin/listings/{}/edit", path_param::<Id>(cx)))
}

/// `/listings/{id}/pricing` → `/admin/listings/{id}/pricing`.
#[page("/listings/{id}/pricing")]
pub async fn pricing_alias(cx: &Cx) -> Result<()> {
    to(format!("/admin/listings/{}/pricing", path_param::<Id>(cx)))
}

/// `POST /listings/{id}/pricing/add` → `/admin/listings/{id}/pricing/add`.
#[page(POST "/listings/{id}/pricing/add")]
pub async fn pricing_add_alias(cx: &Cx) -> Result<()> {
    to(format!(
        "/admin/listings/{}/pricing/add",
        path_param::<Id>(cx)
    ))
}

/// `/bookings` → `/admin/bookings`.
#[page("/bookings")]
pub async fn bookings_alias(cx: &Cx) -> Result<()> {
    let _ = cx;
    to("/admin/bookings".into())
}

/// `/payouts` → `/admin/payouts`.
#[page("/payouts")]
pub async fn payouts_alias(cx: &Cx) -> Result<()> {
    let _ = cx;
    to("/admin/payouts".into())
}

/// `/users` → `/admin/users`.
#[page("/users")]
pub async fn users_alias(cx: &Cx) -> Result<()> {
    let _ = cx;
    to("/admin/users".into())
}
