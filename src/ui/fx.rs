//! Visual effects: easing, typing feedback, backdrop lighting, a half-block
//! pixel canvas, the synthwave backdrop, twinkling stars and a big block font.

use std::time::{Duration, Instant};

use ratatui::{buffer::Buffer, layout::Rect};

use super::theme::{Rgb, Theme};

// ───────────────────────────── motion ─────────────────────────────

pub fn clamp01(x: f64) -> f64 {
    x.clamp(0.0, 1.0)
}

/// Fast start, gentle landing.
pub fn ease_out(t: f64) -> f64 {
    1.0 - (1.0 - clamp01(t)).powi(3)
}

/// Overshoots slightly before settling, for things that land.
pub fn ease_out_back(t: f64) -> f64 {
    let t = clamp01(t) - 1.0;
    1.0 + 2.7 * t.powi(3) + 1.7 * t.powi(2)
}

/// A damped oscillation that starts at zero: a head shake.
pub fn wobble(age: f64, hz: f64, decay: f64) -> f64 {
    if age < 0.0 {
        return 0.0;
    }
    (age * hz * std::f64::consts::TAU).sin() * (-age * decay).exp()
}

pub fn smoothstep(a: f64, b: f64, x: f64) -> f64 {
    let t = clamp01((x - a) / (b - a));
    t * t * (3.0 - 2.0 * t)
}

/// A value easing toward its latest target (menu markers and the like).
#[derive(Debug, Clone, Copy)]
pub struct Tween {
    from: f64,
    to: f64,
    at: Instant,
    secs: f64,
}

impl Tween {
    pub fn new(v: f64, secs: f64) -> Self {
        Self {
            from: v,
            to: v,
            at: Instant::now(),
            secs,
        }
    }

    pub fn set(&mut self, v: f64) {
        if v != self.to {
            let now = Instant::now();
            self.from = self.value_at(now);
            self.to = v;
            self.at = now;
        }
    }

    /// Still moving toward its target?
    pub fn moving(&self) -> bool {
        self.at.elapsed().as_secs_f64() < self.secs
    }

    pub fn value_at(&self, now: Instant) -> f64 {
        let p = (now - self.at).as_secs_f64() / self.secs;
        self.from + (self.to - self.from) * ease_out(p)
    }
}

/// Glyphs a mistyped character flickers through before it settles.
const GLITCH: [char; 10] = ['#', '%', '&', '@', '¤', '§', '≠', '░', '▒', '¿'];

pub fn glitch_char(seed: u32, t: f64) -> char {
    GLITCH[(hash(seed, (t * 28.0) as u32) % GLITCH.len() as u32) as usize]
}

/// Typing feedback the drill screen animates: every press leaves a trace
/// that the renderer turns into glow, glitches and sweeps.
#[derive(Debug)]
pub struct Feedback {
    /// Correct presses (index, when): freshly typed letters glow and cool.
    pub hits: Vec<(usize, Instant)>,
    /// Mistakes (index, when): the letter glitches, the text shakes.
    pub misses: Vec<(usize, Instant)>,
    /// Words finished without a mistake ([start, end), when): a light sweep.
    pub words: Vec<(usize, usize, Instant)>,
    /// Where the caret was before its last move, and when it moved.
    pub caret_from: (usize, Instant),
    /// The latest combo tier reached, and when.
    pub tier_up: Option<(usize, Instant)>,
    /// When the drill text appeared (it cascades in).
    pub shown_at: Instant,
}

impl Default for Feedback {
    fn default() -> Self {
        let now = Instant::now();
        Self {
            hits: Vec::new(),
            misses: Vec::new(),
            words: Vec::new(),
            caret_from: (0, now),
            tier_up: None,
            shown_at: now,
        }
    }
}

impl Feedback {
    pub fn prune(&mut self) {
        let keep = |at: &Instant, ms: u64| at.elapsed() < Duration::from_millis(ms);
        self.hits.retain(|(_, at)| keep(at, 600));
        self.misses.retain(|(_, at)| keep(at, 3000));
        self.words.retain(|(_, _, at)| keep(at, 600));
    }

