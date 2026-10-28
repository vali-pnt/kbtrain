//! Visual effects: particles, a half-block pixel canvas, the synthwave
//! backdrop, twinkling stars and a big block font.

use rand::{Rng, RngExt};
use ratatui::{buffer::Buffer, layout::Rect};

use super::theme::{Rgb, Theme};

// ───────────────────────────── particles ─────────────────────────────

#[derive(Debug, Clone)]
pub struct Particle {
    pub x: f64,
    pub y: f64,
    pub vx: f64,
    pub vy: f64,
    pub age: f64,
    pub ttl: f64,
    pub glyph: char,
    pub color: Rgb,
}

#[derive(Debug, Default)]
pub struct Particles {
    pub list: Vec<Particle>,
}

const SPARKS: [char; 6] = ['✦', '✧', '•', '·', '*', '+'];

impl Particles {
    /// Advances the simulation. `gravity` is in cells/s².
    pub fn update(&mut self, dt: f64, gravity: f64) {
        for p in &mut self.list {
            p.age += dt;
            p.vy += gravity * dt;
            p.vx *= 1.0 - 0.8 * dt;
            p.x += p.vx * dt;
            p.y += p.vy * dt;
        }
        self.list.retain(|p| p.age < p.ttl);
    }

    /// A firework-style burst from (x, y) in cell coordinates.
    pub fn burst<R: Rng + ?Sized>(
        &mut self,
        x: f64,
        y: f64,
        n: usize,
        colors: &[Rgb],
        rng: &mut R,
    ) {
        for _ in 0..n {
            let a = rng.random_range(0.0..std::f64::consts::TAU);
            let speed = rng.random_range(4.0..22.0);
            self.list.push(Particle {
                x,
                y,
                // Cells are ~2× taller than wide: halve vertical speed.
                vx: a.cos() * speed,
                vy: a.sin() * speed * 0.5 - 3.0,
                age: 0.0,
                ttl: rng.random_range(0.8..1.8),
                glyph: SPARKS[rng.random_range(0..SPARKS.len())],
                color: colors[rng.random_range(0..colors.len())],
            });
        }
    }

    /// Draws particles onto blank cells only, so text stays readable.
    pub fn render(&self, buf: &mut Buffer, area: Rect, theme: &Theme) {
        for p in &self.list {
            let (x, y) = (p.x.round(), p.y.round());
            if x < area.x as f64
                || y < area.y as f64
                || x >= area.right() as f64
                || y >= area.bottom() as f64
            {
                continue;
            }
            let Some(cell) = buf.cell_mut((x as u16, y as u16)) else {
                continue;
            };
            if cell.symbol() != " " {
                continue;
            }
            let alpha = 1.0 - (p.age / p.ttl).powi(2);
            cell.set_char(p.glyph)
                .set_fg(theme.fade(p.color, alpha).c());
        }
    }
}

// ───────────────────────────── pixel canvas ─────────────────────────────

/// A canvas with two square-ish pixels per cell (upper/lower half blocks).
pub struct Pixels {
    w: usize,
    h: usize,
    px: Vec<Option<Rgb>>,
}

impl Pixels {
    pub fn new(area: Rect) -> Self {
        let (w, h) = (area.width as usize, area.height as usize * 2);
        Self {
            w,
            h,
            px: vec![None; w * h],
        }
    }

    pub fn width(&self) -> usize {
        self.w
    }

    pub fn height(&self) -> usize {
        self.h
    }

    pub fn set(&mut self, x: i32, y: i32, c: Rgb) {
        if x >= 0 && y >= 0 && (x as usize) < self.w && (y as usize) < self.h {
            self.px[y as usize * self.w + x as usize] = Some(c);
        }
    }

