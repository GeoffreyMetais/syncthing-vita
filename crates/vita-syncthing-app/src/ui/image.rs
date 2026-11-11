//! Raster image drawing on top of `Gfx`: greyscale camera frames for the QR viewfinder.

use crate::domain::CameraPreview;
use crate::ui::gfx::{rgb, Gfx, GFX_H, GFX_W};

/// Draws `preview` scaled (nearest neighbour) into the `w x h` rectangle at `(x, y)`.
/// Pixels outside the screen are skipped; nothing is drawn without a frame.
pub fn draw_gray_image(gfx: &mut Gfx, x: i32, y: i32, w: i32, h: i32, preview: &CameraPreview) {
    if !preview.has_frame() || w <= 0 || h <= 0 {
        return;
    }
    let x0 = x.max(0);
    let x1 = (x + w).min(GFX_W);
    let y0 = y.max(0);
    let y1 = (y + h).min(GFX_H);
    for py in y0..y1 {
        let sy = ((py - y) as usize * preview.height) / h as usize;
        let src_row = &preview.gray[sy * preview.width..(sy + 1) * preview.width];
        let dst_row = (py * GFX_W) as usize;
        for px in x0..x1 {
            let sx = ((px - x) as usize * preview.width) / w as usize;
            let v = src_row[sx];
            gfx.pixels[dst_row + px as usize] = rgb(v, v, v);
        }
    }
}
