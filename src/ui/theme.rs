//! Colour themes. Everything is defined in RGB so it can be blended for
//! gradients, glow and fades; terminals without truecolor get the nearest
//! xterm-256 colour (see [`to_ansi256`]).

use ratatui::style::{Color, Modifier, Style};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rgb(pub u8, pub u8, pub u8);

impl Rgb {
    pub fn lerp(self, o: Rgb, t: f64) -> Rgb {
        let t = t.clamp(0.0, 1.0);
        let f = |a: u8, b: u8| (a as f64 + (b as f64 - a as f64) * t).round() as u8;
        Rgb(f(self.0, o.0), f(self.1, o.1), f(self.2, o.2))
    }

    pub fn c(self) -> Color {
        Color::Rgb(self.0, self.1, self.2)
    }

    /// Samples a multi-stop gradient at t ∈ [0, 1].
    pub fn gradient(stops: &[Rgb], t: f64) -> Rgb {
        if stops.len() == 1 {
            return stops[0];
        }
        let t = t.clamp(0.0, 1.0) * (stops.len() - 1) as f64;
        let i = (t.floor() as usize).min(stops.len() - 2);
        stops[i].lerp(stops[i + 1], t - i as f64)
    }
}

pub struct Theme {
    pub name: &'static str,
    pub bg: Rgb,
    /// Panel background, a touch lighter than `bg`.
    pub panel: Rgb,
    pub fg: Rgb,
    pub dim: Rgb,
    pub faint: Rgb,
    pub accent: Rgb,
    pub accent2: Rgb,
    pub accent3: Rgb,
    pub good: Rgb,
    pub bad: Rgb,
    pub warn: Rgb,
    /// Title / highlight gradient stops.
    pub gradient: [Rgb; 3],
    /// Heatmap: strong → weak.
    pub heat: [Rgb; 3],
    /// Finger zone colours (LP, LR, LM, LI, RI, RM, RR, RP, thumb).
    pub fingers: [Rgb; 9],
    /// Synthwave backdrop: sun top/bottom and grid line colour.
    pub sun: [Rgb; 2],
    pub grid: Rgb,
}

const fn rgb(hex: u32) -> Rgb {
    Rgb((hex >> 16) as u8, (hex >> 8) as u8, hex as u8)
}

pub const THEMES: [&str; 7] = [
    "synthwave",
    "midnight",
    "sakura",
    "phosphor",
    "glacier",
    "paper",
    "mono",
];

