//! User slugs: the public handle in links such as `/listings?host=jane-brown`.
//!
//! The database assigns the default (`<first>-<last>`, made unique) in the
//! `user_default_slug` trigger; [`slugify`] mirrors its `user_slug_base`
//! function and is used to clean up a slug a user types in.

/// Longest slug, in characters.
pub const MAX_SLUG_CHARS: usize = 60;
/// Shortest slug a user may choose, in characters.
pub const MIN_SLUG_CHARS: usize = 3;

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
}