    pub fn line(&mut self, (x0, y0): (i32, i32), (x1, y1): (i32, i32), c: impl Fn(f64) -> Rgb) {
        let (dx, dy) = ((x1 - x0).abs(), -(y1 - y0).abs());
        let (sx, sy) = (if x0 < x1 { 1 } else { -1 }, if y0 < y1 { 1 } else { -1 });
        let (mut x, mut y, mut err) = (x0, y0, dx + dy);
        let len = dx.max(-dy).max(1) as f64;
        let mut i = 0.0;
        loop {
            self.set(x, y, c(i / len));
            if x == x1 && y == y1 {
                break;
            }
            let e2 = 2 * err;
            if e2 >= dy {
                err += dy;
                x += sx;
            }
            if e2 <= dx {
                err += dx;
                y += sy;
            }
            i += 1.0;
        }
    }

    /// Writes set pixels into the buffer; unset pixels leave cells alone.
    pub fn render(&self, buf: &mut Buffer, area: Rect) {
        for row in 0..area.height as usize {
            for col in 0..self.w {
                let top = self.px[(row * 2) * self.w + col];
                let bot = self.px[(row * 2 + 1) * self.w + col];
                let Some(cell) = buf.cell_mut((area.x + col as u16, area.y + row as u16)) else {
                    continue;
                };
                match (top, bot) {
                    (Some(t), Some(b)) => {
                        cell.set_char('▀').set_fg(t.c()).set_bg(b.c());
                    }
                    (Some(t), None) => {
                        cell.set_char('▀').set_fg(t.c());
                    }
                    (None, Some(b)) => {
                        cell.set_char('▄').set_fg(b.c());
                    }
                    (None, None) => {}
                }
            }
        }
    }
}

// ───────────────────────────── backdrop ─────────────────────────────

/// Cheap deterministic hash for procedural placement.
fn hash(a: u32, b: u32) -> u32 {
    let mut h = a.wrapping_mul(0x9E37_79B1) ^ b.wrapping_mul(0x85EB_CA77);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2C1B_3C6D);
    h ^ (h >> 12)
}

/// Twinkling stars scattered across `area`.
pub fn stars(buf: &mut Buffer, area: Rect, theme: &Theme, t: f64, density: u32) {
    for y in area.top()..area.bottom() {
        for x in area.left()..area.right() {
            let h = hash(x as u32, y as u32);
            if h % 1000 >= density {
                continue;
            }
            let phase = (h >> 10) as f64 / 1000.0;
            let speed = 0.6 + ((h >> 4) % 100) as f64 / 60.0;
            let tw = ((t * speed + phase).sin() + 1.0) / 2.0;
            let glyph = match (h >> 20) % 7 {
                0 => '✦',
                1 | 2 => '·',
                3 => '˙',
                _ => '.',
            };
            let col = if (h >> 8).is_multiple_of(4) {
                theme.accent2
            } else {
                theme.fg
            };
            if let Some(cell) = buf.cell_mut((x, y))
                && cell.symbol() == " "
            {
                cell.set_char(glyph)
                    .set_fg(theme.fade(col, 0.15 + 0.7 * tw).c());
            }
        }
    }
}

