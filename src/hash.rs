//! A 64-bit content hash.
//!
//! FNV-1a: eight lines, no tables, no dependency. It is used to key the cache
//! and to stamp config into it, never for anything security-bearing -- a
//! collision costs a stale parse, which the next edit corrects.

pub fn fnv1a(bytes: &[u8]) -> u64 {
    const OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x0000_0100_0000_01b3;

    let mut h = OFFSET;
    for byte in bytes {
        h ^= *byte as u64;
        h = h.wrapping_mul(PRIME);
    }
    h
}

/// Fixed-width so cache file names all have the same shape.
pub fn hex(value: u64) -> String {
    format!("{value:016x}")
}

pub fn parse_hex(text: &str) -> Option<u64> {
    u64::from_str_radix(text, 16).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_the_reference_vectors() {
        assert_eq!(fnv1a(b""), 0xcbf2_9ce4_8422_2325);
        assert_eq!(fnv1a(b"a"), 0xaf63_dc4c_8601_ec8c);
        assert_eq!(fnv1a(b"foobar"), 0x85944171f73967e8);
    }

    #[test]
    fn hex_round_trips_at_fixed_width() {
        let h = fnv1a(b"notes/ideas.md");
        assert_eq!(hex(h).len(), 16);
        assert_eq!(parse_hex(&hex(h)), Some(h));
    }

    #[test]
    fn differs_on_a_one_byte_change() {
        assert_ne!(fnv1a(b"# One\n"), fnv1a(b"# Two\n"));
    }
}
