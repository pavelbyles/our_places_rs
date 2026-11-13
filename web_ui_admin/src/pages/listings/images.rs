//! Listing photo uploads: the browser asks for signed URLs here and then
//! uploads straight to Cloud Storage (no image bytes pass through the
//! portal). `image_worker` processes the uploads afterwards. Photos can be
//! removed from the grid on the edit page.

use common::listing_details::{
    MAX_PHOTO_NAME_CHARS, default_photo_label, photo_label, validate_photo_name,
};
use common::models::{
    ImagePresignRequest, ImagePresignResponse, ListingImageResponse, PendingImageMetadata,
};
use serde::Deserialize;
use topcoat::{
    Result,
    context::Cx,
    router::{
        content::{Form, Json},
        error::{not_found, see_other},
        page, path_param, route,
    },
    view::{View, component, view},
};
use uuid::Uuid;
use web_client::get_api_client;
use web_ui_common::components::domain::responsive_image::responsive_image;
use web_ui_common::components::toast::{ToastVariant, toast};

use super::managed_listing;
use crate::support::{service_token, staff};

path_param!(id);

/// Most photos accepted per upload.
pub const MAX_FILES: usize = 10;
/// Largest photo accepted, in bytes (15 MiB).
pub const MAX_BYTES: u64 = 15 * 1024 * 1024;
/// Accepted photo types.
pub const IMAGE_TYPES: [&str; 3] = ["image/jpeg", "image/png", "image/webp"];

/// One file the browser wants to upload.
#[derive(Deserialize)]
pub struct UploadFile {
    /// MIME type reported by the browser.
    pub content_type: String,
    /// Size in bytes.
    pub size_bytes: u64,
}

/// The browser's upload request.
#[derive(Deserialize)]
pub struct UploadRequest {
    /// Files to upload, in display order.
    pub files: Vec<UploadFile>,
}

/// Checks an upload request; `Err` carries the reason.
pub fn check_files(files: &[UploadFile]) -> std::result::Result<(), String> {
    if files.is_empty() || files.len() > MAX_FILES {
        return Err(format!("Choose between 1 and {MAX_FILES} photos."));
    }
    files.iter().try_for_each(|f| {
        if !IMAGE_TYPES.contains(&f.content_type.as_str()) {
            Err("Photos must be JPEG, PNG or WebP.".to_string())
        } else if f.size_bytes == 0 || f.size_bytes > MAX_BYTES {
            Err("Each photo must be under 15 MB.".to_string())
        } else {
            Ok(())
        }
    })
}

/// Signed upload URLs for a listing the user may manage (404 otherwise).
#[route(POST "/admin/listings/{id}/images/presign")]
pub async fn presign_images(
    cx: &Cx,
    Json(request): Json<UploadRequest>,
) -> Result<Json<Vec<ImagePresignResponse>>> {
    let user = staff(cx).await?;
    let raw = id(cx);
    Uuid::parse_str(raw).map_err(|_| not_found())?;
    let listing = managed_listing(cx, &user, raw).await?;
    check_files(&request.files).map_err(web_client::app_error)?;
    let images = request
        .files
        .iter()
        .enumerate()
        .map(|(i, f)| PendingImageMetadata {
            client_file_id: i.to_string(),
            content_type: f.content_type.clone(),
            size_bytes: f.size_bytes,
            display_order: i32::try_from(i).unwrap_or(i32::MAX),
        })
        .collect();
    let signed = get_api_client(cx)
        .presign_listing_images(
            listing.id,
            &ImagePresignRequest { images },
            &service_token(cx, &user)?,
        )
        .await
        .map_err(web_client::app_error)?;
    tracing::info!(listing_id = %listing.id, count = signed.len(), "photo upload URLs issued");
    Ok(Json(signed))
}

/// Remove-photo form fields.
#[derive(Deserialize)]
pub struct RemoveForm {
    /// The photo to remove.
    pub image_id: Uuid,
}

/// A listing's processed photos (empty if the listing cannot be read).
pub async fn listing_photos(cx: &Cx, listing_id: Uuid) -> Vec<ListingImageResponse> {
    get_api_client(cx)
        .get_listing_by_id(&listing_id.to_string(), None)
        .await
        .map(|d| d.images)
        .unwrap_or_default()
}

/// Photo-name form fields.
#[derive(Deserialize)]
pub struct NameForm {
    /// The photo to name.
    pub image_id: Uuid,
    /// Its 0-based position, for the "Photo N" placeholder.
    #[serde(default)]
    pub index: usize,
    /// The name; blank clears it.
    #[serde(default)]
    pub name: String,
}

