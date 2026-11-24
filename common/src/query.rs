//! Building query strings for links.

/// Percent-encodes a query value.
pub fn urlencode(value: &str) -> String {
    value
        .bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                char::from(b).to_string()
            }
            _ => format!("%{b:02X}"),
        })
        .collect()
}

#[cfg(test)]
mod urlencode_tests {
    use super::urlencode;

    #[test]
    fn plain_text_is_unchanged() {
        assert_eq!(urlencode("Runaway-Bay_2.0~"), "Runaway-Bay_2.0~");
    }

    #[test]
    fn characters_that_would_change_the_query_are_encoded() {
        assert_eq!(urlencode("a&currency=XXX#"), "a%26currency%3DXXX%23");
        assert_eq!(urlencode("a b+\"<"), "a%20b%2B%22%3C");
    }

    #[test]
    fn non_ascii_text_encodes_each_byte() {
        assert_eq!(urlencode("é"), "%C3%A9");
    }
}
