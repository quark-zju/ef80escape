#![deny(missing_docs)]

//! Lossless conversion between UTF-8 and bytes in Rust.
//!
//! Non-UTF-8 bytes (>= 128) are encoded in a subset of Unicode Private Use Area
//! `U+EF80`..`U+EFFF`. Conflicted Unicode characters are escaped by prefixing
//! `U+EF00`.
//!
//! This can be useful to pass mostly UTF-8 but occasionally invalid UTF-8 data
//! to UTF-8-only format like JSON, after receiving the UTF-8 data, reconstruct
//! the original data losslessly.
//!
//! ## About PEP 383 (surrogateescape)
//!
//! [PEP 383 (surrogateescape)](https://peps.python.org/pep-0383) is Python's
//! attempt to solve a similar problem. It uses `U+DC80`..`U+DCFF` (surrogates)
//! for non-UTF-8 bytes.
//!
//! According to [the Unicode FAQ](https://unicode.org/faq/utf_bom.html#utf8-4),
//! surrogate pairs are for UTF-16, and are invalid in UTF-8:
//!
//! > The definition of UTF-8 requires that supplementary characters
//! > (those using surrogate pairs in UTF-16) be encoded with a single
//! > 4-byte sequence. However, there is a widespread practice of generating
//! > pairs of 3-byte sequences in older software, especially software which
//! > pre-dates the introduction of UTF-16 or that is interoperating with
//! > UTF-16 environments under particular constraints.
//! > Such an encoding is *not* conformant to UTF-8 as defined.
//!
//! A standard-conformant UTF-8 implementation like Rust's [`str`] would
//! error out on surrogates. `"\u{dc80}"` does not compile.
//! `char::from_u32(0xdc80)` is `None` meaning `U+DC80` is not a valid
//! Rust [`char`].
//!
//! Therefore, although Python is widely used and it would be nice to be
//! compatible with Python, this crate has to use a different encoding.
//! The `U+EF80`..`U+EFFF` range was originally chosen by
//! [MirBSD](http://www.mirbsd.org/htman/i386/man3/optu8to16.htm).
//! This crate uses an additional `U+EF00` for escaping to achieve lossless
//! round-trip.

use std::borrow::Cow;

// U+0800-U+FFFF: 1110xxxx 10xxxxxx 10xxxxxx
// U+EF00: EE BC 80
// U+EF80: EE BE 80
// U+EFBF: EE BE BF
// U+EFC0: EE BF 80
// U+EFFF: EE BF BF

/// Converts a byte slice to UTF-8 [`str`].
///
/// The return value can be converted back to bytes by [`str_to_bytes`].
///
/// # Examples
///
/// ASCII and a (large) subset of UTF-8 is returned as-is:
///
/// ```
/// # use ef80escape::*;
/// assert_eq!(bytes_to_str(b"abc"), "abc");
/// assert_eq!(bytes_to_str("汉字".as_bytes()), "汉字");
/// assert_eq!(bytes_to_str("🤦🏼‍♂️".as_bytes()), "🤦🏼‍♂️");
/// ```
///
/// Non-UTF-8 bytes (>=128) are replaced by `U+EF80`..`U+EFFF`:
///
/// ```
/// # use ef80escape::*;
/// assert_eq!(bytes_to_str(b"\xff"), "\u{efff}");
/// ```
///
/// For conflicted Unicode characters in `U+EF80`..`U+EFFF`,
/// they are escaped by prefixing `U+EF00`. `U+EF00` is prefixed too:
///
/// ```
/// # use ef80escape::*;
/// assert_eq!(bytes_to_str("\u{efff}".as_bytes()), "\u{ef00}\u{efff}");
/// assert_eq!(bytes_to_str("\u{ef00}".as_bytes()), "\u{ef00}\u{ef00}");
/// ```
///
/// # Zero-copy optimization
///
/// If `data` is already in valid UTF-8 and does not contain `U+Exxx`,
/// the return value uses [`Cow::Borrowed`] for zero-copy.
///
/// ```
/// # use ef80escape::*;
/// # use std::borrow::Cow;
/// let s: &str = "abc 汉字 🤦🏼‍♂️";
/// assert_eq!(bytes_to_str(s.as_bytes()), s);
/// assert!(matches!(bytes_to_str(s.as_bytes()), Cow::Borrowed(_)));
/// ```
///
/// Note: Whether the returned value is [`Cow::Borrowed`] or [`Cow::Owned`]
/// is an optimization detail that might change in the future.
/// Your program should be designed to behave correctly if this function
/// randomly changes [`Cow::Borrowed`] return values to [`Cow::Owned`].
///
/// # About `U+EF00`..`U+EFFF` range
///
/// `U+EF00`..`U+EFFF` is in Unicode Private Use Area. They can be
/// encoded in UTF-8.
pub fn bytes_to_str(data: &[u8]) -> Cow<str> {
    let rest: &mut &[u8] = &mut &data[..];
    let mut result = Vec::new();

    // Extend `out` with escaped UTF-8 bytes.
    fn extend_escaped_utf8(utf8_bytes: &[u8], out: &mut Vec<u8>) {
        for (i, &b) in utf8_bytes.iter().enumerate() {
            if b == 0xee {
                if let (Some(&b1), Some(&b2)) = (utf8_bytes.get(i + 1), utf8_bytes.get(i + 2)) {
                    // U+EE00, U+EF80..U+EFFF
                    if need_escape(b1, b2) {
                        // Push U+EF00 as escape prefix.
                        out.extend_from_slice(&[0xee, 0xbc, 0x80]);
                    }
                }
            }
            out.push(b);
        }
    }

    while !rest.is_empty() {
        match std::str::from_utf8(rest) {
            Ok(s) => {
                if result.is_empty() && !rest.contains(&0xee) {
                    // Zero-copy fast path.
                    return Cow::Borrowed(s);
                }
                extend_escaped_utf8(rest, &mut result);
                break;
            }
            Err(e) => {
                let l = e.valid_up_to();
                extend_escaped_utf8(&rest[..l], &mut result);
                let b = rest[l];
                result.extend_from_slice(&[0xee, 0xbe + ((b ^ 128) >> 6), (b | 0x40) ^ 0x40]);
                *rest = &rest[l + 1..];
            }
        }
    }

    let s = if cfg!(debug_assertions) {
        String::from_utf8(result).unwrap()
    } else {
        // safety: code above only appends valid utf-8 to result.
        unsafe { String::from_utf8_unchecked(result) }
    };
    Cow::Owned(s)
}

