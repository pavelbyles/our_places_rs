//! User slugs: the public handle in links such as `/listings?host=jane-brown`.
//!
//! The database assigns the default (`<first>-<last>`, made unique) in the
//! `user_default_slug` trigger; [`slugify`] mirrors its `user_slug_base`
//! function and is used to clean up a slug a user types in.

/// Longest slug, in characters.
pub const MAX_SLUG_CHARS: usize = 60;
/// Shortest slug a user may choose, in characters.
pub const MIN_SLUG_CHARS: usize = 3;
/// Maximum input characters considered when deriving a suggested listing slug.
pub const MAX_SUGGESTION_NAME_CHARS: usize = 255;

/// Folds the accented Latin letters the SQL `translate()` call handles.
fn fold(c: char) -> char {
    match c {
        'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' | 'ā' => 'a',
        'è' | 'é' | 'ê' | 'ë' | 'ē' => 'e',
        'ì' | 'í' | 'î' | 'ï' | 'ī' => 'i',
        'ò' | 'ó' | 'ô' | 'õ' | 'ö' | 'ø' | 'ō' => 'o',
        'ù' | 'ú' | 'û' | 'ü' | 'ū' => 'u',
        'ý' | 'ÿ' => 'y',
        'ñ' => 'n',
        'ç' => 'c',
        other => other,
    }
}

/// Lowercase ASCII letters and digits joined by single hyphens, at most
/// [`MAX_SLUG_CHARS`] long: `"José  O'Brien"` becomes `"jose-o-brien"`.
pub fn slugify(input: &str) -> String {
    let mut slug = String::with_capacity(input.len());
    for c in input.to_lowercase().chars().map(fold) {
        if c.is_ascii_alphanumeric() {
            slug.push(c);
        } else if !slug.is_empty() && !slug.ends_with('-') {
            slug.push('-');
        }
    }
    let capped: String = slug
        .trim_end_matches('-')
        .chars()
        .take(MAX_SLUG_CHARS)
        .collect();
    capped.trim_end_matches('-').to_string()
}

/// Cleans up a slug a user typed and checks its length. The error is a
/// sentence fit to show next to the field.
pub fn user_slug(input: &str) -> Result<String, String> {
    let slug = slugify(input);
    if slug.chars().count() < MIN_SLUG_CHARS {
        return Err(format!(
            "Profile URL needs at least {MIN_SLUG_CHARS} letters or numbers."
        ));
    }
    Ok(slug)
}

/// Strictly validates a manually entered listing URL slug without normalizing it.
///
/// Enforces length (`3..=60`), lowercase ASCII letters/digits separated by single
/// hyphens (no whitespace, uppercase, punctuation, or leading/trailing/consecutive
/// hyphens), and rejects UUID-shaped strings so `/listings/{id_or_slug}` lookups
/// are never shadowed by UUID parsing.
pub fn validate_listing_slug(input: &str) -> Result<(), &'static str> {
    let len = input.chars().count();
    if len < MIN_SLUG_CHARS {
        return Err("Listing URL must be at least 3 characters.");
    }
    if len > MAX_SLUG_CHARS {
        return Err("Listing URL must be at most 60 characters.");
    }
    if !input
        .bytes()
        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
    {
        return Err("Listing URL may only contain lowercase letters, numbers, and single hyphens.");
    }
    if input.starts_with('-') || input.ends_with('-') || input.contains("--") {
        return Err(
            "Listing URL cannot start or end with a hyphen or contain consecutive hyphens.",
        );
    }
    if uuid::Uuid::parse_str(input).is_ok() {
        return Err("Listing URL cannot be formatted like a UUID.");
    }
    Ok(())
}

/// Derives a valid URL slug suggestion from a listing name.
///
/// Caps input at [`MAX_SUGGESTION_NAME_CHARS`] before slugifying, prefixes `v-`
/// (capped to [`MAX_SLUG_CHARS`]) if the result parses as a UUID, and returns
/// `None` if fewer than [`MIN_SLUG_CHARS`] characters remain.
pub fn suggest_listing_slug(name: &str) -> Option<String> {
    let bounded: String = name.chars().take(MAX_SUGGESTION_NAME_CHARS).collect();
    let base = slugify(&bounded);
    let candidate = if uuid::Uuid::parse_str(&base).is_ok() {
        let prefixed: String = format!("v-{base}").chars().take(MAX_SLUG_CHARS).collect();
        prefixed.trim_end_matches('-').to_string()
    } else {
        base
    };
    (candidate.chars().count() >= MIN_SLUG_CHARS).then_some(candidate)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_the_database_default_rules() {
        assert_eq!(slugify("Jane-Brown"), "jane-brown");
        assert_eq!(slugify("José-O'Brien-Smith"), "jose-o-brien-smith");
        assert_eq!(slugify("  --Hello__World--  "), "hello-world");
        assert_eq!(slugify("李-王"), "");
    }

    #[test]
    fn caps_the_length_without_a_trailing_hyphen() {
        let long = format!("{}-b", "a".repeat(59));
        let slug = slugify(&long);
        assert_eq!(slug.chars().count(), 59);
        assert!(!slug.ends_with('-'));
    }

    #[test]
    fn user_slugs_are_cleaned_and_must_be_long_enough() {
        assert_eq!(
            user_slug("Sea Grape Villas!"),
            Ok("sea-grape-villas".into())
        );
        assert!(user_slug("a!").is_err());
        assert!(user_slug("   ").is_err());
    }

    #[test]
    fn validate_listing_slug_accepts_valid_boundaries() {
        assert_eq!(validate_listing_slug("abc"), Ok(()));
        assert_eq!(validate_listing_slug("coral-cove-villa-2"), Ok(()));
        assert_eq!(validate_listing_slug(&"a".repeat(60)), Ok(()));
    }

    #[test]
    fn validate_listing_slug_rejects_invalid_inputs_without_normalizing() {
        for bad in [
            "",
            "ab",
            &"a".repeat(61),
            "Villa-Azure",
            "villa azure",
            " villa-azure",
            "villa-azure ",
            "villa_azure",
            "villa!",
            "-villa",
            "villa-",
            "villa--azure",
            "550e8400-e29b-41d4-a716-446655440000",
            "550e8400e29b41d4a716446655440000",
        ] {
            assert!(
                validate_listing_slug(bad).is_err(),
                "expected `{bad}` to be rejected"
            );
        }
    }

    #[test]
    fn suggest_listing_slug_derives_valid_url_safe_candidates() {
        assert_eq!(
            suggest_listing_slug("Villa José — Sea View!"),
            Some("villa-jose-sea-view".into())
        );
        assert_eq!(
            suggest_listing_slug("550e8400-e29b-41d4-a716-446655440000"),
            Some("v-550e8400-e29b-41d4-a716-446655440000".into())
        );
        assert_eq!(
            suggest_listing_slug("550e8400e29b41d4a716446655440000"),
            Some("v-550e8400e29b41d4a716446655440000".into())
        );
        assert_eq!(suggest_listing_slug("a!"), None);
        assert_eq!(suggest_listing_slug("   "), None);

        let huge = format!("{} tail", "a".repeat(400));
        let suggested = suggest_listing_slug(&huge);
        assert_eq!(suggested.as_deref().map(str::len), Some(60));
        assert!(
            suggested
                .as_deref()
                .is_some_and(|s| validate_listing_slug(s).is_ok())
        );
    }
}
