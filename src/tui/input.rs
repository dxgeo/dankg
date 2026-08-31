//! Raw bytes off stdin, decoded into key events.
//!
//! Split in two on purpose. [`decode`] is a pure function over an
//! already-collected byte slice and is fully unit-testable without a
//! terminal, unlike everything in [`crate::tui::term`]. [`read_key`] is the
//! thin, untested I/O wrapper that feeds it one byte at a time from a real
//! reader.

use std::io::{self, Read};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    Char(char),
    /// A control byte with no more specific meaning below, as the letter it
    /// pairs with: Ctrl-C is `Ctrl('c')`.
    Ctrl(char),
    Enter,
    Tab,
    Backspace,
    Esc,
    Up,
    Down,
    Left,
    Right,
}

/// Decodes the key event at the front of `bytes`, returning it along with
/// how many bytes it consumed. `None` means `bytes` is a prefix of a longer
/// sequence -- only possible for a lone `ESC`, which cannot be told apart
/// from the start of `ESC [ <letter>` without either more bytes or a read
/// timeout. [`read_key`] has no timeout yet (see architecture.org, Terminal
/// UI, open questions), so a bare Esc keypress blocks until another key
/// arrives; nothing in the current interaction table binds standalone Esc
/// to anything, so this is a documented gap rather than a silent one.
pub fn decode(bytes: &[u8]) -> Option<(Key, usize)> {
    let &first = bytes.first()?;
    match first {
        0x1b => {
            if bytes.len() < 2 {
                return None;
            }
            if bytes[1] != b'[' {
                return Some((Key::Esc, 1));
            }
            if bytes.len() < 3 {
                return None;
            }
            let key = match bytes[2] {
                b'A' => Key::Up,
                b'B' => Key::Down,
                b'C' => Key::Right,
                b'D' => Key::Left,
                _ => return Some((Key::Esc, 1)),
            };
            Some((key, 3))
        }
        b'\r' | b'\n' => Some((Key::Enter, 1)),
        b'\t' => Some((Key::Tab, 1)),
        0x7f | 0x08 => Some((Key::Backspace, 1)),
        0x01..=0x1a => Some((Key::Ctrl((first - 1 + b'a') as char), 1)),
        _ => {
            let width = utf8_width(first);
            if bytes.len() < width {
                return None;
            }
            let ch = std::str::from_utf8(&bytes[..width]).ok()?.chars().next()?;
            Some((Key::Char(ch), width))
        }
    }
}

/// Bytes in the UTF-8 sequence starting with `first`. An invalid leading
/// byte is treated as width 1 so decoding cannot stall waiting for bytes
/// that were never going to complete a scalar.
fn utf8_width(first: u8) -> usize {
    if first & 0x80 == 0 {
        1
    } else if first & 0xe0 == 0xc0 {
        2
    } else if first & 0xf0 == 0xe0 {
        3
    } else if first & 0xf8 == 0xf0 {
        4
    } else {
        1
    }
}

/// Blocks until one key event is available on `r`.
pub fn read_key<R: Read>(mut r: R) -> io::Result<Key> {
    let mut buf = Vec::with_capacity(4);
    loop {
        let mut byte = [0u8; 1];
        if r.read(&mut byte)? == 0 {
            return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "stdin closed"));
        }
        buf.push(byte[0]);
        if let Some((key, used)) = decode(&buf) {
            debug_assert_eq!(used, buf.len());
            return Ok(key);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_ascii_and_control_chars() {
        assert_eq!(decode(b"q"), Some((Key::Char('q'), 1)));
        assert_eq!(decode(b"\r"), Some((Key::Enter, 1)));
        assert_eq!(decode(b"\n"), Some((Key::Enter, 1)));
        assert_eq!(decode(b"\t"), Some((Key::Tab, 1)));
        assert_eq!(decode(&[0x7f]), Some((Key::Backspace, 1)));
        assert_eq!(decode(&[0x03]), Some((Key::Ctrl('c'), 1))); // as seen from the real terminal
    }

    #[test]
    fn arrow_keys_are_three_byte_sequences() {
        assert_eq!(decode(b"\x1b[A"), Some((Key::Up, 3)));
        assert_eq!(decode(b"\x1b[B"), Some((Key::Down, 3)));
        assert_eq!(decode(b"\x1b[C"), Some((Key::Right, 3)));
        assert_eq!(decode(b"\x1b[D"), Some((Key::Left, 3)));
    }

    #[test]
    fn a_lone_or_incomplete_escape_asks_for_more_bytes() {
        assert_eq!(decode(b"\x1b"), None);
        assert_eq!(decode(b"\x1b["), None);
    }

    #[test]
    fn an_unrecognised_escape_sequence_falls_back_to_a_standalone_esc() {
        assert_eq!(decode(b"\x1bZ"), Some((Key::Esc, 1)));
        assert_eq!(decode(b"\x1b[Z"), Some((Key::Esc, 1)));
    }

    #[test]
    fn multibyte_utf8_is_one_key_not_several() {
        // 'é' = U+00E9, 2 bytes.
        let bytes = "é".as_bytes();
        assert_eq!(decode(bytes), Some((Key::Char('é'), 2)));
        // A truncated scalar asks for more rather than misdecoding.
        assert_eq!(decode(&bytes[..1]), None);
    }

    #[test]
    fn read_key_assembles_bytes_from_a_reader() {
        let mut src: &[u8] = b"\x1b[A";
        assert_eq!(read_key(&mut src).unwrap(), Key::Up);
    }
}
