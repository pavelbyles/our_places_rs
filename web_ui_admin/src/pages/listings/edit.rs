use serde::Deserialize;
use topcoat::{
    Result,
    context::Cx,
    router::{
        content::Form,
        error::{not_found, see_other, service_unavailable},
        page, parse_query_params, path_param,
    },
    view::{View, view},
};
use uuid::Uuid;
use web_client::session::AdminRole;

use super::form::{ListingFields, listing_form, split_api_rejection};
use super::images::{listing_photos, photo_grid, suite_photo_choices};

use super::managed_listing;
use crate::support::{host_or_admin, service_token};

path_param!(id);

#[derive(Deserialize, Default)]
struct SlugSuggestionQuery {
    #[serde(default)]
    name: String,
    #[serde(default)]
    slug: Option<String>,
}

/// Stateless HTMX fragment returning an optional URL slug suggestion derived
/// from a listing's new name (in-memory only; no DB or `listing_api` calls).
#[page("/admin/listings/slug-suggestion")]
pub async fn slug_suggestion(cx: &Cx) -> Result<impl View> {
    let _user = host_or_admin(cx).await?;
    let q = parse_query_params::<SlugSuggestionQuery>(cx).unwrap_or_default();
    let current = q.slug.as_deref().unwrap_or("");
    let suggestion =
        common::slug::suggest_listing_slug(&q.name).filter(|suggested| suggested != current);
    Ok(view! {
        if let Some(suggested) = suggestion {
            <div
                data-slug-suggestion-chip=""
                class="flex flex-wrap items-center gap-2 rounded-lg border border-border bg-muted/40 px-3 py-2 text-xs text-muted-foreground"
            >
                <span>"Suggested URL:"</span>
                <code class="font-mono text-foreground">
                    (format!("/listings/{suggested}"))
                </code>
                <button
                    type="button"
                    data-apply-slug-target="listing-slug"
                    data-apply-slug-value=(suggested)
                    class="ml-auto rounded-md border border-border bg-card px-2.5 py-1 text-xs font-medium text-foreground hover:bg-muted"
                >
                    "Use suggested URL"
                </button>
            </div>
        }
    })
}

/// The stored location for the edit form, from the manager-only endpoint
/// (spec 103 §1.9). Never from the public detail, whose point may be the
/// approximate one: saving that would replace the stored point.
pub async fn edit_prefill(
    client: &web_client::TopcoatApiClient,
    listing_id: Uuid,
    jwt: &str,
) -> std::result::Result<common::location::ListingLocationResponse, web_client::client::UserCallError>
{
    client.get_listing_location_as(listing_id, jwt).await
}