impl Theme {
    pub fn by_name(name: &str) -> Theme {
        match name {
            "midnight" => Theme {
                name: "midnight",
                bg: rgb(0x0b1020),
                panel: rgb(0x121a30),
                fg: rgb(0xd5def5),
                dim: rgb(0x7684a8),
                faint: rgb(0x3a4566),
                accent: rgb(0x6ea8ff),
                accent2: rgb(0x9d7cff),
                accent3: rgb(0xffc56e),
                good: rgb(0x5ee0b0),
                bad: rgb(0xff6b81),
                warn: rgb(0xffb86b),
                gradient: [rgb(0x6ea8ff), rgb(0x9d7cff), rgb(0x5ee0e0)],
                heat: [rgb(0x5ee0b0), rgb(0xffc56e), rgb(0xff6b81)],
                fingers: finger_set(rgb(0x6ea8ff), rgb(0x9d7cff)),
                sun: [rgb(0xffc56e), rgb(0x9d7cff)],
                grid: rgb(0x3d5aa8),
            },
            "sakura" => Theme {
                name: "sakura",
                bg: rgb(0x1e1418),
                panel: rgb(0x2a1c22),
                fg: rgb(0xf6e3ea),
                dim: rgb(0xa58893),
                faint: rgb(0x5a4049),
                accent: rgb(0xff8fb8),
                accent2: rgb(0xffc4d8),
                accent3: rgb(0xb8f0c8),
                good: rgb(0xa6e9c4),
                bad: rgb(0xff5f7e),
                warn: rgb(0xffcf8a),
                gradient: [rgb(0xff8fb8), rgb(0xffc4d8), rgb(0xfff0f5)],
                heat: [rgb(0xa6e9c4), rgb(0xffcf8a), rgb(0xff5f7e)],
                fingers: finger_set(rgb(0xff8fb8), rgb(0xb8a6ff)),
                sun: [rgb(0xfff0c4), rgb(0xff8fb8)],
                grid: rgb(0x8a4a62),
            },
            "phosphor" => Theme {
                name: "phosphor",
                bg: rgb(0x050a05),
                panel: rgb(0x0b140b),
                fg: rgb(0x9dff9d),
                dim: rgb(0x4f9a4f),
                faint: rgb(0x1f3f1f),
                accent: rgb(0x39ff14),
                accent2: rgb(0xb4ff9a),
                accent3: rgb(0xffe066),
                good: rgb(0x39ff14),
                bad: rgb(0xff5555),
                warn: rgb(0xffe066),
                gradient: [rgb(0x1f8f1f), rgb(0x39ff14), rgb(0xd0ffd0)],
                heat: [rgb(0x39ff14), rgb(0xffe066), rgb(0xff5555)],
                fingers: finger_set(rgb(0x39ff14), rgb(0x8fd18f)),
                sun: [rgb(0xd0ffd0), rgb(0x39ff14)],
                grid: rgb(0x1f6f1f),
            },
            "glacier" => Theme {
                name: "glacier",
                bg: rgb(0x2e3440),
                panel: rgb(0x3b4252),
                fg: rgb(0xeceff4),
                dim: rgb(0x9aa5b8),
                faint: rgb(0x4c566a),
                accent: rgb(0x88c0d0),
                accent2: rgb(0x81a1c1),
                accent3: rgb(0xebcb8b),
                good: rgb(0xa3be8c),
                bad: rgb(0xbf616a),
                warn: rgb(0xd08770),
                gradient: [rgb(0x8fbcbb), rgb(0x88c0d0), rgb(0x81a1c1)],
                heat: [rgb(0xa3be8c), rgb(0xebcb8b), rgb(0xbf616a)],
                fingers: finger_set(rgb(0x88c0d0), rgb(0xb48ead)),
                sun: [rgb(0xebcb8b), rgb(0xd08770)],
                grid: rgb(0x5e81ac),
            },
            "paper" => Theme {
                name: "paper",
                bg: rgb(0xf7f3ea),
                panel: rgb(0xece6d8),
                fg: rgb(0x2b2a33),
                dim: rgb(0x7a7666),
                faint: rgb(0xc9c2b0),
                accent: rgb(0xc2410c),
                accent2: rgb(0x1d4ed8),
                accent3: rgb(0x7c3aed),
                good: rgb(0x15803d),
                bad: rgb(0xdc2626),
                warn: rgb(0xb45309),
                gradient: [rgb(0xc2410c), rgb(0xdb2777), rgb(0x7c3aed)],
                heat: [rgb(0x15803d), rgb(0xb45309), rgb(0xdc2626)],
                fingers: finger_set(rgb(0x1d4ed8), rgb(0xc2410c)),
                sun: [rgb(0xf59e0b), rgb(0xdb2777)],
                grid: rgb(0xc9b8a0),
            },
            "mono" => Theme {
                name: "mono",
                bg: rgb(0x111111),
                panel: rgb(0x1c1c1c),
                fg: rgb(0xe8e8e8),
                dim: rgb(0x8a8a8a),
                faint: rgb(0x3a3a3a),
                accent: rgb(0xffffff),
                accent2: rgb(0xbdbdbd),
                accent3: rgb(0xd0d0d0),
                good: rgb(0xe8e8e8),
                bad: rgb(0xff5f5f),
                warn: rgb(0xbdbdbd),
                gradient: [rgb(0x777777), rgb(0xffffff), rgb(0x999999)],
                heat: [rgb(0x5a5a5a), rgb(0xa0a0a0), rgb(0xffffff)],
                fingers: [rgb(0x777777); 9],
                sun: [rgb(0xf0f0f0), rgb(0x666666)],
                grid: rgb(0x444444),
            },
            // "synthwave" — the default neon look.
            _ => Theme {
                name: "synthwave",
                bg: rgb(0x120b24),
                panel: rgb(0x1b1135),
                fg: rgb(0xe9e1ff),
                dim: rgb(0x8c7bb8),
                faint: rgb(0x3b2a63),
                accent: rgb(0xff2a8a),
                accent2: rgb(0x22e4ff),
                accent3: rgb(0xffd23f),
                good: rgb(0x3dffc5),
                bad: rgb(0xff3864),
                warn: rgb(0xff9f43),
                gradient: [rgb(0xff2a8a), rgb(0xb84dff), rgb(0x22e4ff)],
                heat: [rgb(0x22e4ff), rgb(0xb84dff), rgb(0xff2a8a)],
                fingers: finger_set(rgb(0xff2a8a), rgb(0x22e4ff)),
                sun: [rgb(0xffd23f), rgb(0xff2a8a)],
                grid: rgb(0x9b30ff),
            },
        }
    }

