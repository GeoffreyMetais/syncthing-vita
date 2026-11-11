//! Pure-Rust QR code decoder (`quircs`, a port of `quirc`) for greyscale camera frames.
//! Platform independent: the same code decodes PS Vita camera frames and host test images.

/// Reusable QR decoder: keeps `quircs` working buffers between frames to avoid reallocations.
pub struct QrDecoder {
    quirc: quircs::Quirc,
}

impl QrDecoder {
    pub fn new() -> Self {
        Self {
            quirc: quircs::Quirc::default(),
        }
    }

    /// Returns the text payload of every QR code found in a `width * height` luma image.
    /// Codes that cannot be extracted or decoded (blur, partial view) are skipped.
    pub fn decode(&mut self, gray: &[u8], width: usize, height: usize) -> Vec<String> {
        if width == 0 || height == 0 || gray.len() < width * height {
            return Vec::new();
        }
        self.quirc
            .identify(width, height, &gray[..width * height])
            .filter_map(|code| code.ok()?.decode().ok())
            .filter_map(|data| String::from_utf8(data.payload).ok())
            .collect()
    }
}

impl Default for QrDecoder {
    fn default() -> Self {
        Self::new()
    }
}
