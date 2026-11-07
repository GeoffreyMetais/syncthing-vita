//! 2D software vector & anti-aliased TrueType renderer (`960x544` RGBA8888)
//! Inspired by VitaSend (`gfx.c` & `gfx_icons.c`):
//! - Anti-aliased rectangles, rounded rectangles, outlines, circles, rings, thick lines, arcs, and polygons
//! - Embedded Poppins (Regular, Medium, Bold) TrueType fonts rasterized and cached in memory
//! - Vector icons and authentic colored PlayStation button symbols (`Cross`, `Circle`, `Square`, `Triangle`, `L`, `R`, `START`, `SELECT`, `D-Pad`)

use std::collections::HashMap;
use ab_glyph::{Font, FontRef, PxScale, ScaleFont};

pub const GFX_W: i32 = 960;
pub const GFX_H: i32 = 544;

pub type Color = u32;

#[inline]
pub const fn rgba(r: u8, g: u8, b: u8, a: u8) -> Color {
    ((a as u32) << 24) | ((b as u32) << 16) | ((g as u32) << 8) | (r as u32)
}

#[inline]
pub const fn rgb(r: u8, g: u8, b: u8) -> Color {
    rgba(r, g, b, 255)
}

#[inline]
pub const fn with_alpha(c: Color, a: u8) -> Color {
    (c & 0x00FF_FFFF) | ((a as u32) << 24)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FontWeight {
    Regular = 0,
    Medium = 1,
    Bold = 2,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Align {
    Left,
    Center,
    Right,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Icon {
    Logo,
    Receive,
    Send,
    History,
    Settings,
    Star,
    StarOutline,
    Folder,
    File,
    Package,
    Check,
    Close,
    Plus,
    Phone,
    Desktop,
    Laptop,
    Web,
    Server,
    Vita,
    ArrowDown,
    ArrowUp,
    ArrowLeft,
    Wifi,
    Refresh,
    Back,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PadButton {
    Cross,
    Circle,
    Square,
    Triangle,
    L,
    R,
    Start,
    Select,
    Dpad,
    DpadLr,
    DpadUd,
}

static FONT_POPPINS_REG: &[u8] = include_bytes!("../../assets/fonts/font_poppins_regular.ttf");
static FONT_POPPINS_MED: &[u8] = include_bytes!("../../assets/fonts/font_poppins_medium.ttf");
static FONT_POPPINS_BOLD: &[u8] = include_bytes!("../../assets/fonts/font_poppins_bold.ttf");

#[derive(Clone)]
struct CachedGlyph {
    xoff: i32,
    yoff: i32,
    w: usize,
    h: usize,
    adv: f32,
    bitmap: Vec<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct GlyphKey {
    ch: char,
    weight: FontWeight,
    size_px: u16,
}

pub struct Gfx {
    pub pixels: Vec<u32>,
    cx0: i32,
    cy0: i32,
    cx1: i32,
    cy1: i32,
    font_reg: FontRef<'static>,
    font_med: FontRef<'static>,
    font_bold: FontRef<'static>,
    glyph_cache: HashMap<GlyphKey, CachedGlyph>,
}

impl Gfx {
    pub fn new() -> Self {
        let font_reg = FontRef::try_from_slice(FONT_POPPINS_REG).expect("valid poppins regular");
        let font_med = FontRef::try_from_slice(FONT_POPPINS_MED).expect("valid poppins medium");
        let font_bold = FontRef::try_from_slice(FONT_POPPINS_BOLD).expect("valid poppins bold");

        Self {
            pixels: vec![rgb(15, 19, 23); (GFX_W * GFX_H) as usize],
            cx0: 0,
            cy0: 0,
            cx1: GFX_W,
            cy1: GFX_H,
            font_reg,
            font_med,
            font_bold,
            glyph_cache: HashMap::with_capacity(512),
        }
    }

    pub fn clip_reset(&mut self) {
        self.cx0 = 0;
        self.cy0 = 0;
        self.cx1 = GFX_W;
        self.cy1 = GFX_H;
    }

    #[allow(dead_code)]
    pub fn clip(&mut self, x: i32, y: i32, w: i32, h: i32) {
        self.cx0 = x.max(0);
        self.cy0 = y.max(0);
        self.cx1 = (x + w).min(GFX_W).max(self.cx0);
        self.cy1 = (y + h).min(GFX_H).max(self.cy0);
    }

    #[inline]
    fn mix(d: u32, c: u32, a: u32) -> u32 {
        if a >= 255 {
            return c | 0xFF00_0000;
        }
        let a1 = a + (a >> 7); // 0..256
        let ia = 256 - a1;
        let rb = (((c & 0x00FF_00FF) * a1 + (d & 0x00FF_00FF) * ia) >> 8) & 0x00FF_00FF;
        let g = (((c & 0x0000_FF00) * a1 + (d & 0x0000_FF00) * ia) >> 8) & 0x0000_FF00;
        0xFF00_0000 | rb | g
    }

    #[inline]
    fn cov255(v: f32) -> u32 {
        if v <= 0.0 {
            0
        } else if v >= 1.0 {
            255
        } else {
            (v * 255.0 + 0.5) as u32
        }
    }

    #[inline]
    fn plot(&mut self, x: i32, y: i32, c: Color, cov: u32) {
        if x < self.cx0 || x >= self.cx1 || y < self.cy0 || y >= self.cy1 || cov == 0 {
            return;
        }
        let a = (cov * (c >> 24) + 127) / 255;
        if a == 0 {
            return;
        }
        let idx = (y * GFX_W + x) as usize;
        self.pixels[idx] = Self::mix(self.pixels[idx], c, a);
    }

    pub fn clear(&mut self, c: Color) {
        self.clip_reset();
        self.pixels.fill(c | 0xFF00_0000);
    }

    pub fn rect(&mut self, x: i32, y: i32, w: i32, h: i32, c: Color) {
        if w <= 0 || h <= 0 {
            return;
        }
        let x0 = x.max(self.cx0);
        let y0 = y.max(self.cy0);
        let x1 = (x + w).min(self.cx1);
        let y1 = (y + h).min(self.cy1);
        if x1 <= x0 || y1 <= y0 {
            return;
        }
        let a = c >> 24;
        if a == 0 {
            return;
        }
        if a == 255 {
            let v = c | 0xFF00_0000;
            for yy in y0..y1 {
                let row_start = (yy * GFX_W + x0) as usize;
                let row_end = (yy * GFX_W + x1) as usize;
                self.pixels[row_start..row_end].fill(v);
            }
        } else {
            for yy in y0..y1 {
                let base = (yy * GFX_W) as usize;
                for xx in x0..x1 {
                    let idx = base + xx as usize;
                    self.pixels[idx] = Self::mix(self.pixels[idx], c, a);
                }
            }
        }
    }

    #[allow(dead_code)]
    pub fn dim(&mut self, alpha: u8) {
        self.rect(0, 0, GFX_W, GFX_H, rgba(0, 0, 0, alpha));
    }

    #[inline]
    fn circ_cov(px: i32, py: i32, ccx: f32, ccy: f32, r: f32) -> f32 {
        let dx = (px as f32) + 0.5 - ccx;
        let dy = (py as f32) + 0.5 - ccy;
        r + 0.5 - (dx * dx + dy * dy).sqrt()
    }

    pub fn rrect(&mut self, x: i32, y: i32, w: i32, h: i32, mut r: i32, c: Color) {
        if w <= 0 || h <= 0 {
            return;
        }
        r = r.min(w / 2).min(h / 2);
        if r <= 0 {
            self.rect(x, y, w, h, c);
            return;
        }
        self.rect(x + r, y, w - 2 * r, h, c);
        self.rect(x, y + r, r, h - 2 * r, c);
        self.rect(x + w - r, y + r, r, h - 2 * r, c);

        let rf = r as f32;
        for q in 0..4 {
            let qx = if (q & 1) != 0 { x + w - r } else { x };
            let qy = if (q & 2) != 0 { y + h - r } else { y };
            let ccx = if (q & 1) != 0 { (x + w - r) as f32 } else { (x + r) as f32 };
            let ccy = if (q & 2) != 0 { (y + h - r) as f32 } else { (y + r) as f32 };
            for yy in qy..(qy + r) {
                for xx in qx..(qx + r) {
                    let cov = Self::cov255(Self::circ_cov(xx, yy, ccx, ccy, rf));
                    self.plot(xx, yy, c, cov);
                }
            }
        }
    }

    pub fn rrect_outline(&mut self, x: i32, y: i32, w: i32, h: i32, mut r: i32, t: i32, c: Color) {
        if w <= 0 || h <= 0 || t <= 0 {
            return;
        }
        r = r.min(w / 2).min(h / 2).max(0);
        self.rect(x + r, y, w - 2 * r, t, c);
        self.rect(x + r, y + h - t, w - 2 * r, t, c);
        self.rect(x, y + r, t, h - 2 * r, c);
        self.rect(x + w - t, y + r, t, h - 2 * r, c);
        if r == 0 {
            self.rect(x, y, t, t, c);
            self.rect(x + w - t, y, t, t, c);
            self.rect(x, y + h - t, t, t, c);
            self.rect(x + w - t, y + h - t, t, t, c);
            return;
        }
        let rf = r as f32;
        let inner_r = (r - t) as f32;
        for q in 0..4 {
            let qx = if (q & 1) != 0 { x + w - r } else { x };
            let qy = if (q & 2) != 0 { y + h - r } else { y };
            let ccx = if (q & 1) != 0 { (x + w - r) as f32 } else { (x + r) as f32 };
            let ccy = if (q & 2) != 0 { (y + h - r) as f32 } else { (y + r) as f32 };
            for yy in qy..(qy + r) {
                for xx in qx..(qx + r) {
                    let outer = Self::circ_cov(xx, yy, ccx, ccy, rf);
                    let inner = if inner_r > 0.0 {
                        Self::circ_cov(xx, yy, ccx, ccy, inner_r)
                    } else {
                        0.0
                    };
                    let v = outer.min(1.0) - inner.clamp(0.0, 1.0);
                    self.plot(xx, yy, c, Self::cov255(v));
                }
            }
        }
    }

    pub fn circle(&mut self, cx: f32, cy: f32, r: f32, c: Color) {
        if r <= 0.0 {
            return;
        }
        let x0 = (cx - r - 1.0).floor() as i32;
        let x1 = (cx + r + 1.0).ceil() as i32;
        let y0 = (cy - r - 1.0).floor() as i32;
        let y1 = (cy + r + 1.0).ceil() as i32;
        for y in y0..=y1 {
            for x in x0..=x1 {
                self.plot(x, y, c, Self::cov255(Self::circ_cov(x, y, cx, cy, r)));
            }
        }
    }

    pub fn ring(&mut self, cx: f32, cy: f32, r: f32, thick: f32, c: Color) {
        if r <= 0.0 {
            return;
        }
        let ro = r + thick * 0.5;
        let ri = r - thick * 0.5;
        let x0 = (cx - ro - 1.0).floor() as i32;
        let x1 = (cx + ro + 1.0).ceil() as i32;
        let y0 = (cy - ro - 1.0).floor() as i32;
        let y1 = (cy + ro + 1.0).ceil() as i32;
        for y in y0..=y1 {
            for x in x0..=x1 {
                let o = Self::circ_cov(x, y, cx, cy, ro);
                if o <= 0.0 {
                    continue;
                }
                let i = if ri > 0.0 {
                    Self::circ_cov(x, y, cx, cy, ri)
                } else {
                    0.0
                };
                let v = o.min(1.0) - i.clamp(0.0, 1.0);
                self.plot(x, y, c, Self::cov255(v));
            }
        }
    }

    pub fn line(&mut self, x0: f32, y0: f32, x1: f32, y1: f32, thick: f32, c: Color) {
        let hw = thick * 0.5;
        let bx0 = (x0.min(x1) - hw - 1.0).floor() as i32;
        let bx1 = (x0.max(x1) + hw + 1.0).ceil() as i32;
        let by0 = (y0.min(y1) - hw - 1.0).floor() as i32;
        let by1 = (y0.max(y1) + hw + 1.0).ceil() as i32;
        let dx = x1 - x0;
        let dy = y1 - y0;
        let len2 = dx * dx + dy * dy;
        for y in by0..=by1 {
            for x in bx0..=bx1 {
                let px = (x as f32) + 0.5 - x0;
                let py = (y as f32) + 0.5 - y0;
                let t = if len2 > 0.0 {
                    ((px * dx + py * dy) / len2).clamp(0.0, 1.0)
                } else {
                    0.0
                };
                let ex = px - t * dx;
                let ey = py - t * dy;
                let cov = Self::cov255(hw + 0.5 - (ex * ex + ey * ey).sqrt());
                self.plot(x, y, c, cov);
            }
        }
    }

    pub fn arc(&mut self, cx: f32, cy: f32, r: f32, a0: f32, a1: f32, thick: f32, c: Color) {
        const STEPS: i32 = 10;
        let mut px = cx + r * a0.cos();
        let mut py = cy + r * a0.sin();
        for i in 1..=STEPS {
            let a = a0 + (a1 - a0) * (i as f32) / (STEPS as f32);
            let x = cx + r * a.cos();
            let y = cy + r * a.sin();
            self.line(px, py, x, y, thick, c);
            px = x;
            py = y;
        }
    }

    pub fn poly(&mut self, xy: &[(f32, f32)], c: Color) {
        let n = xy.len();
        if n < 3 || n > 32 {
            return;
        }
        let mut ymin = xy[0].1;
        let mut ymax = xy[0].1;
        let mut xmin = xy[0].0;
        let mut xmax = xy[0].0;
        for &(x, y) in &xy[1..] {
            ymin = ymin.min(y);
            ymax = ymax.max(y);
            xmin = xmin.min(x);
            xmax = xmax.max(x);
        }
        let px0 = (xmin.floor() as i32).max(0);
        let px1 = (xmax.ceil() as i32).min(GFX_W);
        let width = px1 - px0;
        if width <= 0 || width > GFX_W {
            return;
        }
        let mut acc = vec![0.0f32; (width + 1) as usize];
        const SUB: i32 = 4;
        for y in (ymin.floor() as i32)..=(ymax.ceil() as i32) {
            if y < self.cy0 || y >= self.cy1 {
                continue;
            }
            acc.fill(0.0);
            for s in 0..SUB {
                let sy = (y as f32) + ((s as f32) + 0.5) / (SUB as f32);
                let mut xs = [0.0f32; 34];
                let mut nx = 0usize;
                for i in 0..n {
                    let j = (i + 1) % n;
                    let (ax, ay) = xy[i];
                    let (bx, by) = xy[j];
                    if (ay <= sy && by > sy) || (by <= sy && ay > sy) {
                        let t = (sy - ay) / (by - ay);
                        xs[nx] = ax + t * (bx - ax);
                        nx += 1;
                    }
                }
                xs[..nx].sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
                let mut k = 0;
                while k + 1 < nx {
                    let a = (xs[k] - (px0 as f32)).max(0.0);
                    let b = (xs[k + 1] - (px0 as f32)).min(width as f32);
                    if b > a {
                        let ia = a.floor() as usize;
                        let ib = b.floor() as usize;
                        if ia == ib {
                            acc[ia] += (b - a) / (SUB as f32);
                        } else {
                            acc[ia] += ((ia + 1) as f32 - a) / (SUB as f32);
                            for item in acc.iter_mut().take(ib).skip(ia + 1) {
                                *item += 1.0 / (SUB as f32);
                            }
                            if ib <= width as usize {
                                acc[ib] += (b - (ib as f32)) / (SUB as f32);
                            }
                        }
                    }
                    k += 2;
                }
            }
            for (x, &val) in acc.iter().enumerate().take(width as usize) {
                if val > 0.0 {
                    self.plot(px0 + (x as i32), y, c, Self::cov255(val));
                }
            }
        }
    }

    pub fn shadow(&mut self, x: i32, y: i32, w: i32, h: i32, r: i32) {
        let mut i = 8;
        while i >= 1 {
            self.rrect(
                x - i,
                y - i / 2 + 4,
                w + 2 * i,
                h + 2 * i,
                r + i,
                rgba(0, 0, 0, 16),
            );
            i -= 2;
        }
    }

    // ============================================================== TrueType Text

    fn get_or_rasterize_glyph(&mut self, ch: char, weight: FontWeight, size: i32) -> CachedGlyph {
        let size_px = size.clamp(8, 96) as u16;
        let clean_ch = if (ch as u32) < 0x20 { ' ' } else { ch };
        let key = GlyphKey {
            ch: clean_ch,
            weight,
            size_px,
        };
        if let Some(g) = self.glyph_cache.get(&key) {
            return g.clone();
        }
        if self.glyph_cache.len() > 1536 {
            self.glyph_cache.clear();
        }

        let font = match weight {
            FontWeight::Regular => &self.font_reg,
            FontWeight::Medium => &self.font_med,
            FontWeight::Bold => &self.font_bold,
        };

        // Match stb_truetype ScaleForMappingEmToPixels metric used in VitaSend
        let units_per_em = font.units_per_em().unwrap_or(1000.0);
        let height_unscaled = font.height_unscaled();
        let px_scale = (size_px as f32) * (height_unscaled / units_per_em);
        let scaled = font.as_scaled(PxScale::from(px_scale));

        let mut glyph_id = scaled.glyph_id(clean_ch);
        if glyph_id.0 == 0 && clean_ch != ' ' {
            glyph_id = scaled.glyph_id('?');
        }

        let adv = scaled.h_advance(glyph_id);
        let glyph = glyph_id.with_scale_and_position(PxScale::from(px_scale), ab_glyph::point(0.0, 0.0));

        let cached = if clean_ch == ' ' {
            CachedGlyph {
                xoff: 0,
                yoff: 0,
                w: 0,
                h: 0,
                adv,
                bitmap: Vec::new(),
            }
        } else if let Some(outlined) = font.outline_glyph(glyph) {
            let bounds = outlined.px_bounds();
            let w = (bounds.width().ceil() as usize).max(1);
            let h = (bounds.height().ceil() as usize).max(1);
            let mut bitmap = vec![0u8; w * h];
            outlined.draw(|x, y, cov| {
                let idx = (y as usize) * w + (x as usize);
                if idx < bitmap.len() {
                    bitmap[idx] = (cov.clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
                }
            });
            CachedGlyph {
                xoff: bounds.min.x.floor() as i32,
                yoff: bounds.min.y.floor() as i32,
                w,
                h,
                adv,
                bitmap,
            }
        } else {
            CachedGlyph {
                xoff: 0,
                yoff: 0,
                w: 0,
                h: 0,
                adv,
                bitmap: Vec::new(),
            }
        };

        self.glyph_cache.insert(key, cached.clone());
        cached
    }

    pub fn text_width(&mut self, size: i32, weight: FontWeight, s: &str) -> i32 {
        let mut w = 0.0f32;
        for ch in s.chars() {
            w += self.get_or_rasterize_glyph(ch, weight, size).adv;
        }
        w.ceil() as i32
    }

    fn draw_run(&mut self, x: i32, cy: i32, size: i32, weight: FontWeight, c: Color, s: &str) {
        let mut pen = x as f32;
        let baseline = cy + ((0.35 * (size as f32) + 0.5) as i32);
        for ch in s.chars() {
            let g = self.get_or_rasterize_glyph(ch, weight, size);
            if !g.bitmap.is_empty() {
                let gx = (pen + 0.5).floor() as i32 + g.xoff;
                let gy = baseline + g.yoff;
                for yy in 0..g.h {
                    let row = &g.bitmap[yy * g.w..(yy + 1) * g.w];
                    for (xx, &cov) in row.iter().enumerate() {
                        if cov != 0 {
                            self.plot(gx + (xx as i32), gy + (yy as i32), c, cov as u32);
                        }
                    }
                }
            }
            pen += g.adv;
        }
    }

    pub fn text(
        &mut self,
        x: i32,
        cy: i32,
        size: i32,
        weight: FontWeight,
        c: Color,
        align: Align,
        s: &str,
    ) -> i32 {
        let w = self.text_width(size, weight, s);
        let left = match align {
            Align::Left => x,
            Align::Center => x - w / 2,
            Align::Right => x - w,
        };
        self.draw_run(left, cy, size, weight, c, s);
        w
    }

    pub fn text_fit(
        &mut self,
        x: i32,
        cy: i32,
        maxw: i32,
        size: i32,
        weight: FontWeight,
        c: Color,
        align: Align,
        s: &str,
    ) -> i32 {
        if maxw <= 0 {
            return 0;
        }
        let w = self.text_width(size, weight, s);
        if w <= maxw {
            return self.text(x, cy, size, weight, c, align, s);
        }
        let dots = "...";
        let dots_w = self.text_width(size, weight, dots) as f32;
        let room = (maxw as f32) - dots_w;
        let mut acc = 0.0f32;
        let mut cut_str = String::new();
        for ch in s.chars() {
            let adv = self.get_or_rasterize_glyph(ch, weight, size).adv;
            if acc + adv > room {
                break;
            }
            acc += adv;
            cut_str.push(ch);
        }
        cut_str.push_str(dots);
        self.text(x, cy, size, weight, c, align, &cut_str)
    }

    // ============================================================== Vector Icons

    pub fn icon(&mut self, icon: Icon, cx: f32, cy: f32, s: f32, c: Color) {
        let t = (s * 0.085).max(1.6);
        let l = |gfx: &mut Self, x0: f32, y0: f32, x1: f32, y1: f32| {
            gfx.line(cx + x0 * s, cy + y0 * s, cx + x1 * s, cy + y1 * s, t, c);
        };
        let rr = |gfx: &mut Self, x0: f32, y0: f32, w: f32, h: f32, r: f32| {
            gfx.rrect_outline(
                (cx + x0 * s + 0.5).floor() as i32,
                (cy + y0 * s + 0.5).floor() as i32,
                (w * s + 0.5).floor() as i32,
                (h * s + 0.5).floor() as i32,
                (r * s + 0.5) as i32,
                (t + 0.5) as i32,
                c,
            );
        };

        match icon {
            Icon::Logo => {
                l(self, -0.34, -0.14, 0.34, -0.14);
                l(self, 0.34, -0.14, 0.16, -0.31);
                l(self, 0.34, -0.14, 0.16, 0.03);
                l(self, 0.34, 0.17, -0.34, 0.17);
                l(self, -0.34, 0.17, -0.16, 0.0);
                l(self, -0.34, 0.17, -0.16, 0.34);
            }
            Icon::Receive => {
                l(self, 0.0, -0.40, 0.0, 0.14);
                l(self, -0.20, -0.06, 0.0, 0.14);
                l(self, 0.20, -0.06, 0.0, 0.14);
                l(self, -0.38, 0.16, -0.38, 0.38);
                l(self, -0.38, 0.38, 0.38, 0.38);
                l(self, 0.38, 0.38, 0.38, 0.16);
            }
            Icon::Send => {
                l(self, 0.0, 0.14, 0.0, -0.40);
                l(self, -0.20, -0.20, 0.0, -0.40);
                l(self, 0.20, -0.20, 0.0, -0.40);
                l(self, -0.38, 0.16, -0.38, 0.38);
                l(self, -0.38, 0.38, 0.38, 0.38);
                l(self, 0.38, 0.38, 0.38, 0.16);
            }
            Icon::History => {
                self.ring(cx, cy, 0.40 * s, t, c);
                l(self, 0.0, 0.0, 0.0, -0.22);
                l(self, 0.0, 0.0, 0.17, 0.09);
            }
            Icon::Settings => {
                self.ring(cx, cy, 0.27 * s, t, c);
                self.circle(cx, cy, 0.09 * s, c);
                for i in 0..8 {
                    let a = (i as f32) * std::f32::consts::FRAC_PI_4;
                    self.line(
                        cx + 0.33 * s * a.cos(),
                        cy + 0.33 * s * a.sin(),
                        cx + 0.45 * s * a.cos(),
                        cy + 0.45 * s * a.sin(),
                        t * 1.5,
                        c,
                    );
                }
            }
            Icon::Star | Icon::StarOutline => {
                let mut pts = [(0.0f32, 0.0f32); 10];
                for (i, pt) in pts.iter_mut().enumerate() {
                    let r = if (i & 1) != 0 { 0.20 } else { 0.48 };
                    let a = -std::f32::consts::FRAC_PI_2 + (i as f32) * 0.628_319;
                    *pt = (cx + r * s * a.cos(), cy + 0.03 * s + r * s * a.sin());
                }
                if icon == Icon::Star {
                    self.poly(&pts, c);
                } else {
                    for i in 0..10 {
                        let j = (i + 1) % 10;
                        self.line(pts[i].0, pts[i].1, pts[j].0, pts[j].1, t, c);
                    }
                }
            }
            Icon::Folder => {
                let raw = [
                    (-0.44, -0.30),
                    (-0.12, -0.30),
                    (-0.02, -0.18),
                    (0.44, -0.18),
                    (0.44, 0.34),
                    (-0.44, 0.34),
                ];
                let q: Vec<(f32, f32)> = raw
                    .iter()
                    .map(|&(px, py)| (cx + px * s, cy + py * s))
                    .collect();
                self.poly(&q, with_alpha(c, 70));
                for i in 0..q.len() {
                    let j = (i + 1) % q.len();
                    self.line(q[i].0, q[i].1, q[j].0, q[j].1, t, c);
                }
            }
            Icon::File => {
                let raw = [
                    (-0.30, -0.44),
                    (0.12, -0.44),
                    (0.32, -0.24),
                    (0.32, 0.44),
                    (-0.30, 0.44),
                ];
                let q: Vec<(f32, f32)> = raw
                    .iter()
                    .map(|&(px, py)| (cx + px * s, cy + py * s))
                    .collect();
                self.poly(&q, with_alpha(c, 60));
                for i in 0..q.len() {
                    let j = (i + 1) % q.len();
                    self.line(q[i].0, q[i].1, q[j].0, q[j].1, t, c);
                }
                l(self, 0.12, -0.44, 0.12, -0.24);
                l(self, 0.12, -0.24, 0.32, -0.24);
            }
            Icon::Package => {
                let raw = [(-0.40, -0.34), (0.40, -0.34), (0.40, 0.42), (-0.40, 0.42)];
                let q: Vec<(f32, f32)> = raw
                    .iter()
                    .map(|&(px, py)| (cx + px * s, cy + py * s))
                    .collect();
                self.poly(&q, with_alpha(c, 60));
                for i in 0..q.len() {
                    let j = (i + 1) % q.len();
                    self.line(q[i].0, q[i].1, q[j].0, q[j].1, t, c);
                }
                l(self, -0.40, -0.06, 0.40, -0.06);
                l(self, 0.0, -0.34, 0.0, -0.06);
                l(self, -0.14, 0.16, 0.14, 0.16);
            }
            Icon::Check => {
                self.line(
                    cx - 0.38 * s,
                    cy + 0.02 * s,
                    cx - 0.12 * s,
                    cy + 0.28 * s,
                    t * 1.3,
                    c,
                );
                self.line(
                    cx - 0.12 * s,
                    cy + 0.28 * s,
                    cx + 0.40 * s,
                    cy - 0.28 * s,
                    t * 1.3,
                    c,
                );
            }
            Icon::Close => {
                l(self, -0.30, -0.30, 0.30, 0.30);
                l(self, -0.30, 0.30, 0.30, -0.30);
            }
            Icon::Plus => {
                l(self, -0.36, 0.0, 0.36, 0.0);
                l(self, 0.0, -0.36, 0.0, 0.36);
            }
            Icon::Phone => {
                rr(self, -0.24, -0.44, 0.48, 0.88, 0.09);
                l(self, -0.07, 0.32, 0.07, 0.32);
            }
            Icon::Desktop => {
                rr(self, -0.45, -0.42, 0.90, 0.58, 0.07);
                l(self, 0.0, 0.16, 0.0, 0.34);
                l(self, -0.20, 0.38, 0.20, 0.38);
            }
            Icon::Laptop => {
                rr(self, -0.33, -0.38, 0.66, 0.46, 0.05);
                l(self, -0.47, 0.22, 0.47, 0.22);
            }
            Icon::Web => {
                self.ring(cx, cy, 0.40 * s, t, c);
                l(self, -0.40, 0.0, 0.40, 0.0);
                l(self, 0.0, -0.40, 0.0, 0.40);
                l(self, -0.33, -0.2, 0.33, -0.2);
                l(self, -0.33, 0.2, 0.33, 0.2);
            }
            Icon::Server => {
                rr(self, -0.40, -0.40, 0.80, 0.34, 0.06);
                rr(self, -0.40, 0.06, 0.80, 0.34, 0.06);
                self.circle(cx - 0.26 * s, cy - 0.23 * s, 0.045 * s, c);
                self.circle(cx - 0.26 * s, cy + 0.23 * s, 0.045 * s, c);
            }
            Icon::Vita => {
                let tb = t * 0.72;
                let tt = (s * 0.032).max(1.1);
                let bx0 = (cx - 0.50 * s + 0.5).floor() as i32;
                let by0 = (cy - 0.25 * s + 0.5).floor() as i32;
                self.rrect_outline(
                    bx0,
                    by0,
                    (1.0 * s + 0.5).floor() as i32,
                    (0.50 * s + 0.5).floor() as i32,
                    (0.17 * s + 0.5) as i32,
                    (tb + 0.5) as i32,
                    c,
                );
                let sx0 = (cx - 0.215 * s + 0.5).floor() as i32;
                let sy0 = (cy - 0.17 * s + 0.5).floor() as i32;
                self.rrect(
                    sx0,
                    sy0,
                    (0.43 * s + 0.5).floor() as i32,
                    (0.34 * s + 0.5).floor() as i32,
                    2,
                    with_alpha(c, 90),
                );
                let lx = cx - 0.355 * s;
                let rx = cx + 0.355 * s;
                self.line(lx, cy - 0.17 * s, lx, cy - 0.03 * s, tt * 1.7, c);
                self.line(lx - 0.07 * s, cy - 0.10 * s, lx + 0.07 * s, cy - 0.10 * s, tt * 1.7, c);
                self.ring(lx, cy + 0.11 * s, 0.05 * s, tt, c);
                let by = cy - 0.10 * s;
                let d = 0.055 * s;
                let r = 0.022 * s + 0.35;
                self.circle(rx, by - d, r, c);
                self.circle(rx, by + d, r, c);
                self.circle(rx - d, by, r, c);
                self.circle(rx + d, by, r, c);
                self.ring(rx, cy + 0.11 * s, 0.05 * s, tt, c);
            }
            Icon::ArrowDown => {
                l(self, 0.0, -0.36, 0.0, 0.36);
                l(self, -0.22, 0.14, 0.0, 0.36);
                l(self, 0.22, 0.14, 0.0, 0.36);
            }
            Icon::ArrowUp => {
                l(self, 0.0, 0.36, 0.0, -0.36);
                l(self, -0.22, -0.14, 0.0, -0.36);
                l(self, 0.22, -0.14, 0.0, -0.36);
            }
            Icon::ArrowLeft => {
                l(self, 0.36, 0.0, -0.36, 0.0);
                l(self, -0.14, -0.22, -0.36, 0.0);
                l(self, -0.14, 0.22, -0.36, 0.0);
            }
            Icon::Back => {
                l(self, 0.14, -0.34, -0.18, 0.0);
                l(self, -0.18, 0.0, 0.14, 0.34);
            }
            Icon::Wifi => {
                self.circle(cx, cy + 0.32 * s, 0.07 * s + 0.5, c);
                self.arc(cx, cy + 0.32 * s, 0.28 * s, -2.356, -0.785, t, c);
                self.arc(cx, cy + 0.32 * s, 0.52 * s, -2.356, -0.785, t, c);
                self.arc(cx, cy + 0.32 * s, 0.76 * s, -2.356, -0.785, t, c);
            }
            Icon::Refresh => {
                self.arc(cx, cy, 0.34 * s, -1.2, 3.6, t, c);
                l(self, 0.14, -0.40, 0.28, -0.18);
                l(self, 0.28, -0.18, 0.02, -0.12);
            }
        }
    }

    // ============================================================== PlayStation Button Symbols

    fn pad_color(b: PadButton) -> Color {
        match b {
            PadButton::Cross => rgb(118, 166, 244),
            PadButton::Circle => rgb(242, 104, 124),
            PadButton::Square => rgb(222, 140, 220),
            PadButton::Triangle => rgb(98, 212, 164),
            _ => rgb(196, 204, 218),
        }
    }

    pub fn pad_button_width(b: PadButton, size: i32) -> i32 {
        match b {
            PadButton::L | PadButton::R => size * 3 / 2,
            PadButton::Start => size * 3,
            PadButton::Select => size * 3 + size / 2,
            _ => size,
        }
    }

    pub fn pad_button(&mut self, b: PadButton, cx: i32, cy: i32, size: i32) -> i32 {
        let w = Self::pad_button_width(b, size);
        let r = (size as f32) * 0.5;
        let col = Self::pad_color(b);
        let disc = rgb(48, 56, 68);
        let fx = cx as f32;
        let fy = cy as f32;

        match b {
            PadButton::Cross | PadButton::Circle | PadButton::Square | PadButton::Triangle => {
                self.circle(fx, fy, r, disc);
                self.ring(fx, fy, r - 0.8, 1.4, with_alpha(col, 150));
                let k = r * 0.42;
                let t = (r * 0.17).max(1.5);
                match b {
                    PadButton::Cross => {
                        self.line(fx - k, fy - k, fx + k, fy + k, t, col);
                        self.line(fx - k, fy + k, fx + k, fy - k, t, col);
                    }
                    PadButton::Circle => {
                        self.ring(fx, fy, k * 1.05, t, col);
                    }
                    PadButton::Square => {
                        self.rrect_outline(
                            cx - ((k * 0.95) as i32),
                            cy - ((k * 0.95) as i32),
                            (k * 1.9) as i32,
                            (k * 1.9) as i32,
                            1,
                            (t + 0.5) as i32,
                            col,
                        );
                    }
                    PadButton::Triangle => {
                        let p = [
                            (fx, fy - k * 1.1),
                            (fx + k * 1.1, fy + k * 0.8),
                            (fx - k * 1.1, fy + k * 0.8),
                        ];
                        for i in 0..3 {
                            let j = (i + 1) % 3;
                            self.line(p[i].0, p[i].1, p[j].0, p[j].1, t, col);
                        }
                    }
                    _ => {}
                }
            }
            PadButton::L | PadButton::R => {
                let h = size * 3 / 4;
                self.rrect(cx - w / 2, cy - h / 2, w, h, h / 2, disc);
                self.text(
                    cx,
                    cy,
                    size * 11 / 20 + 1,
                    FontWeight::Bold,
                    col,
                    Align::Center,
                    if b == PadButton::L { "L" } else { "R" },
                );
            }
            PadButton::Start | PadButton::Select => {
                let h = size * 3 / 4;
                self.rrect(cx - w / 2, cy - h / 2, w, h, h / 2, disc);
                self.text(
                    cx,
                    cy,
                    size * 2 / 5 + 1,
                    FontWeight::Medium,
                    col,
                    Align::Center,
                    if b == PadButton::Start { "START" } else { "SELECT" },
                );
            }
            PadButton::Dpad | PadButton::DpadLr | PadButton::DpadUd => {
                let on = rgb(214, 222, 234);
                let off = rgb(84, 94, 110);
                let th = (r * 0.62) as i32;
                let ri = r as i32;
                self.rrect(
                    cx - ri,
                    cy - th / 2,
                    ri * 2,
                    th,
                    th / 3,
                    if b == PadButton::DpadUd { off } else { on },
                );
                self.rrect(
                    cx - th / 2,
                    cy - ri,
                    th,
                    ri * 2,
                    th / 3,
                    if b == PadButton::DpadLr { off } else { on },
                );
            }
        }
        w
    }
}