/// The listing's photos as suite photo choices: (id, name or "Photo N").
pub fn suite_photo_choices(photos: &[ListingImageResponse]) -> Vec<(String, String)> {
    photos
        .iter()
        .enumerate()
        .map(|(i, p)| (p.id.to_string(), photo_label(p.name.as_deref(), i)))
        .collect()
}

/// One photo's name form: a textbox (placeholder "Photo N") and Save. With
/// HTMX it saves when the textbox changes and swaps only itself.
#[component]
pub async fn photo_name_form(
    cx: &Cx,
    /// The listing.
    listing_id: Uuid,
    /// The photo.
    image_id: Uuid,
    /// The photo's 0-based position.
    index: usize,
    /// The textbox value.
    value: String,
    /// Why the last save failed, if it did.
    error: Option<String>,
) -> Result<impl View> {
    let action = format!("/admin/listings/{listing_id}/images/name");
    let field_id = format!("photo-name-{image_id}");
    Ok(view! {
        <form
            id=(format!("photo-name-form-{image_id}"))
            data-photo-name-form=""
            method="post"
            action=(action.clone())
            hx-post=(action)
            hx-trigger="change, submit"
            hx-target="this"
            hx-swap="outerHTML"
            class="mt-2 grid grid-cols-1 gap-1"
        >
            <input type="hidden" name="image_id" value=(image_id.to_string()) />
            <input type="hidden" name="index" value=(index.to_string()) />
            <label for=(field_id.clone()) class="sr-only">
                (format!("Name for {}", default_photo_label(index)))
            </label>
            <div class="flex gap-1.5">
                <input
                    id=(field_id)
                    name="name"
                    maxlength=(MAX_PHOTO_NAME_CHARS.to_string())
                    placeholder=(default_photo_label(index))
                    value=(value)
                    aria-invalid=(error.is_some().then_some("true"))
                    class="h-9 w-0 min-w-0 flex-1 rounded-md border border-border bg-card px-2 text-sm"
                />
                <button
                    type="submit"
                    class="h-9 shrink-0 rounded-md border border-border px-2.5 text-xs font-medium"
                >
                    "Save"
                </button>
            </div>
            if let Some(error) = error {
                <p role="alert" class="text-xs text-destructive">(error)</p>
            }
        </form>
    })
}

/// The photo grid with a remove button per photo. HTMX swaps it (outerHTML)
/// after a removal, and ui.js re-reads it from the page after an upload.
#[component]
pub async fn photo_grid(
    cx: &Cx,
    /// The listing the photos belong to.
    listing_id: Uuid,
    /// Listing name.
    name: String,
    /// Processed photos in display order.
    photos: Vec<ListingImageResponse>,
    /// Why the last removal failed, if it did.
    error: Option<String>,
) -> Result<impl View> {
    let remove = format!("/admin/listings/{listing_id}/images/remove");
    Ok(view! {
        <div
            id="listing-photos-slot"
            data-photo-count=(photos.len().to_string())
            class="space-y-3"
        >
            if let Some(error) = error {
                <p
                    role="alert"
                    class="rounded-lg border border-destructive p-3 text-sm text-destructive"
                >
                    (error)
                </p>
            }
            if photos.is_empty() {
                <p class="text-sm text-muted-foreground">"No processed photos yet."</p>
            } else {
                <ul id="listing-photos" class="grid grid-cols-2 gap-x-3 gap-y-5 sm:grid-cols-4">
                    for (i, photo) in photos.into_iter().enumerate() {
                        <li data-photo=(photo.id.to_string()) class="relative min-w-0">
                            // A fixed 4:3 frame: the image fills and is clipped
                            // to it, so the name form below keeps its place.
                            <div
                                data-photo-frame=""
                                class="aspect-[4/3] overflow-hidden rounded-lg bg-photo-placeholder"
                            >
                                responsive_image(
                                    src: photo.url,
                                    alt: photo
                                        .name
                                        .clone()
                                        .unwrap_or_else(|| format!("{name}, photo {}", i + 1)),
                                    sizes: Some("(min-width: 640px) 25vw, 50vw".to_string())
                                )
                            </div>
                            <form
                                method="post"
                                action=(remove.clone())
                                hx-post=(remove.clone())
                                hx-target="#listing-photos-slot"
                                hx-swap="outerHTML"
                                hx-confirm="Delete this photo? This cannot be undone."
                                class="absolute top-2 right-2"
                            >
                                <input
                                    type="hidden"
                                    name="image_id"
                                    value=(photo.id.to_string())
                                />
                                <button
                                    type="submit"
                                    aria-label=(format!("Remove photo {}", i + 1))
                                    class="rounded-md bg-card/90 px-2 py-1 text-xs font-medium text-destructive shadow-xs"
                                >
                                    "Remove"
                                </button>
                            </form>
                            photo_name_form(
                                listing_id: listing_id,
                                image_id: photo.id,
                                index: i,
                                value: photo.name.clone().unwrap_or_default(),
                                error: None
                            )
                        </li>
                    }
                </ul>
            }
        </div>
    })
}

