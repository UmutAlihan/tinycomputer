//! A minimal PNG encoder: straight RGBA, stored (uncompressed) deflate.
//!
//! macOS builds an image from encoded data, and the cursor's frames are a
//! few kilobytes each, so compression would buy nothing worth a dependency.

/// Encodes a `width` × `height` straight-RGBA image as PNG.
///
/// Returns an empty vector when `rgba` is not `width × height × 4` bytes.
#[must_use]
pub fn encode(width: u32, height: u32, rgba: &[u8]) -> Vec<u8> {
    let row = usize::try_from(width).unwrap_or(0) * 4;
    let rows = usize::try_from(height).unwrap_or(0);
    if row == 0 || rows == 0 || rgba.len() != row * rows {
        return Vec::new();
    }
    // Each scanline is prefixed with filter type 0 (none).
    let mut raw = Vec::with_capacity(rgba.len() + rows);
    for line in rgba.chunks_exact(row) {
        raw.push(0);
        raw.extend_from_slice(line);
    }

    let mut png = b"\x89PNG\r\n\x1a\n".to_vec();
    let mut header = Vec::with_capacity(13);
    header.extend(width.to_be_bytes());
    header.extend(height.to_be_bytes());
    header.extend([8, 6, 0, 0, 0]); // 8-bit, RGBA, deflate, no filter, no interlace
    chunk(&mut png, *b"IHDR", &header);
    chunk(&mut png, *b"IDAT", &zlib(&raw));
    chunk(&mut png, *b"IEND", &[]);
    png
}

fn chunk(png: &mut Vec<u8>, kind: [u8; 4], data: &[u8]) {
    png.extend(u32::try_from(data.len()).unwrap_or(u32::MAX).to_be_bytes());
    let start = png.len();
    png.extend(kind);
    png.extend_from_slice(data);
    let checksum = crc32(&png[start..]);
    png.extend(checksum.to_be_bytes());
}

/// A zlib stream of stored deflate blocks.
fn zlib(data: &[u8]) -> Vec<u8> {
    const BLOCK: usize = 65_535;
    let mut out = vec![0x78, 0x01];
    let blocks: Vec<&[u8]> = if data.is_empty() {
        vec![&[]]
    } else {
        data.chunks(BLOCK).collect()
    };
    let last = blocks.len() - 1;
    for (index, block) in blocks.into_iter().enumerate() {
        out.push(u8::from(index == last));
        let length = u16::try_from(block.len()).unwrap_or(u16::MAX);
        out.extend(length.to_le_bytes());
        out.extend((!length).to_le_bytes());
        out.extend_from_slice(block);
    }
    out.extend(adler32(data).to_be_bytes());
    out
}

fn crc32(data: &[u8]) -> u32 {
    let mut crc = u32::MAX;
    for &byte in data {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            crc = if crc & 1 == 1 {
                (crc >> 1) ^ 0xEDB8_8320
            } else {
                crc >> 1
            };
        }
    }
    !crc
}

fn adler32(data: &[u8]) -> u32 {
    let (mut a, mut b) = (1_u32, 0_u32);
    for &byte in data {
        a = (a + u32::from(byte)) % 65_521;
        b = (b + a) % 65_521;
    }
    (b << 16) | a
}

#[cfg(test)]
pub(super) mod checks {
    pub(in crate::sprite) use super::{adler32, crc32};
}
