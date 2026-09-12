//! 字节读写工具：长度前缀编解码、hex、base64url。
//!
//! 这里刻意不引入额外依赖：v1 需要的行为很少，自己实现比
//! 多一个依赖更可控（也更容易测）。

use crate::error::{Error, Result};

pub fn to_hex(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push(char::from_digit((b >> 4) as u32, 16).unwrap());
        s.push(char::from_digit((b & 0x0f) as u32, 16).unwrap());
    }
    s
}

pub fn from_hex(s: &str) -> Result<Vec<u8>> {
    if s.len() % 2 != 0 {
        return Err(Error::protocol("hex 字符串长度必须是偶数"));
    }
    let mut out = Vec::with_capacity(s.len() / 2);
    let b = s.as_bytes();
    for i in (0..b.len()).step_by(2) {
        let hi = (b[i] as char)
            .to_digit(16)
            .ok_or_else(|| Error::protocol(format!("非法 hex 字符: {}", b[i] as char)))?;
        let lo = (b[i + 1] as char)
            .to_digit(16)
            .ok_or_else(|| Error::protocol(format!("非法 hex 字符: {}", b[i + 1] as char)))?;
        out.push(((hi << 4) | lo) as u8);
    }
    Ok(out)
}

const B64_URL: &[u8; 64] =
    b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

/// base64url（无填充）。用于二维码载荷，避免 `+ / =` 带来的转义问题。
pub fn b64url_encode(input: &[u8]) -> String {
    let mut out = String::with_capacity((input.len() + 2) / 3 * 4);
    for chunk in input.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = *chunk.get(1).unwrap_or(&0) as u32;
        let b2 = *chunk.get(2).unwrap_or(&0) as u32;
        let n = (b0 << 16) | (b1 << 8) | b2;
        out.push(B64_URL[((n >> 18) & 63) as usize] as char);
        out.push(B64_URL[((n >> 12) & 63) as usize] as char);
        if chunk.len() > 1 {
            out.push(B64_URL[((n >> 6) & 63) as usize] as char);
        }
        if chunk.len() > 2 {
            out.push(B64_URL[(n & 63) as usize] as char);
        }
    }
    out
}

pub fn b64url_decode(s: &str) -> Result<Vec<u8>> {
    let mut lut = [255u8; 256];
    for (i, c) in B64_URL.iter().enumerate() {
        lut[*c as usize] = i as u8;
    }
    let mut out = Vec::with_capacity(s.len() / 4 * 3);
    let mut acc: u32 = 0;
    let mut nbits = 0u32;
    for ch in s.bytes() {
        let v = lut[ch as usize];
        if v == 255 {
            return Err(Error::protocol(format!("非法 base64url 字符: {}", ch as char)));
        }
        acc = (acc << 6) | v as u32;
        nbits += 6;
        if nbits >= 8 {
            nbits -= 8;
            out.push((acc >> nbits) as u8);
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_roundtrip() {
        let data = vec![0x00, 0x0f, 0xff, 0xab, 0x10];
        assert_eq!(from_hex(&to_hex(&data)).unwrap(), data);
    }

    #[test]
    fn hex_rejects_odd_length() {
        assert!(from_hex("abc").is_err());
        assert!(from_hex("zz").is_err());
    }

    #[test]
    fn base64url_roundtrip_all_lengths() {
        for len in 0..64usize {
            let data: Vec<u8> = (0..len).map(|i| (i * 37 % 256) as u8).collect();
            let enc = b64url_encode(&data);
            assert!(!enc.contains('+') && !enc.contains('/') && !enc.contains('='));
            assert_eq!(b64url_decode(&enc).unwrap(), data, "len={len}");
        }
    }

    #[test]
    fn base64url_known_vector() {
        // RFC 4648 测试向量，URL 安全字母表
        assert_eq!(b64url_encode(b"foobar"), "Zm9vYmFy");
        assert_eq!(b64url_encode(b"f"), "Zg");
        assert_eq!(b64url_encode(b"fo"), "Zm8");
    }
}