/// Inverse of [`bytes_to_str`].
///
/// # Examples
///
/// ASCII and a (large) subset of UTF-8 is returned as-is:
///
/// ```
/// # use ef80escape::*;
/// assert_eq!(str_to_bytes("abc"), "abc".as_bytes());
/// assert_eq!(str_to_bytes("汉字"), "汉字".as_bytes());
/// assert_eq!(str_to_bytes("🤦🏼‍♂️"), "🤦🏼‍♂️".as_bytes());
/// ```
///
/// `U+EF80`..`U+EFFF` without `U+EF00` prefix are converted back to
/// `\x80`..`\xff`:
///
/// ```
/// # use ef80escape::*;
/// assert_eq!(str_to_bytes("\u{ef80}\u{efff}"), &b"\x80\xff"[..]);
/// ```
///
/// `U+EF00` keeps the next `U+EF80`..`U+EFFF` or `U+EF00` unchanged:
///
/// ```
/// # use ef80escape::*;
/// assert_eq!(str_to_bytes("\u{ef00}\u{efff}"), "\u{efff}".as_bytes());
/// assert_eq!(str_to_bytes("\u{ef00}\u{ef00}"), "\u{ef00}".as_bytes());
/// ```
///
/// `U+EF00` followed by other byte sequences won't be produced by
/// [`bytes_to_str`]. It will be simply ignored:
///
/// ```
/// # use ef80escape::*;
/// assert_eq!(str_to_bytes("\u{ef00}abc"), &b"abc"[..]);
/// assert_eq!(str_to_bytes("\u{ef00}abc\u{efff}"), &b"abc\xff"[..]);
/// ```
///
/// # Zero-copy optimization
///
/// If `data` does not contain `0xEE` (`U+Exxx`), the return value
/// uses [`Cow::Borrowed`] for zero-copy.
///
/// ```
/// # use ef80escape::*;
/// # use std::borrow::Cow;
/// let s: &str = "abc 汉字 🤦🏼‍♂️";
/// assert_eq!(str_to_bytes(s), s.as_bytes());
/// assert!(matches!(str_to_bytes(s), Cow::Borrowed(_)));
/// ```
///
/// Note: Whether the returned value is [`Cow::Borrowed`] or [`Cow::Owned`]
/// is an optimization detail that might change in the future.
/// Your program should be designed to behave correct if this function
/// randomly changes [`Cow::Borrowed`] return values to [`Cow::Owned`].
pub fn str_to_bytes<'a>(data: &'a str) -> Cow<'a, [u8]> {
    let data = data.as_bytes();
    if !data.contains(&0xee) {
        return Cow::Borrowed(data);
    }
    let mut result = Vec::with_capacity(data.len());
    let mut escaped = false;
    let mut iter = data.iter().enumerate();
    while let Some((i, &b)) = iter.next() {
        if b == 0xee {
            if let (Some(&b1), Some(&b2)) = (data.get(i + 1), data.get(i + 2)) {
                if need_escape(b1, b2) {
                    match (b1, escaped) {
                        (0xbc, false) => {
                            escaped = true;
                        }
                        (_, true) => {
                            result.extend_from_slice(&[b, b1, b2]);
                            escaped = false;
                        }
                        (_, false) => {
                            let v = ((b1 & 3) << 6) | (b2 & 63);
                            result.push(v);
                        }
                    }
                    iter.next();
                    iter.next();
                    continue;
                }
            }
        }
        escaped = false;
        result.push(b);
    }
    Cow::Owned(result)
}