    /// Mistakes in the last `secs` seconds.
    pub fn recent_misses(&self, secs: f64) -> usize {
        self.misses
            .iter()
            .filter(|(_, at)| at.elapsed().as_secs_f64() < secs)
            .count()
    }

    pub fn last_miss(&self) -> Option<f64> {
        self.misses.last().map(|(_, at)| at.elapsed().as_secs_f64())
    }
}

/// Combo tiers: (threshold, name).
pub const TIERS: [(u32, &str); 5] = [
    (0, ""),
    (10, "flow"),
    (25, "surge"),
    (50, "overdrive"),
    (100, "transcendent"),
];

pub fn tier_of(combo: u32) -> usize {
    TIERS.iter().rposition(|(t, _)| combo >= *t).unwrap_or(0)
}

// ───────────────────────────── lighting ─────────────────────────────

/// A soft coloured light on the backdrop. With `ring > 0` it is a ring of
/// that radius (in units of `rx`/`ry`) instead of a blob.
#[derive(Debug, Clone, Copy)]
pub struct Light {
    pub x: f64,
    pub y: f64,
    pub rx: f64,
    pub ry: f64,
    pub ring: f64,
    pub color: Rgb,
    pub strength: f64,
}

impl Light {
    pub fn blob(x: f64, y: f64, rx: f64, ry: f64, color: Rgb, strength: f64) -> Self {
        Self {
            x,
            y,
            rx,
            ry,
            ring: 0.0,
            color,
            strength,
        }
    }

    fn at(&self, x: f64, y: f64) -> f64 {
        let d = (((x - self.x) / self.rx).powi(2) + ((y - self.y) / self.ry).powi(2)).sqrt();
        if self.ring > 0.0 {
            self.strength * (-((d - self.ring) * 3.0).powi(2)).exp()
        } else {
            self.strength * (-d * d * 1.5).exp()
        }
    }
}

/// Post-processing a screen asks for: lights on the backdrop, a coloured
/// glow bleeding in from the edges, an aurora, and draining the colour.
#[derive(Debug, Default)]
pub struct Scene {
    pub lights: Vec<Light>,
    /// Colour and strength of a glow from the screen edges.
    pub edge: Option<(Rgb, f64)>,
    /// Aurora intensity in [0, 1] and its clock.
    pub aurora: Option<(f64, f64)>,
    /// How much colour to drain from everything, in [0, 1].
    pub drain: f64,
}

/// Re-lights the backdrop: a glow spilling from the top, a vignette, then
/// the scene's lights. Only cells painted in the theme's background or panel
/// colour change, so text and graphics keep their exact colours.
pub fn light(buf: &mut Buffer, theme: &Theme, scene: &Scene, ambient: bool) {
    let area = buf.area;
    if area.width == 0 || area.height == 0 {
        return;
    }
    let (w, h) = (area.width as f64, area.height as f64);
    let light_theme = theme.is_light();
    let shade = if light_theme {
        theme.bg.lerp(theme.dim, 0.35)
    } else {
        Rgb(0, 0, 0)
    };
    let (vig_k, glow_k) = if light_theme {
        (0.25, 0.05)
    } else {
        (0.55, 0.09)
    };
    let sky = theme.gradient[1];
    let (bg, panel) = (theme.bg.c(), theme.panel.c());
    for y in area.top()..area.bottom() {
        for x in area.left()..area.right() {
            let cell = &mut buf[(x, y)];
            let (lit_bg, lit_fg) = (
                cell.bg == bg || cell.bg == panel,
                cell.fg == bg || cell.fg == panel,
            );
            if !lit_bg && !lit_fg {
                continue;
            }
            let (cx, cy) = (x as f64 + 0.5, y as f64 + 0.5);
            let (u, v) = ((cx - area.x as f64) / w, (cy - area.y as f64) / h);
            let shine = |c: Rgb| {
                let mut c = c;
                if ambient {
                    c = c.lerp(sky, glow_k * (1.0 - v).powi(2));
                    let d = ((u - 0.5) * 2.0).powi(2) * 0.7 + ((v - 0.5) * 2.0).powi(2);
                    c = c.lerp(shade, vig_k * smoothstep(0.5, 2.2, d));
                }
                if let Some((k, t)) = scene.aurora {
                    let (a, col) = aurora_at(u, v, t, theme);
                    c = c.lerp(col, a * k);
                }
                for l in &scene.lights {
                    c = c.lerp(l.color, l.at(cx, cy));
                }
                if let Some((col, k)) = scene.edge {
                    // Distance from the nearest edge, in cells (rows count
                    // double, being taller).
                    let dx = (cx - area.x as f64).min(area.right() as f64 - cx);
                    let dy = (cy - area.y as f64).min(area.bottom() as f64 - cy) * 2.2;
                    let e = (1.0 - dx.min(dy) / 6.0).max(0.0);
                    if e > 0.0 {
                        c = c.lerp(col, k * e * e);
                    }
                }
                c
            };
            if lit_bg && let ratatui::style::Color::Rgb(r, g, b) = cell.bg {
                cell.bg = shine(Rgb(r, g, b)).c();
            }
            if lit_fg && let ratatui::style::Color::Rgb(r, g, b) = cell.fg {
                cell.fg = shine(Rgb(r, g, b)).c();
            }
        }
    }
    if scene.drain > 0.0 {
        drain(buf, theme, scene.drain);
    }
}