    pub fn base(&self) -> Style {
        Style::new().bg(self.bg.c()).fg(self.fg.c())
    }

    pub fn fg(&self, c: Rgb) -> Style {
        Style::new().fg(c.c())
    }

    pub fn dim(&self) -> Style {
        self.fg(self.dim)
    }

    pub fn faint(&self) -> Style {
        self.fg(self.faint)
    }

    pub fn bold(&self, c: Rgb) -> Style {
        self.fg(c).add_modifier(Modifier::BOLD)
    }

    /// Fades a colour toward the background (alpha 1 = full colour).
    pub fn fade(&self, c: Rgb, alpha: f64) -> Rgb {
        self.bg.lerp(c, alpha)
    }

    /// Heatmap colour for a weakness score in [0, 1].
    pub fn heat(&self, t: f64) -> Rgb {
        Rgb::gradient(&self.heat, t)
    }

    pub fn finger(&self, f: crate::layout::Finger) -> Rgb {
        self.fingers[f.index()]
    }

    /// Light theme? (Affects how glow is blended.)
    pub fn is_light(&self) -> bool {
        let Rgb(r, g, b) = self.bg;
        (r as u32 + g as u32 + b as u32) > 382
    }
}

/// Finger colours sweeping from `left` (pinkies) to `right` (index).
fn finger_set(left: Rgb, right: Rgb) -> [Rgb; 9] {
    let l = |t| left.lerp(right, t);
    [
        l(0.0),
        l(0.25),
        l(0.5),
        l(0.8),
        l(0.8),
        l(0.5),
        l(0.25),
        l(0.0),
        rgb(0x8888aa),
    ]
}

/// Nearest xterm-256 palette index for an RGB colour.
pub fn to_ansi256(Rgb(r, g, b): Rgb) -> u8 {
    const LEVELS: [u8; 6] = [0, 95, 135, 175, 215, 255];
    let nearest = |v: u8| {
        LEVELS
            .iter()
            .enumerate()
            .min_by_key(|(_, l)| (**l as i32 - v as i32).abs())
            .unwrap()
            .0
    };
    let (ri, gi, bi) = (nearest(r), nearest(g), nearest(b));
    let cube = Rgb(LEVELS[ri], LEVELS[gi], LEVELS[bi]);
    let avg = (r as u32 + g as u32 + b as u32) / 3;
    let gi_ = ((avg.saturating_sub(8)) / 10).min(23) as u8;
    let gray = 8 + gi_ * 10;
    let dist = |c: Rgb| {
        let d = |a: u8, b: u8| (a as i32 - b as i32).pow(2);
        d(c.0, r) + d(c.1, g) + d(c.2, b)
    };
    if dist(Rgb(gray, gray, gray)) < dist(cube) {
        232 + gi_
    } else {
        16 + 36 * ri as u8 + 6 * gi as u8 + bi as u8
    }
}

/// Rewrites every RGB colour in the buffer to its 256-colour equivalent.
pub fn downgrade(buf: &mut ratatui::buffer::Buffer) {
    let map = |c: Color| match c {
        Color::Rgb(r, g, b) => Color::Indexed(to_ansi256(Rgb(r, g, b))),
        c => c,
    };
    for cell in buf.content.iter_mut() {
        cell.fg = map(cell.fg);
        cell.bg = map(cell.bg);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ansi256_mapping() {
        assert_eq!(to_ansi256(Rgb(0, 0, 0)), 16);
        assert_eq!(to_ansi256(Rgb(255, 0, 0)), 196);
        assert_eq!(to_ansi256(Rgb(255, 255, 255)), 231);
        assert_eq!(to_ansi256(Rgb(128, 128, 128)), 244);
    }

    #[test]
    fn gradients() {
        let s = [Rgb(0, 0, 0), Rgb(100, 100, 100), Rgb(200, 0, 0)];
        assert_eq!(Rgb::gradient(&s, 0.0), Rgb(0, 0, 0));
        assert_eq!(Rgb::gradient(&s, 0.5), Rgb(100, 100, 100));
        assert_eq!(Rgb::gradient(&s, 1.0), Rgb(200, 0, 0));
        for name in THEMES {
            assert_eq!(Theme::by_name(name).name, name);
        }
    }
}
