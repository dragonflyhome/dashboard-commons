//! Base64 (Standard-Alphabet, mit Füllzeichen) für die Nachweise der
//! Windows-Anmeldung (`Authorization: Negotiate <base64>`), ohne weiteres Paket.

const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

pub fn kodieren(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b = [chunk[0], *chunk.get(1).unwrap_or(&0), *chunk.get(2).unwrap_or(&0)];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        s.push(T[(n >> 18) as usize & 63] as char);
        s.push(T[(n >> 12) as usize & 63] as char);
        s.push(if chunk.len() > 1 { T[(n >> 6) as usize & 63] as char } else { '=' });
        s.push(if chunk.len() > 2 { T[n as usize & 63] as char } else { '=' });
    }
    s
}

fn wert(c: u8) -> Option<u32> {
    match c {
        b'A'..=b'Z' => Some(u32::from(c - b'A')),
        b'a'..=b'z' => Some(u32::from(c - b'a') + 26),
        b'0'..=b'9' => Some(u32::from(c - b'0') + 52),
        b'+' => Some(62),
        b'/' => Some(63),
        _ => None,
    }
}

/// Streng: Länge ein Vielfaches von 4, Füllzeichen nur am Ende, sonst `None`.
pub fn dekodieren(text: &str) -> Option<Vec<u8>> {
    let b = text.as_bytes();
    if !b.len().is_multiple_of(4) {
        return None;
    }
    let mut out = Vec::with_capacity(b.len() / 4 * 3);
    let viererblocks = b.len() / 4;
    for (i, block) in b.chunks(4).enumerate() {
        let letzter = i + 1 == viererblocks;
        let fuell = block.iter().rev().take_while(|&&c| c == b'=').count();
        if fuell > 2 || (fuell > 0 && !letzter) {
            return None;
        }
        let mut n = 0u32;
        for &c in &block[..4 - fuell] {
            n = (n << 6) | wert(c)?;
        }
        n <<= 6 * fuell as u32;
        out.push((n >> 16) as u8);
        if fuell < 2 {
            out.push((n >> 8) as u8);
        }
        if fuell < 1 {
            out.push(n as u8);
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hin_und_zurueck() {
        for (roh, text) in [("", ""), ("f", "Zg=="), ("fo", "Zm8="), ("foo", "Zm9v"), ("foobar", "Zm9vYmFy")] {
            assert_eq!(kodieren(roh.as_bytes()), text);
            assert_eq!(dekodieren(text).as_deref(), Some(roh.as_bytes()));
        }
        let alle: Vec<u8> = (0..=255).collect();
        assert_eq!(dekodieren(&kodieren(&alle)), Some(alle));
    }

    #[test]
    fn unsinn_wird_abgelehnt() {
        for falsch in ["Zg=", "Zg===", "Z===", "Zg==Zm8=", "Zm9v!", "Zm 9", "Zm9v\n"] {
            assert_eq!(dekodieren(falsch), None, "{falsch:?}");
        }
    }
}