/// The retro sun over a scrolling perspective grid. `horizon` is the row
/// (relative to `area`) where sky meets ground.
pub fn synthwave(buf: &mut Buffer, area: Rect, theme: &Theme, t: f64, horizon: u16) {
    if area.width < 4 || area.height < 4 {
        return;
    }
    let mut px = Pixels::new(area);
    let (w, h) = (px.width() as i32, px.height() as i32);
    let hz = (horizon as i32 * 2).clamp(2, h - 2);
    let cx = w / 2;

    // Sun: a disc with horizontal bands cut into its lower half.
    let r = ((hz as f64) * 0.8).min(w as f64 * 0.16).clamp(3.0, 15.0);
    let cy = hz as f64 - r * 0.35;
    let top = (cy - r).floor() as i32;
    let scroll = (t * 4.0) as i32;
    for y in top..hz {
        let dy = y as f64 + 0.5 - cy;
        if dy.abs() > r {
            continue;
        }
        let rel = (dy + r) / (2.0 * r); // 0 top → 1 bottom
        // Bands cut into the lower part of the visible disc, thickening
        // toward the horizon and drifting downward.
        let v = (y - top) as f64 / (hz - top).max(1) as f64;
        if v > 0.35 {
            let gap = 1 + ((v - 0.35) / 0.65 * 2.0) as i32;
            if (y - scroll).rem_euclid(4) < gap {
                continue;
            }
        }
        let half = (r * r - dy * dy).sqrt() * 1.0;
        let col = theme.sun[0].lerp(theme.sun[1], rel);
        for x in (cx as f64 - half).round() as i32..=(cx as f64 + half).round() as i32 {
            px.set(x, y, col);
        }
    }

    // Ground grid.
    let ground = (h - hz).max(1) as f64;
    let phase = (t * 0.6).fract();
    let mut last_y = i32::MAX;
    for k in 0..14 {
        let z = k as f64 + 1.0 - phase;
        let y = (hz as f64 + ground * 1.6 / (z + 0.6)) as i32;
        // Skip lines that would merge into a solid band near the horizon.
        if y >= h || (last_y - y) < 2 {
            continue;
        }
        last_y = y;
        let fade = (1.0 - (z / 10.0)).clamp(0.05, 1.0).powi(2);
        let col = theme.fade(theme.grid, fade);
        for x in 0..w {
            px.set(x, y, col);
        }
    }
    let spread = w as f64 * 0.11;
    for j in -14..=14 {
        let bottom = (cx as f64 + j as f64 * spread * 2.2) as i32;
        let grid = theme.grid;
        px.line((cx + j * 2, hz), (bottom, h - 1), |p| {
            theme.fade(grid, 0.15 + 0.85 * p)
        });
    }
    // Glowing horizon line.
    for x in 0..w {
        let d = ((x - cx).abs() as f64 / (w as f64 / 2.0)).min(1.0);
        px.set(x, hz, theme.accent.lerp(theme.accent2, d));
    }
    px.render(buf, area);
}

// ───────────────────────────── big font ─────────────────────────────

pub const FONT_H: u16 = 5;

fn glyph(c: char) -> [&'static str; 5] {
    match c.to_ascii_uppercase() {
        'A' => [" ### ", "#   #", "#####", "#   #", "#   #"],
        'B' => ["#### ", "#   #", "#### ", "#   #", "#### "],
        'C' => [" ####", "#    ", "#    ", "#    ", " ####"],
        'D' => ["#### ", "#   #", "#   #", "#   #", "#### "],
        'E' => ["#####", "#    ", "#### ", "#    ", "#####"],
        'F' => ["#####", "#    ", "#### ", "#    ", "#    "],
        'G' => [" ####", "#    ", "#  ##", "#   #", " ### "],
        'H' => ["#   #", "#   #", "#####", "#   #", "#   #"],
        'I' => ["###", " # ", " # ", " # ", "###"],
        'J' => ["  ###", "    #", "    #", "#   #", " ### "],
        'K' => ["#   #", "#  # ", "###  ", "#  # ", "#   #"],
        'L' => ["#    ", "#    ", "#    ", "#    ", "#####"],
        'M' => ["#   #", "## ##", "# # #", "#   #", "#   #"],
        'N' => ["#   #", "##  #", "# # #", "#  ##", "#   #"],
        'O' => [" ### ", "#   #", "#   #", "#   #", " ### "],
        'P' => ["#### ", "#   #", "#### ", "#    ", "#    "],
        'Q' => [" ### ", "#   #", "# # #", "#  # ", " ## #"],
        'R' => ["#### ", "#   #", "#### ", "#  # ", "#   #"],
        'S' => [" ####", "#    ", " ### ", "    #", "#### "],
        'T' => ["#####", "  #  ", "  #  ", "  #  ", "  #  "],
        'U' => ["#   #", "#   #", "#   #", "#   #", " ### "],
        'V' => ["#   #", "#   #", "#   #", " # # ", "  #  "],
        'W' => ["#   #", "#   #", "# # #", "## ##", "#   #"],
        'X' => ["#   #", " # # ", "  #  ", " # # ", "#   #"],
        'Y' => ["#   #", " # # ", "  #  ", "  #  ", "  #  "],
        'Z' => ["#####", "   # ", "  #  ", " #   ", "#####"],
        '0' => [" ### ", "#  ##", "# # #", "##  #", " ### "],
        '1' => ["  #  ", " ##  ", "  #  ", "  #  ", " ### "],
        '2' => [" ### ", "#   #", "  ## ", " #   ", "#####"],
        '3' => ["#### ", "    #", " ### ", "    #", "#### "],
        '4' => ["#   #", "#   #", "#####", "    #", "    #"],
        '5' => ["#####", "#    ", "#### ", "    #", "#### "],
        '6' => [" ### ", "#    ", "#### ", "#   #", " ### "],
        '7' => ["#####", "    #", "   # ", "  #  ", "  #  "],
        '8' => [" ### ", "#   #", " ### ", "#   #", " ### "],
        '9' => [" ### ", "#   #", " ####", "    #", " ### "],
        '.' => ["  ", "  ", "  ", "  ", "# "],
        '%' => ["#   #", "   # ", "  #  ", " #   ", "#   #"],
        ';' => ["  ", "# ", "  ", "# ", "# "],
        ',' => ["  ", "  ", "  ", "# ", "# "],
        '\'' => ["# ", "# ", "  ", "  ", "  "],
        '/' => ["    #", "   # ", "  #  ", " #   ", "#    "],
        '-' => ["    ", "    ", "####", "    ", "    "],
        _ => ["   ", "   ", "   ", "   ", "   "],
    }
}

