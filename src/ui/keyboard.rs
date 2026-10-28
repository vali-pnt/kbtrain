//! On-screen keyboard: finger-zoned keys with a heatmap of the learner's
//! weakness, the next key highlighted, and a ripple on each press.

use std::time::Instant;

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Modifier, Style},
};

use super::theme::{Rgb, Theme};
use crate::{
    app::{App, Ripple},
    config::Animations,
    layout::{Finger, Hand, Key, NUMBER_ROW},
};

/// Leading modifier width (in key units) and labels per row.
const LEAD: [f64; 4] = [0.0, 1.5, 1.75, 2.25];
const LEAD_LABEL: [&str; 4] = ["", "tab", "caps", "shift"];
const TRAIL_LABEL: [&str; 4] = ["bksp", "", "enter", "shift"];
const ROW_UNITS: f64 = 15.0;

pub struct Opts<'a> {
    pub next: Option<char>,
    /// Overall opacity (adaptive mode fades the whole keyboard).
    pub alpha: f64,
    pub ripples: &'a [Ripple],
    /// Keys still locked by progressive unlocking are drawn dim.
    pub allowed: Option<&'a [char]>,
    pub numbers: bool,
}

/// Rows of text the large keyboard needs.
pub fn large_height(numbers: bool) -> u16 {
    if numbers { 15 } else { 12 }
}

pub fn large_width(unit: u16) -> u16 {
    (ROW_UNITS * unit as f64) as u16
}

pub fn compact_height(numbers: bool) -> u16 {
    if numbers { 4 } else { 3 }
}

struct Painter<'a> {
    buf: &'a mut Buffer,
    theme: &'a Theme,
    alpha: f64,
}

impl Painter<'_> {
    fn col(&self, c: Rgb) -> Rgb {
        self.theme.fade(c, self.alpha)
    }

    fn put(&mut self, x: u16, y: u16, s: &str, fg: Rgb, bg: Option<Rgb>, bold: bool) {
        let area = self.buf.area;
        let mut style = Style::new().fg(self.col(fg).c());
        if let Some(b) = bg {
            style = style.bg(self.col(b).c());
        }
        if bold {
            style = style.add_modifier(Modifier::BOLD);
        }
        for (cx, ch) in (x..).zip(s.chars()) {
            if cx >= area.right() || y >= area.bottom() {
                break;
            }
            if let Some(cell) = self.buf.cell_mut((cx, y)) {
                cell.set_char(ch).set_style(style);
            }
        }
    }

    /// A rounded key cap `w` cells wide and 3 tall.
    #[allow(clippy::too_many_arguments)]
    fn cap(
        &mut self,
        x: u16,
        y: u16,
        w: u16,
        label: &str,
        border: Rgb,
        label_fg: Rgb,
        fill: Option<Rgb>,
        bump: bool,
    ) {
        if w < 3 {
            return;
        }
        let inner = (w - 2) as usize;
        let mid = inner / 2;
        let top = format!("╭{}╮", "─".repeat(inner));
        let bottom: String = if bump {
            let mut s: Vec<char> = "─".repeat(inner).chars().collect();
            s[mid] = '━';
            format!("╰{}╯", s.into_iter().collect::<String>())
        } else {
            format!("╰{}╯", "─".repeat(inner))
        };
        self.put(x, y, &top, border, None, false);
        self.put(x, y + 2, &bottom, border, None, false);
        self.put(x, y + 1, "│", border, None, false);
        self.put(x + w - 1, y + 1, "│", border, None, false);
        let lw = label.chars().count().min(inner);
        let lpad = (inner - lw).div_ceil(2);
        let text: String = format!(
            "{}{}{}",
            " ".repeat(lpad),
            label.chars().take(inner).collect::<String>(),
            " ".repeat(inner - lw - lpad)
        );
        self.put(x + 1, y + 1, &text, label_fg, fill, fill.is_some());
    }
}

/// Visual state shared by both keyboard styles.
struct KeyLook {
    border: Rgb,
    label: Rgb,
    fill: Option<Rgb>,
}

fn look(
    app: &App,
    key: &Key,
    opts: &Opts,
    now: Instant,
    centers: &dyn Fn(char) -> Option<(f64, f64)>,
) -> KeyLook {
    let theme = &app.theme;
    let target = app.target_ms();
    let locked = key.ch.is_ascii_lowercase() && opts.allowed.is_some_and(|a| !a.contains(&key.ch));
    let stat = app.lp().key(key.ch);
    let zone = theme.finger(key.finger);
    let mut look = KeyLook {
        border: theme.fade(zone, if locked { 0.18 } else { 0.5 }),
        label: match stat {
            _ if locked => theme.faint,
            Some(s) if s.count() > 0 => theme.heat(s.weakness(target)),
            _ => theme.dim,
        },
        fill: None,
    };

    let is_next = opts.next.is_some_and(|n| n == key.ch || n == key.shift);
    if is_next {
        let pulse = if app.animations() == Animations::Off {
            0.5
        } else {
            (app.t * 5.0).sin() * 0.5 + 0.5
        };
        look.border = theme.accent.lerp(theme.accent3, pulse * 0.4);
        look.fill = Some(theme.fade(theme.accent, 0.45 + 0.25 * pulse));
        look.label = theme.fg;
    }

    // Ripples: a ring expanding from each pressed key.
    let anim = app.animations();
    for r in opts.ripples {
        let age = (now - r.at).as_secs_f64();
        let hit = r.ch == key.ch || r.ch == key.shift;
        let color = if r.ok { theme.good } else { theme.bad };
        if hit && age < 0.15 {
            look.fill = Some(theme.fade(color, 0.7));
            look.label = theme.bg;
        }
        if anim != Animations::Full {
            continue;
        }
        let (Some((px, py)), Some((kx, ky))) = (centers(r.ch), centers(key.ch)) else {
            continue;
        };
        let d = ((px - kx).powi(2) + ((py - ky) * 1.1).powi(2)).sqrt();
        let radius = age * 9.0;
        let ring = (1.0 - (d - radius).abs() / 1.2).max(0.0) * (1.0 - age / 0.6).max(0.0);
        if ring > 0.0 {
            look.border = look.border.lerp(color, ring * 0.9);
        }
    }
    look
}

