//! Parameter lookup for hosts without an allocator.
//!
//! The PhotoCraft host passes a plug-in its validated parameters as a flat JSON object (plus an
//! `_image` object). A `no_std` module without an allocator can't run a JSON parser, so these
//! find one top-level value by key with a byte scan: enough for the plug-in's manifest
//! parameters, which the host has already validated and clamped.
//!
//! Limitation: the scan is not nesting-aware, so a parameter name that also occurs as a key inside
//! `_image` (`x`, `y`, `width`, `height`, `overlap`, `mode`, `depth`, `alpha`, `channels`,
//! `canvasWidth`, `canvasHeight`) would match whichever comes first. Don't name parameters that way.

/// The number stored under `"key"` in `json`, if there is one and it parses as a finite float.
pub fn number(json: &[u8], key: &str) -> Option<f32> {
    let value = value(json, key)?;
    let end = value.iter().position(|b| !matches!(b, b'0'..=b'9' | b'-' | b'+' | b'.' | b'e' | b'E')).unwrap_or(value.len());
    let text = core::str::from_utf8(value.get(..end)?).ok()?;
    text.parse::<f32>().ok().filter(|v| v.is_finite())
}

/// The string stored under `"key"` in `json`, if there is one (no escape sequences).
pub fn string<'a>(json: &'a [u8], key: &str) -> Option<&'a str> {
    let value = value(json, key)?.strip_prefix(b"\"")?;
    let end = value.iter().position(|b| *b == b'"')?;
    let text = value.get(..end)?;
    if text.contains(&b'\\') {
        return None;
    }
    core::str::from_utf8(text).ok()
}

/// The boolean stored under `"key"` in `json`, if there is one.
pub fn boolean(json: &[u8], key: &str) -> Option<bool> {
    let value = value(json, key)?;
    if value.starts_with(b"true") {
        Some(true)
    } else if value.starts_with(b"false") {
        Some(false)
    } else {
        None
    }
}

/// `json` from the first byte of the value that follows `"key":`.
fn value<'a>(json: &'a [u8], key: &str) -> Option<&'a [u8]> {
    let mut from = 0;
    while let Some(at) = find_key(json, key, from) {
        let rest = skip_ws(json.get(at..)?);
        if let Some(after_colon) = rest.strip_prefix(b":") {
            return Some(skip_ws(after_colon));
        }
        from = at;
    }
    None
}

/// Offset just past the next `"key"` token at or after `from`.
fn find_key(json: &[u8], key: &str, from: usize) -> Option<usize> {
    let key = key.as_bytes();
    let needle_len = key.len().checked_add(2)?;
    let hay = json.get(from..)?;
    hay.windows(needle_len)
        .position(|w| w.first() == Some(&b'"') && w.last() == Some(&b'"') && w.get(1..needle_len - 1) == Some(key))
        .and_then(|i| from.checked_add(i)?.checked_add(needle_len))
}

fn skip_ws(s: &[u8]) -> &[u8] {
    let n = s.iter().position(|b| !b.is_ascii_whitespace()).unwrap_or(s.len());
    s.get(n..).unwrap_or(&[])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_top_level_numbers() {
        let j = br#"{"gamma": 1.8, "steps":4,"neg":-0.5e1, "_image": {"x": 0, "width": 1024}}"#;
        assert_eq!(number(j, "gamma"), Some(1.8));
        assert_eq!(number(j, "steps"), Some(4.0));
        assert_eq!(number(j, "neg"), Some(-5.0));
        assert_eq!(number(j, "width"), Some(1024.0));
    }

    #[test]
    fn finds_strings_and_booleans() {
        let j = br#"{"tone": "standard", "mono": true, "auto":false, "name": "a\"b", "n": 1}"#;
        assert_eq!(string(j, "tone"), Some("standard"));
        assert_eq!(boolean(j, "mono"), Some(true));
        assert_eq!(boolean(j, "auto"), Some(false));
        assert_eq!(string(j, "name"), None, "escapes are not supported");
        assert_eq!(string(j, "n"), None);
        assert_eq!(boolean(j, "n"), None);
        assert_eq!(number(j, "tone"), None);
    }

    #[test]
    fn ignores_values_that_merely_contain_the_key() {
        let j = br#"{"mode": "gamma", "gamma": 2}"#;
        assert_eq!(number(j, "gamma"), Some(2.0));
        let j = br#"{"mode": "gamma"}"#;
        assert_eq!(number(j, "gamma"), None);
        let j = br#"{"gammas": 3}"#;
        assert_eq!(number(j, "gamma"), None);
    }

    #[test]
    fn rejects_non_numbers_and_garbage() {
        assert_eq!(number(br#"{"gamma": true}"#, "gamma"), None);
        assert_eq!(number(br#"{"gamma": "1"}"#, "gamma"), None);
        assert_eq!(number(br#"{"gamma": 1e99999}"#, "gamma"), None);
        assert_eq!(number(b"", "gamma"), None);
        assert_eq!(number(b"\"gamma\"", "gamma"), None);
        assert_eq!(number(&[0xff, b'"', b'g', b'"'], "g"), None);
        assert_eq!(string(b"{\"s\": \"unterminated", "s"), None);
    }
}
