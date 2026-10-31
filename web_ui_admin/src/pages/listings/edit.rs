use topcoat::{
    Result,
    context::Cx,
    router::{
        content::Form,
        error::{not_found, see_other},
        page, path_param,
    },
    view::{View, view},
};
use uuid::Uuid;
use web_client::session::AdminRole;

use super::form::{ListingFields, api_rejection_messages, listing_form};
use super::images::{listing_photos, photo_grid};
use super::managed_listing;
use crate::support::staff;

path_param!(id);

/// The studio form for an existing listing.
#[page("/admin/listings/{id}/edit")]
pub async fn edit_listing_page(cx: &Cx) -> Result<impl View> {
    let user = staff(cx).await?;
    let raw = path_param::<Id>(cx);
    Uuid::parse_str(raw).map_err(|_| not_found())?;
    let listing = managed_listing(cx, &user, raw).await?;
    let admin = user.role == AdminRole::Admin;
    let fields = ListingFields::from_listing(&listing);
    let action = format!("/admin/listings/{}/edit", listing.id);
    let presign = format!("/admin/listings/{}/images/presign", listing.id);
    let photos = listing_photos(cx, listing.id).await;
    Ok(view! {
        <div class="space-y-6">
            <div>
                <a href=(format!("/admin/listings/{}", listing.id)) class="text-sm">"← Listing"</a>
                <h1 class="text-2xl font-semibold">(format!("Edit {}", listing.name))</h1>
            </div>
            listing_form(action: action, submit_label: "Save changes".into(), fields: fields, owners: Vec::new(), can_set_commission: admin, errors: Vec::new())
            <section aria-labelledby="photos-heading" class="space-y-4 rounded-xl border border-border p-6">
                <h2 id="photos-heading" class="text-sm font-semibold">"Photos"</h2>
                photo_grid(listing_id: listing.id, name: listing.name.clone(), photos: photos, error: None)
                <form id="image-upload-form" data-upload=(presign) class="flex flex-wrap items-center gap-3">
                    <label for="image-upload-input" class="text-sm">"Add photos (JPEG, PNG or WebP, up to 15 MB each)"</label>
                    <input id="image-upload-input" type="file" name="photos" multiple=(true) accept="image/jpeg,image/png,image/webp" class="text-sm" />
                    <button id="btn-upload-images" type="submit" class="h-10 rounded-lg border border-border px-4 text-sm">"Upload"</button>
                    <p data-upload-status="" role="status" class="w-full text-sm text-muted-foreground"></p>
                </form>
            </section>
        </div>
    })
}

/// Saves a listing the user may manage. The owner never changes here, and
/// only admins may change the commission.
#[page(POST "/admin/listings/{id}/edit")]
pub async fn update_listing(
    cx: &Cx,
    /// The studio form.
    Form(fields): Form<ListingFields>,
) -> Result<impl View> {
    let user = staff(cx).await?;
    let raw = path_param::<Id>(cx);
    Uuid::parse_str(raw).map_err(|_| not_found())?;
    let listing = managed_listing(cx, &user, raw).await?;
    let admin = user.role == AdminRole::Admin;
    let errors = match fields.validate(Some(listing.user_id), admin) {
        Ok(valid) => {
            let mut request = valid.into_update(listing.listing_details.as_ref());
            if !admin {
                request.commission_pct = None;
            }
            match common::app_client::update_listing(&listing.id.to_string(), &request).await {
                Ok(_) => {
                    tracing::info!(listing_id = %listing.id, "listing updated");
                    return Err(see_other(format!("/admin/listings/{}", listing.id)).into());
                }
                Err(e) => {
                    tracing::warn!(listing_id = %listing.id, error = %e, "listing update rejected");
                    api_rejection_messages(&e).unwrap_or_else(|| {
                        vec![
                            "The listing could not be saved. Check the values and try again."
                                .into(),
                        ]
                    })
                }
            }
        }
        Err(errors) => errors,
    };
    let action = format!("/admin/listings/{}/edit", listing.id);
    Ok(view! {
        <div class="space-y-6">
            <h1 class="text-2xl font-semibold">(format!("Edit {}", listing.name))</h1>
            listing_form(action: action, submit_label: "Save changes".into(), fields: fields, owners: Vec::new(), can_set_commission: admin, errors: errors)
        </div>
    })
}