/// Width in cells of `text` rendered in the big font (1-cell letter gaps).
pub fn big_width(text: &str) -> u16 {
    let w: usize = text.chars().map(|c| glyph(c)[0].chars().count() + 1).sum();
    w.saturating_sub(1) as u16
}

/// Draws `text` in the big font at (x, y), colouring each column with
/// `color(t)` where t ∈ [0, 1] runs left to right. An optional drop shadow
/// is drawn one cell down-right.
pub fn big_text(
    buf: &mut Buffer,
    x: u16,
    y: u16,
    text: &str,
    color: impl Fn(f64) -> Rgb,
    shadow: Option<Rgb>,
) {
    let total = big_width(text).max(1) as f64;
    let area = buf.area;
    let mut put = |cx: u16, cy: u16, col: Rgb, ch: char| {
        if cx < area.right()
            && cy < area.bottom()
            && let Some(cell) = buf.cell_mut((cx, cy))
        {
            cell.set_char(ch).set_fg(col.c());
        }
    };
    for pass in 0..2 {
        if pass == 0 && shadow.is_none() {
            continue;
        }
        let mut cx = x;
        for c in text.chars() {
            let g = glyph(c);
            for (row, line) in g.iter().enumerate() {
                for (col, p) in line.chars().enumerate() {
                    if p != '#' {
                        continue;
                    }
                    let gx = cx + col as u16;
                    let gy = y + row as u16;
                    if pass == 0 {
                        put(gx + 1, gy + 1, shadow.unwrap(), '▓');
                    } else {
                        put(gx, gy, color((gx - x) as f64 / total), '█');
                    }
                }
            }
            cx += g[0].chars().count() as u16 + 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn glyphs_are_rectangular() {
        for c in ('A'..='Z').chain('0'..='9').chain(".%;,'/-".chars()) {
            let g = glyph(c);
            let w = g[0].chars().count();
            assert!(g.iter().all(|r| r.chars().count() == w), "{c}");
        }
        assert_eq!(big_width("KB"), 11);
    }

    #[test]
    fn particles_expire() {
        let mut p = Particles::default();
        p.burst(10.0, 10.0, 20, &[Rgb(255, 0, 0)], &mut rand::rng());
        assert_eq!(p.list.len(), 20);
        for _ in 0..40 {
            p.update(0.05, 10.0);
        }
        assert!(p.list.is_empty());
    }
}