/// Drains colour from everything except the theme's "bad" red, so that
/// mistakes are the only thing left glowing.
pub fn drain(buf: &mut Buffer, theme: &Theme, k: f64) {
    let keep = theme.bad;
    map_colors(buf, |c| if c == keep { c } else { desaturate(c, k) });
}

/// Curtains of light hanging from a wavy line near the top of the screen.
fn aurora_at(u: f64, v: f64, t: f64, theme: &Theme) -> (f64, Rgb) {
    let edge = 0.1 + 0.05 * (u * 5.0 + t * 0.5).sin() + 0.025 * (u * 13.0 - t * 1.1).sin();
    let below = v - edge;
    if below < -0.03 {
        return (0.0, theme.bg);
    }
    let hang = smoothstep(-0.03, 0.02, below) * (-below.max(0.0) * 7.0).exp();
    let rays = 0.55 + 0.45 * (u * 70.0 + (u * 6.0 + t * 0.8).sin() * 4.0).sin();
    let hue = (u * 0.8 + 0.15 * (t * 0.3).sin()).rem_euclid(2.0);
    let hue = if hue > 1.0 { 2.0 - hue } else { hue };
    (0.4 * hang * rays, Rgb::gradient(&theme.gradient, hue))
}

/// Applies `f` to every RGB colour in the buffer.
pub fn map_colors(buf: &mut Buffer, f: impl Fn(Rgb) -> Rgb) {
    use ratatui::style::Color;
    let map = |c: Color| match c {
        Color::Rgb(r, g, b) => f(Rgb(r, g, b)).c(),
        c => c,
    };
    for cell in buf.content.iter_mut() {
        cell.fg = map(cell.fg);
        cell.bg = map(cell.bg);
    }
}

/// Moves a colour toward grey (and a little darker): colour draining away.
pub fn desaturate(c: Rgb, k: f64) -> Rgb {
    let l = (0.3 * c.0 as f64 + 0.59 * c.1 as f64 + 0.11 * c.2 as f64) * 0.85;
    let g = l.round() as u8;
    c.lerp(Rgb(g, g, g), k)
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

    pub fn get(&self, x: i32, y: i32) -> Option<Rgb> {
        if x >= 0 && y >= 0 && (x as usize) < self.w && (y as usize) < self.h {
            self.px[y as usize * self.w + x as usize]
        } else {
            None
        }
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
pub fn hash(a: u32, b: u32) -> u32 {
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

pub fn glyph(c: char) -> [&'static str; 5] {
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

/// The filled cells of `text` in the big font, relative to its top left.
pub fn big_cells(text: &str) -> Vec<(u16, u16)> {
    let mut cells = Vec::new();
    let mut cx = 0;
    for c in text.chars() {
        let g = glyph(c);
        for (row, line) in g.iter().enumerate() {
            for (col, p) in line.chars().enumerate() {
                if p == '#' {
                    cells.push((cx + col as u16, row as u16));
                }
            }
        }
        cx += g[0].chars().count() as u16 + 1;
    }
    cells
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
                        put(gx + 1, gy + 1, shadow.unwrap(), '█');
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
}
