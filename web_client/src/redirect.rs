/// Longest redirect target accepted, in bytes.
const MAX_REDIRECT_LEN: usize = 512;

/// Returns `raw` if it is a same-origin relative path, otherwise `/`.
///
/// Accepts only targets starting with a single `/`. Rejects protocol-relative
/// (`//host`) and backslash (`/\host`) forms, any control character (browsers
/// strip tabs and newlines, which can turn `/\t/host` into `//host`), and
/// anything longer than 512 bytes.
pub fn safe_redirect(raw: &str) -> &str {
    let is_safe = raw.len() <= MAX_REDIRECT_LEN
        && raw.starts_with('/')
        && !raw.starts_with("//")
        && !raw.starts_with("/\\")
        && !raw.chars().any(char::is_control);
    if is_safe { raw } else { "/" }
}
