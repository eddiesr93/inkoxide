//! Standard base64 (RFC 4648 §4) decoding for inline image data. Small
//! enough not to warrant a dependency; tolerates ASCII whitespace, missing
//! padding and a `data:<mime>;base64,` prefix.

fn value(c: u8) -> Option<u32> {
    Some(match c {
        b'A'..=b'Z' => c - b'A',
        b'a'..=b'z' => c - b'a' + 26,
        b'0'..=b'9' => c - b'0' + 52,
        b'+' => 62,
        b'/' => 63,
        _ => return None,
    } as u32)
}

pub(crate) fn decode(input: &str) -> Result<Vec<u8>, String> {
    let mut s = input.trim_start();
    if let Some(rest) = s.strip_prefix("data:") {
        match rest.find(";base64,") {
            Some(i) => s = &rest[i + 8..],
            None => return Err("data URL is not base64-encoded".into()),
        }
    }
    let mut out = Vec::with_capacity(s.len() / 4 * 3 + 3);
    let mut acc = 0u32;
    let mut n = 0u32;
    let mut padding = 0usize;
    for (i, &c) in s.as_bytes().iter().enumerate() {
        if c.is_ascii_whitespace() {
            continue;
        }
        if c == b'=' {
            padding += 1;
            continue;
        }
        if padding > 0 {
            return Err(format!("unexpected data after padding at byte {i}"));
        }
        let v = value(c).ok_or_else(|| format!("invalid base64 character at byte {i}"))?;
        acc = (acc << 6) | v;
        n += 1;
        if n == 4 {
            out.extend_from_slice(&[(acc >> 16) as u8, (acc >> 8) as u8, acc as u8]);
            acc = 0;
            n = 0;
        }
    }
    match n {
        0 => {}
        2 => out.push((acc >> 4) as u8),
        3 => out.extend_from_slice(&[(acc >> 10) as u8, (acc >> 2) as u8]),
        _ => return Err("truncated base64 data".into()),
    }
    if padding > 2 {
        return Err("too much base64 padding".into());
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::decode;

    #[test]
    fn round_trips_known_vectors() {
        assert_eq!(decode("").unwrap(), b"");
        assert_eq!(decode("Zg==").unwrap(), b"f");
        assert_eq!(decode("Zm8=").unwrap(), b"fo");
        assert_eq!(decode("Zm9v").unwrap(), b"foo");
        assert_eq!(decode("Zm9vYg").unwrap(), b"foob");
        assert_eq!(decode("Zm9v\nYmFy").unwrap(), b"foobar");
        assert_eq!(decode("data:image/png;base64,Zm9v").unwrap(), b"foo");
        assert!(decode("Zm9v!").is_err());
        assert!(decode("Z").is_err());
        assert!(decode("Zg==Zg").is_err());
    }
}