/// The studio form for an existing listing.
#[page("/admin/listings/{id}/edit")]
pub async fn edit_listing_page(cx: &Cx) -> Result<impl View> {
    let user = host_or_admin(cx).await?;
    let raw = id(cx);
    Uuid::parse_str(raw).map_err(|_| not_found())?;
    let listing = managed_listing(cx, &user, raw).await?;
    let admin = user.role == AdminRole::Admin;
    let jwt = service_token(cx, &user)?;
    let location = edit_prefill(web_client::get_api_client(cx), listing.id, &jwt)
        .await
        .map_err(|e| {
            tracing::error!(listing_id = %listing.id, error = %e, "couldn't load this listing's location for editing");
            service_unavailable(5)
        })?;
    let fields = ListingFields::from_listing(&listing).with_location(&location);
    let action = format!("/admin/listings/{}/edit", listing.id);
    let presign = format!("/admin/listings/{}/images/presign", listing.id);
    let photos = listing_photos(cx, listing.id).await;
    let suite_photos = suite_photo_choices(&photos);
    let preview_href = format!("/admin/listings/{}/preview", listing.id);
    Ok(view! {
        <div class="space-y-6">
            <div class="flex flex-wrap items-end justify-between gap-4">
                <div>
                    <a href=(format!("/admin/listings/{}", listing.id)) class="text-sm">
                        "← Listing"
                    </a>
                    <h1 class="text-2xl font-semibold">
                        (format!("Edit {}", listing.name))
                    </h1>
                </div>
                <a
                    href=(preview_href)
                    class="inline-flex h-10 items-center rounded-lg border border-border px-4 text-sm font-medium no-underline text-foreground"
                >
                    "Preview"
                </a>
            </div>
            listing_form(
                action: action,
                submit_label: "Save changes".into(),
                fields: fields,
                owners: Vec::new(),
                can_set_commission: admin,
                errors: Vec::new(),
                suite_photos: suite_photos
            )
            <section
                aria-labelledby="photos-heading"
                class="space-y-4 rounded-xl border border-border p-6"
            >
                <h2 id="photos-heading" class="text-sm font-semibold">"Photos"</h2>
                photo_grid(
                    listing_id: listing.id,
                    name: listing.name.clone(),
                    photos: photos,
                    error: None
                )
                <form
                    id="image-upload-form"
                    data-upload=(presign)
                    class="flex flex-wrap items-center gap-3"
                >
                    <label for="image-upload-input" class="text-sm">
                        "Add photos (JPEG, PNG or WebP, up to 15 MB each)"
                    </label>
                    <input
                        id="image-upload-input"
                        type="file"
                        name="photos"
                        multiple=(true)
                        accept="image/jpeg,image/png,image/webp"
                        class="text-sm"
                    />
                    <button
                        id="btn-upload-images"
                        type="submit"
                        class="h-10 rounded-lg border border-border px-4 text-sm"
                    >
                        "Upload"
                    </button>
                    <p
                        data-upload-status=""
                        role="status"
                        class="w-full text-sm text-muted-foreground"
                    ></p>
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
    let user = host_or_admin(cx).await?;
    let raw = id(cx);
    Uuid::parse_str(raw).map_err(|_| not_found())?;
    let listing = managed_listing(cx, &user, raw).await?;
    let admin = user.role == AdminRole::Admin;
    let slug_changed = fields.slug != listing.slug;
    let local_slug_error = if slug_changed {
        common::slug::validate_listing_slug(&fields.slug)
            .err()
            .map(str::to_string)
    } else {
        None
    };
    let (slug_error, errors) = match (
        fields.validate(Some(listing.user_id), admin),
        local_slug_error,
    ) {
        (Ok(valid), None) => {
            let mut request = valid.into_update(listing.listing_details.as_ref());
            if slug_changed {
                request.slug = Some(fields.slug.clone());
            }
            if !admin {
                request.commission_pct = None;
            }
            let jwt = service_token(cx, &user)?;
            match common::app_client::update_listing(&listing.id.to_string(), &request, &jwt).await
            {
                Ok(_) => {
                    tracing::info!(listing_id = %listing.id, "listing updated");
                    return Err(see_other(format!("/admin/listings/{}", listing.id)).into());
                }
                Err(e) => {
                    tracing::warn!(listing_id = %listing.id, error = %e, "listing update rejected");
                    split_api_rejection(&e).unwrap_or_else(|| {
                        (
                            None,
                            vec![
                                "The listing could not be saved. Check the values and try again."
                                    .into(),
                            ],
                        )
                    })
                }
            }
        }
        (Ok(_), Some(slug_err)) => (Some(slug_err), Vec::new()),
        (Err(errors), slug_err) => (slug_err, errors),
    };
    let action = format!("/admin/listings/{}/edit", listing.id);
    let suite_photos = suite_photo_choices(&listing_photos(cx, listing.id).await);
    let preview_href = format!("/admin/listings/{}/preview", listing.id);
    Ok(view! {
        <div class="space-y-6">
            <div class="flex flex-wrap items-center justify-between gap-4">
                <h1 class="text-2xl font-semibold">
                    (format!("Edit {}", listing.name))
                </h1>
                <a
                    href=(preview_href)
                    class="inline-flex h-10 items-center rounded-lg border border-border px-4 text-sm font-medium no-underline text-foreground"
                >
                    "Preview"
                </a>
            </div>
            listing_form(
                action: action,
                submit_label: "Save changes".into(),
                fields: fields,
                owners: Vec::new(),
                can_set_commission: admin,
                errors: errors,
                suite_photos: suite_photos,
                slug_error: slug_error
            )
        </div>
    })
}