/// Key centre in key units, for ripple distances.
fn center_of(app: &App, ch: char) -> Option<(f64, f64)> {
    let (k, _) = app.layout.locate(ch)?;
    Some((LEAD[k.row] + k.col as f64 + 0.5, k.row as f64))
}

pub fn draw_large(buf: &mut Buffer, area: Rect, app: &App, opts: &Opts) {
    if opts.alpha < 0.03 {
        return;
    }
    let theme = &app.theme;
    let unit: u16 = if area.width >= large_width(5) { 5 } else { 4 };
    let total_w = large_width(unit);
    let x0 = area.x + area.width.saturating_sub(total_w) / 2;
    let first_row = if opts.numbers { 0 } else { 1 };
    let now = Instant::now();
    let centers = |c: char| center_of(app, c);
    let next_shift_hand = opts
        .next
        .and_then(|n| app.layout.locate(n))
        .filter(|(_, s)| *s)
        .map(|(k, _)| k.finger.hand());
    let mut p = Painter {
        buf,
        theme,
        alpha: opts.alpha,
    };

    for (ri, row) in app.layout.rows.iter().enumerate().skip(first_row) {
        let y = area.y + (ri - first_row) as u16 * 3;
        if y + 3 > area.bottom() {
            break;
        }
        let lead = (LEAD[ri] * unit as f64).round() as u16;
        let mod_style = theme.fade(theme.faint, 0.9);
        if lead >= 3 {
            let mut border = mod_style;
            let mut fill = None;
            // Left shift lights up when the next key needs it (opposite hand).
            if ri == 3 && next_shift_hand == Some(Hand::Right) {
                border = theme.accent;
                fill = Some(theme.fade(theme.accent, 0.4));
            }
            p.cap(x0, y, lead, LEAD_LABEL[ri], border, theme.dim, fill, false);
        }
        for key in row {
            let x = x0 + lead + key.col as u16 * unit;
            let l = look(app, key, opts, now, &centers);
            let bump = key.home && matches!(key.finger, Finger::LIndex | Finger::RIndex);
            let label = if opts.next == Some(key.shift) && key.shift != key.ch {
                key.shift
            } else {
                key.ch
            };
            p.cap(
                x,
                y,
                unit,
                &label.to_string(),
                l.border,
                l.label,
                l.fill,
                bump,
            );
        }
        let used = lead + row.len() as u16 * unit;
        let trail = total_w.saturating_sub(used);
        if trail >= 3 {
            let mut border = mod_style;
            let mut fill = None;
            if ri == 3 && next_shift_hand == Some(Hand::Left) {
                border = theme.accent;
                fill = Some(theme.fade(theme.accent, 0.4));
            }
            if ri == 2 && opts.next == Some('\n') {
                border = theme.accent;
                fill = Some(theme.fade(theme.accent, 0.4));
            }
            p.cap(
                x0 + used,
                y,
                trail,
                TRAIL_LABEL[ri],
                border,
                theme.dim,
                fill,
                false,
            );
        }
    }

    // Space bar.
    let rows = 4 - first_row;
    let y = area.y + rows as u16 * 3;
    if y + 3 <= area.bottom() {
        let sx = x0 + (3.75 * unit as f64).round() as u16;
        let sw = (6.25 * unit as f64).round() as u16;
        let space_key = Key {
            ch: ' ',
            shift: ' ',
            row: 4,
            col: 0,
            finger: Finger::Thumb,
            home: false,
        };
        let mut l = look(app, &space_key, opts, now, &|_| None);
        l.border = if opts.next == Some(' ') {
            l.border
        } else {
            theme.fade(theme.finger(Finger::Thumb), 0.4)
        };
        p.cap(
            sx,
            y,
            sw,
            "space",
            l.border,
            if l.fill.is_some() {
                theme.fg
            } else {
                theme.dim
            },
            l.fill,
            false,
        );
    }
}

/// One line per row: keys as heat-tinted cells.
pub fn draw_compact(buf: &mut Buffer, area: Rect, app: &App, opts: &Opts) {
    let theme = &app.theme;
    let first_row = if opts.numbers { NUMBER_ROW } else { 1 };
    let stagger = [0u16, 2, 3, 4];
    let width = 13 * 4 + 4;
    let x0 = area.x + area.width.saturating_sub(width) / 2;
    let now = Instant::now();
    let mut p = Painter {
        buf,
        theme,
        alpha: opts.alpha.max(0.6),
    };
    for (ri, row) in app.layout.rows.iter().enumerate().skip(first_row) {
        let y = area.y + (ri - first_row) as u16;
        if y >= area.bottom() {
            break;
        }
        for key in row {
            let x = x0 + stagger[ri] + key.col as u16 * 4;
            let l = look(app, key, opts, now, &|_| None);
            let bg = l.fill.unwrap_or_else(|| theme.fade(l.label, 0.35));
            let fg = if l.fill.is_some() { theme.bg } else { theme.fg };
            p.put(
                x,
                y,
                &format!(" {} ", key.ch),
                fg,
                Some(bg),
                l.fill.is_some(),
            );
        }
    }
}