// Test if bytes [0xEE, b1, b2] matches unicode U+EE00, U+EF80..U+EFFF.
#[inline]
fn need_escape(b1: u8, b2: u8) -> bool {
    (b1 == 0xbc && b2 == 0x80) || ((b1 | 1) == 0xbf && b2 >= 0x80 && b2 <= 0xbf)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn check_round_trip(data: &[u8]) {
        let s = bytes_to_str(data);
        let d = str_to_bytes(&s);
        assert_eq!(data, d.as_ref(), "str: {:?}", s.as_bytes());
    }

    #[test]
    fn round_trip1() {
        check_round_trip(b"");
        check_round_trip(b"abcd  efg");
        check_round_trip("🤦🏼‍♂️".as_bytes());
        check_round_trip("[字符 编码]".as_bytes());
        check_round_trip(b"\xffa\xfe\xfdb\xfc");
        check_round_trip(b"\0\x01\x02\xe0\xe9de\0");
        check_round_trip("\u{ef00}a\u{ef00}\u{ef00}88".as_bytes());
        check_round_trip("\u{ef00}\u{efff}\u{ef00}\u{ef80}\u{ef81}".as_bytes());
        check_round_trip(b"\xbd\xe1tIR\xca\x13[\xd6\xc8H\xbd\xec\xf1wzsl[\xcf\x8b<\xf2U\xd4o\xf8\x8d\xdc\xb1\x86\x83\xe1AQ\xd2\xad\xb1\x00b?sX\x94W\xee0\xc9\x9e)\x97\x1aj\x9e\xb4\x925b_\xcf\x7f\xce\'\xf9\xee\xad\x8c8\xe2e\xa4\xed\x04\x8c\xdcAM\xee\x144\xddS\x03\x9c\x82m\xef,\xb6\xac\x14^Y\x064\xcc!\xd7\xa3\xed\xa7v^\x08i\xd6\xc0O\x0e\xd5\x8e~\x9a\xf8\xba\xb6w\xad7\x16\xa9\xfc\xa2G\xfe\x93ry\\=j\xe17X\x1d\xa1\xdb\x1b-\x8c\x93\x1a\x81%M\xa8E\xad\x079\xb3\xa3f\xf6]\xaek\x0f{\xfc\x88\xbc\x9b\x9a\xb5JtN\r\xdf\xc7\x16A/\xbe7\xb2\x1a\x1fD\xda\xba\x13\"\x1bLU\xb7\x9f\x1d\xe4\xda\xfe\xaaf~\xf9h\xf16\x9c\xada\xdf\x9c\xf0pH\xb1\x06\x93|\x9ep\xcdpz\xbe\x02\xaa\xa1\x87\xe1\x11?g\x84J1\x16}\xe0\x88\x08\xd0\xd6\x03\xeb\x1eNq\t\x08\xa3h\xd3\xca0\xdd\x16\xd6J\xa0~\x96\x11\xeb0\x05\xdc4\xdd\xf3\x0bL\xf5\x00M\xa8\xfc\x06F\xeb\x9el$\x02r\x8cF\x7f\x08y\xf5\xe7\xae\xc2!\x8a^\xf7\x1dd\xe9\xeflvw0\xd2B\x9f\xff\xf6\x92\xfd\x11CH\xc2\xa5\xfa\'S\xda1h+\x08\xbd\xca}\x87\x8cl\xe9%\xe7W`\xb83\x82\xd3n\x98\x91\x94\x02\xe6]\xe6\xe0\xb9*kg\xd50\x8f_\x8cO\x85f\xd4+\x8f\xb0\x97\xec*\xfa[\xc6\xea\x1e\x91\xfb\xbe\xf5u\x0e\x0eK\x11\x9f-1\x16\xc3\x83\xae\xffu\xd6b\xdc\x0f\xc6\x9b\xfc\xae\x7fL\tI\x1d\x85\xbe");
    }

    #[test]
    fn round_trip2() {
        for b1 in 0..=255 {
            for b2 in 0..=255 {
                for b3 in if cfg!(debug_assertions) {
                    0..=5
                } else {
                    0..=255
                } {
                    check_round_trip(&[b1, b2, b3]);
                }
            }
        }
    }

    #[test]
    fn round_trip3() {
        const INTERESTING_BYTES: &[u8] = if cfg!(debug_assertions) {
            &[0, 0x80, 0xbe, 0xee]
        } else {
            &[
                0, 0x01, 0x80, 0x90, 0xbc, 0xbd, 0xbe, 0xbf, 0xc2, 0xe0, 0xee, 0xef, 0xf0, 0xff,
            ]
        };
        for &b1 in INTERESTING_BYTES {
            for &b2 in INTERESTING_BYTES {
                for &b3 in INTERESTING_BYTES {
                    for &b4 in INTERESTING_BYTES {
                        for &b5 in INTERESTING_BYTES {
                            for &b6 in INTERESTING_BYTES {
                                let data = [b1, b2, b3, b4, b5, b6];
                                check_round_trip(&data);
                            }
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn zero_copy() {
        let s = "123 汉字 🤦🏼‍♂️";
        let b = str_to_bytes(&s);
        assert!(matches!(b, Cow::Borrowed(_)));
        let s = bytes_to_str(&b);
        assert!(matches!(s, Cow::Borrowed(_)));
    }
}
