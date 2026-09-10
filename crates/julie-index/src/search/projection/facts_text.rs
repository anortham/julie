use crate::search::index::truncate_utf8_bytes;

pub(super) fn truncate_to_whitespace_boundary(s: &str, max_bytes: usize) -> &str {
    if s.len() <= max_bytes {
        return s;
    }
    let truncated = truncate_utf8_bytes(s, max_bytes);
    if let Some(idx) = truncated.rfind(char::is_whitespace) {
        &truncated[..idx]
    } else {
        truncated
    }
}
