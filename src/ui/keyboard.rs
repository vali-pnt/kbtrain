//! On-screen keyboard: solid keycaps tinted by finger zone, labels coloured
//! by the learner's weakness, the next key lit, and presses that sink the
//! cap and send a ripple across the board.

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
    /// Seconds since the last mistake (the wanted key insists after one).
    pub last_miss: Option<f64>,
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

    /// A keycap `w` cells wide and 3 tall: a face with half-cell rounded
    /// edges and a darker lip below. A pressed cap sinks half a cell.
    fn cap(&mut self, x: u16, y: u16, w: u16, label: &str, look: &KeyLook) {
        if w < 3 {
            return;
        }
        let inner = (w - 2) as usize;
        let face = look.face;
        let lip = if self.theme.is_light() {
            face.lerp(Rgb(0, 0, 0), 0.25)
        } else {
            face.lerp(Rgb(0, 0, 0), 0.5)
        };
        let (top, bottom, bottom_fg) = if look.sunk {
            (
                " ".repeat(w as usize),
                format!("▝{}▘", "▀".repeat(inner)),
                face,
            )
        } else {
            (
                format!("▗{}▖", "▄".repeat(inner)),
                format!("▝{}▘", "▀".repeat(inner)),
                lip,
            )
        };
        let edge = |p: &mut Self, yy: u16, s: &str, fg: Rgb| {
            let area = p.buf.area;
            let fg = p.col(fg).c();
            for (cx, ch) in (x..).zip(s.chars()) {
                if cx >= area.right() || yy >= area.bottom() {
                    break;
                }
                if let Some(cell) = p.buf.cell_mut((cx, yy)) {
                    if ch == ' ' {
                        cell.set_char(' ');
                    } else {
                        cell.set_char(ch).set_fg(fg);
                    }
                }
            }
        };
        edge(self, y, &top, look.rim.unwrap_or(face));
        edge(self, y + 2, &bottom, bottom_fg);
        self.put(x, y + 1, "▐", face, None, false);
        self.put(x + w - 1, y + 1, "▌", face, None, false);
        if look.bump {
            // The home-row nub, drawn into the lip.
            let mid = x + 1 + (inner as u16) / 2;
            if !look.sunk {
                self.put(mid, y + 2, "▀", lip.lerp(self.theme.fg, 0.35), None, false);
            }
        }
        let lw = label.chars().count().min(inner);
        let lpad = (inner - lw).div_ceil(2);
        let text: String = format!(
            "{}{}{}",
            " ".repeat(lpad),
            label.chars().take(inner).collect::<String>(),
            " ".repeat(inner - lw - lpad)
        );
        self.put(x + 1, y + 1, &text, look.label, Some(face), look.bold);
    }
}

/// Visual state shared by both keyboard styles.
struct KeyLook {
    face: Rgb,
    /// Light catching the cap's top edge (the press ripple).
    rim: Option<Rgb>,
    label: Rgb,
    bold: bool,
    sunk: bool,
    bump: bool,
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
    let anim = app.animations();
    let mut look = KeyLook {
        face: if locked {
            theme.bg.lerp(theme.panel, 0.6)
        } else {
            theme.panel.lerp(zone, 0.14)
        },
        label: match stat {
            _ if locked => theme.faint,
            Some(s) if s.count() > 0 => theme.heat(s.weakness(target)),
            _ => theme.dim,
        },
        rim: None,
        bold: false,
        sunk: false,
        bump: key.home && matches!(key.finger, Finger::LIndex | Finger::RIndex),
    };

    let is_next = opts.next.is_some_and(|n| n == key.ch || n == key.shift);
    if is_next {
        // After a mistake the wanted key flashes urgently for a moment.
        let urgent = opts.last_miss.is_some_and(|a| a < 0.6);
        let pulse = match anim {
            Animations::Off => 0.5,
            _ if urgent => (app.t * 22.0).sin() * 0.5 + 0.5,
            _ => (app.t * 5.0).sin() * 0.5 + 0.5,
        };
        look.face = theme.accent.lerp(theme.fg, pulse * 0.3);
        look.label = theme.bg;
        look.bold = true;
    }

    for r in opts.ripples {
        let age = (now - r.at).as_secs_f64();
        let hit = r.ch == key.ch || r.ch == key.shift;
        let color = if r.ok { theme.good } else { theme.bad };
        if hit && age < 0.14 {
            look.sunk = anim != Animations::Off;
            look.face = look.face.lerp(color, if r.ok { 0.55 } else { 0.9 });
            look.label = if r.ok { theme.bg } else { theme.fg };
        } else if hit && !r.ok && age < 0.45 {
            look.face = look.face.lerp(theme.bad, 0.6 * (1.0 - age / 0.45));
        }
        if anim != Animations::Full {
            continue;
        }
        // A ring spreading from the pressed key catches each cap's rim.
        let (Some((px, py)), Some((kx, ky))) = (centers(r.ch), centers(key.ch)) else {
            continue;
        };
        let d = ((px - kx).powi(2) + ((py - ky) * 1.1).powi(2)).sqrt();
        let radius = age * 14.0;
        let ring = (1.0 - (d - radius).abs() / 1.3).max(0.0) * (1.0 - age / 0.4).max(0.0);
        if ring > 0.0 && d > 0.5 {
            look.rim = Some(look.face.lerp(color, ring * 0.8));
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
        if lead >= 3 {
            // Left shift lights up when the next key needs it (opposite hand).
            let lit = ri == 3 && next_shift_hand == Some(Hand::Right);
            p.cap(x0, y, lead, LEAD_LABEL[ri], &modifier(theme, lit));
        }
        for key in row {
            let x = x0 + lead + key.col as u16 * unit;
            let l = look(app, key, opts, now, &centers);
            let label = if opts.next == Some(key.shift) && key.shift != key.ch {
                key.shift
            } else {
                key.ch
            };
            p.cap(x, y, unit, &label.to_string(), &l);
        }
        let used = lead + row.len() as u16 * unit;
        let trail = total_w.saturating_sub(used);
        if trail >= 3 {
            let lit = (ri == 3 && next_shift_hand == Some(Hand::Left))
                || (ri == 2 && opts.next == Some('\n'));
            p.cap(x0 + used, y, trail, TRAIL_LABEL[ri], &modifier(theme, lit));
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
        if opts.next != Some(' ') && !l.sunk {
            l.label = theme.faint;
        }
        p.cap(sx, y, sw, "space", &l);
    }
}

/// A modifier key: plain, or lit when the next character needs it.
fn modifier(theme: &Theme, lit: bool) -> KeyLook {
    KeyLook {
        face: if lit {
            theme.accent
        } else {
            theme.bg.lerp(theme.panel, 0.8)
        },
        label: if lit { theme.bg } else { theme.faint },
        rim: None,
        bold: lit,
        sunk: false,
        bump: false,
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
            let lit = l.bold || l.sunk;
            let bg = if lit {
                l.face
            } else {
                theme.fade(l.label, 0.35)
            };
            let fg = if lit { l.label } else { theme.fg };
            p.put(x, y, &format!(" {} ", key.ch), fg, Some(bg), lit);
        }
    }
}
