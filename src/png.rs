//! Minimal dependency-free PNG writer (uncompressed deflate) for frame captures.

use std::io::Write;

fn crc32(data: &[u8], mut crc: u32) -> u32 {
    for &b in data {
        crc ^= b as u32;
        for _ in 0..8 {
            crc = if crc & 1 != 0 { (crc >> 1) ^ 0xEDB8_8320 } else { crc >> 1 };
        }
    }
    crc
}

fn chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    out.extend_from_slice(kind);
    out.extend_from_slice(data);
    let crc = crc32(data, crc32(kind, 0xFFFF_FFFF)) ^ 0xFFFF_FFFF;
    out.extend_from_slice(&crc.to_be_bytes());
}

/// Encode tightly packed 8-bit RGB pixels as PNG bytes.
pub fn encode_rgb(w: u32, h: u32, rgb: &[u8]) -> Vec<u8> {
    assert_eq!(rgb.len(), (w * h * 3) as usize);
    let mut raw = Vec::with_capacity(((w * 3 + 1) * h) as usize);
    for row in rgb.chunks_exact((w * 3) as usize) {
        raw.push(0); // filter: none
        raw.extend_from_slice(row);
    }
    let mut z = vec![0x78, 0x01];
    let blocks: Vec<&[u8]> = raw.chunks(65_535).collect();
    for (i, b) in blocks.iter().enumerate() {
        z.push(u8::from(i == blocks.len() - 1));
        z.extend_from_slice(&(b.len() as u16).to_le_bytes());
        z.extend_from_slice(&(!(b.len() as u16)).to_le_bytes());
        z.extend_from_slice(b);
    }
    let (mut a, mut bsum) = (1u32, 0u32);
    for &x in &raw {
        a = (a + x as u32) % 65_521;
        bsum = (bsum + a) % 65_521;
    }
    z.extend_from_slice(&((bsum << 16) | a).to_be_bytes());

    let mut out = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
    let mut ihdr = Vec::new();
    ihdr.extend_from_slice(&w.to_be_bytes());
    ihdr.extend_from_slice(&h.to_be_bytes());
    ihdr.extend_from_slice(&[8, 2, 0, 0, 0]);
    chunk(&mut out, b"IHDR", &ihdr);
    chunk(&mut out, b"IDAT", &z);
    chunk(&mut out, b"IEND", &[]);
    out
}

pub fn write_rgb(path: &std::path::Path, w: u32, h: u32, rgb: &[u8]) -> std::io::Result<()> {
    std::fs::File::create(path)?.write_all(&encode_rgb(w, h, rgb))
}

#[cfg(test)]
mod tests {
    #[test]
    fn crc_of_iend_matches_spec() {
        // Well-known CRC of the IEND chunk type.
        assert_eq!(super::crc32(b"IEND", 0xFFFF_FFFF) ^ 0xFFFF_FFFF, 0xAE42_6082);
    }
}