/// Removes a photo from a listing the user may manage and returns the
/// refreshed grid.
#[page(POST "/admin/listings/{id}/images/remove")]
pub async fn remove_image(
    cx: &Cx,
    /// The remove form.
    Form(form): Form<RemoveForm>,
) -> Result<impl View> {
    let user = staff(cx).await?;
    let raw = id(cx);
    Uuid::parse_str(raw).map_err(|_| not_found())?;
    let listing = managed_listing(cx, &user, raw).await?;
    let error = match common::app_client::delete_listing_image(
        listing.id,
        form.image_id,
        &service_token(cx, &user)?,
    )
    .await
    {
        Ok(()) => {
            tracing::info!(listing_id = %listing.id, image_id = %form.image_id, "photo removed");
            None
        }
        Err(e) => {
            tracing::info!("photo removal rejected: {e}");
            Some("The photo could not be removed. Try again.".to_string())
        }
    };
    let photos = listing_photos(cx, listing.id).await;
    Ok(view! {
        photo_grid(
            listing_id: listing.id,
            name: listing.name,
            photos: photos,
            error: error
        )
    })
}

/// Saves one photo's name. With HTMX it returns the photo's form (and a
/// toast); without, it goes back to the edit page.
#[page(POST "/admin/listings/{id}/images/name")]
pub async fn name_image(
    cx: &Cx,
    /// The name form.
    Form(form): Form<NameForm>,
) -> Result<impl View> {
    let user = staff(cx).await?;
    let raw = id(cx);
    Uuid::parse_str(raw).map_err(|_| not_found())?;
    let listing = managed_listing(cx, &user, raw).await?;
    let saved = match validate_photo_name(Some(&form.name)) {
        Err(e) => Err(e.to_string()),
        Ok(name) => common::app_client::rename_listing_image(
            listing.id,
            form.image_id,
            name.as_deref(),
            &service_token(cx, &user)?,
        )
        .await
        .map(|()| name)
        .map_err(|e| {
            tracing::warn!(listing_id = %listing.id, image_id = %form.image_id, "photo rename rejected: {e}");
            "The name could not be saved. Try again.".to_string()
        }),
    };
    if !topcoat::htmx::hx_request(cx) && saved.is_ok() {
        return Err(see_other(format!("/admin/listings/{}/edit", listing.id)).into());
    }
    if let Ok(name) = &saved {
        tracing::info!(listing_id = %listing.id, image_id = %form.image_id, named = name.is_some(), "photo named");
    }
    let (value, error, message) = match saved {
        Ok(name) => (
            name.unwrap_or_default(),
            None,
            Some("Photo name saved".to_string()),
        ),
        Err(error) => (form.name.trim().to_string(), Some(error), None),
    };
    Ok(view! {
        photo_name_form(
            listing_id: listing.id,
            image_id: form.image_id,
            index: form.index,
            value: value,
            error: error
        )
        if let Some(message) = message {
            <div hx-swap-oob="beforeend:#toast-region">
                toast(message: message, variant: ToastVariant::Success)
            </div>
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(content_type: &str, size_bytes: u64) -> UploadFile {
        UploadFile {
            content_type: content_type.into(),
            size_bytes,
        }
    }

    #[test]
    fn accepts_up_to_ten_supported_photos() {
        let files: Vec<_> = (0..10).map(|_| file("image/webp", 1024)).collect();
        assert_eq!(check_files(&files), Ok(()));
    }

    #[test]
    fn rejects_empty_too_many_wrong_type_and_oversized() {
        assert!(check_files(&[]).is_err());
        let eleven: Vec<_> = (0..11).map(|_| file("image/png", 1)).collect();
        assert!(check_files(&eleven).is_err());
        assert!(check_files(&[file("image/svg+xml", 10)]).is_err());
        assert!(check_files(&[file("image/jpeg", MAX_BYTES + 1)]).is_err());
        assert!(check_files(&[file("image/jpeg", 0)]).is_err());
    }
}
