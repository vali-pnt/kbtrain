use std::time::Instant;

use ratatui::{
    Frame,
    buffer::Buffer,
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
};

use super::{
    fmt_secs,
    fx::{self, Light, Scene, TIERS},
    gradient_line, hints, keyboard, pill,
    theme::{Rgb, Theme},
};
use crate::{
    app::App,
    config::{Animations, KeyboardMode},
    layout::NUMBER_ROW,
    trainer::Mode,
};

pub fn draw(f: &mut Frame, area: Rect, app: &App, scene: &mut Scene) {
    let theme = &app.theme;
    let code = app.drill.code;
    let show_bar = matches!(app.drill.mode, Mode::Learn | Mode::Review | Mode::Ngrams)
        && app.config.progressive_unlock;

    let numbers = app.drill.text.chars().any(|c| {
        app.layout
            .locate(c)
            .is_some_and(|(k, _)| k.row == NUMBER_ROW)
    });
    // Prose breathes: a spare row under each line holds the caret trail.
    // It gives way when the room is needed for a bigger keyboard.
    let dense_rows: u16 = if code { 7 } else { 3 };
    let (mut kb_kind, mut kb_h) = keyboard_fit(app, area, numbers, dense_rows);
    let mut spaced = false;
    if !code && area.height >= 18 {
        let (k, h) = keyboard_fit(app, area, numbers, 6);
        if k == kb_kind {
            (kb_kind, kb_h, spaced) = (k, h, true);
        }
    }
    let text_rows = if spaced { 6 } else { dense_rows };

    let [
        header,
        _,
        bar_area,
        _,
        text_area,
        progress,
        combo,
        _,
        kb,
        footer,
    ] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(u16::from(show_bar)),
        Constraint::Fill(1),
        Constraint::Length(text_rows),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Fill(1),
        Constraint::Length(kb_h),
        Constraint::Length(1),
    ])
    .areas(area);

    draw_header(f, header, app);
    if show_bar {
        draw_unlock_bar(f, bar_area, app);
    }
    let mut col_w = text_area
        .width
        .saturating_sub(4)
        .min(if code { 96 } else { 76 });
    if code {
        let widest = app
            .drill
            .text
            .lines()
            .map(|l| l.chars().count() as u16 + 2)
            .max()
            .unwrap_or(0);
        col_w = col_w.min(widest.max(20));
    }
    let col = Rect {
        x: text_area.x + (text_area.width - col_w) / 2,
        width: col_w,
        ..text_area
    };
    let caret = draw_text(f.buffer_mut(), col, app, spaced);
    let rail = Rect {
        x: col.x,
        width: col.width,
        ..progress
    };
    draw_progress(f.buffer_mut(), rail, app, caret.map(|c| c.0));
    draw_combo(f.buffer_mut(), combo, app);
    if let Some(c) = caret {
        light_scene(scene, app, c);
    }

    let allowed = show_bar.then(|| app.allowed_keys());
    let opts = keyboard::Opts {
        next: app.session.expected(),
        alpha: if app.config.keyboard == KeyboardMode::Adaptive {
            app.kb_alpha
        } else {
            1.0
        },
        ripples: &app.ripples,
        allowed: allowed.as_deref(),
        numbers: kb_kind == Kb::LargeNumbers || (kb_kind == Kb::Compact && numbers),
        last_miss: app.fb.last_miss(),
    };
    match kb_kind {
        Kb::Large | Kb::LargeNumbers => keyboard::draw_large(f.buffer_mut(), kb, app, &opts),
        Kb::Compact => keyboard::draw_compact(f.buffer_mut(), kb, app, &opts),
        Kb::None => {}
    }

    let new = if code { "ctrl+n" } else { "tab" };
    f.render_widget(
        hints(
            &[
                (new, "new"),
                ("ctrl+r", "restart"),
                ("ctrl+k", "keyboard"),
                ("esc", "menu"),
            ],
            theme,
        ),
        footer,
    );
}

/// The colour of a combo tier, from cool to hot along the theme gradient.
fn tier_color(theme: &Theme, tier: usize) -> Rgb {
    if tier == 0 {
        theme.accent
    } else {
        Rgb::gradient(
            &theme.gradient,
            (tier - 1) as f64 / (TIERS.len() - 2) as f64,
        )
    }
}

/// Lights the room around the caret. It warms as the combo climbs, flares
/// at each new tier, and a mistake bleeds red in from the edges.
fn light_scene(scene: &mut Scene, app: &App, (x, y): (u16, u16)) {
    let theme = &app.theme;
    let (cx, cy) = (x as f64 + 0.5, y as f64 + 0.5);
    let tier = fx::tier_of(app.session.combo);
    // The glow follows the caret in four-cell steps: the eye can't tell, and
    // the terminal has far fewer cells to repaint per keystroke.
    scene.lights.push(Light::blob(
        (x / 4 * 4) as f64 + 2.0,
        cy,
        16.0 + 4.0 * tier as f64,
        3.5 + tier as f64,
        tier_color(theme, tier),
        0.05 + 0.035 * tier as f64,
    ));
    if let Some((t, at)) = app.fb.tier_up {
        let age = (at.elapsed().as_secs_f64() * 15.0).floor() / 15.0;
        if age < 1.0 {
            scene.lights.push(Light {
                ring: age * 7.0,
                ..Light::blob(cx, cy, 7.0, 2.2, tier_color(theme, t), 0.25 * (1.0 - age))
            });
        }
    }
    if let Some(age) = app.fb.last_miss()
        && age < 0.6
    {
        let k = ((1.0 - age / 0.6).powi(2) * 5.0).ceil() / 5.0;
        scene.edge = Some((theme.bad, 0.45 * k));
        scene
            .lights
            .push(Light::blob(cx, cy, 6.0, 1.6, theme.bad, 0.35 * k));
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kb {
    LargeNumbers,
    Large,
    Compact,
    None,
}

/// Picks the biggest keyboard that fits alongside the text.
fn keyboard_fit(app: &App, area: Rect, numbers: bool, text_rows: u16) -> (Kb, u16) {
    let chrome = 1 + 1 + 1 + text_rows + 2 + 1 + 2;
    let room = area.height.saturating_sub(chrome);
    let wide = area.width >= keyboard::large_width(4);
    match app.config.keyboard {
        KeyboardMode::Hidden => (Kb::None, 0),
        KeyboardMode::Compact => compact_fit(room, numbers),
        KeyboardMode::Large | KeyboardMode::Adaptive => {
            if wide && numbers && room >= keyboard::large_height(true) {
                (Kb::LargeNumbers, keyboard::large_height(true))
            } else if wide && room >= keyboard::large_height(false) {
                (Kb::Large, keyboard::large_height(false))
            } else {
                compact_fit(room, numbers)
            }
        }
    }
}

fn compact_fit(room: u16, numbers: bool) -> (Kb, u16) {
    let h = keyboard::compact_height(numbers);
    if room >= h {
        (Kb::Compact, h)
    } else {
        (Kb::None, 0)
    }
}

fn draw_header(f: &mut Frame, area: Rect, app: &App) {
    let theme = &app.theme;
    let s = &app.session;
    let mut left = gradient_line(" KBTRAIN ", theme, app.t * 0.1).spans;
    left.push(Span::styled(
        format!(" {} ", app.drill.mode.label()),
        theme.bold(theme.fg),
    ));
    if !app.drill.title.is_empty() {
        left.push(Span::styled(format!("· {} ", app.drill.title), theme.dim()));
    }
    left.push(Span::styled(
        format!("· {}", app.layout.name),
        theme.faint(),
    ));

    let mut right: Vec<Span> = Vec::new();
    if app.config.live_stats && s.is_started() {
        right.push(Span::styled(
            format!("{:.0}", s.wpm()),
            theme.bold(theme.accent),
        ));
        right.push(Span::styled(" wpm  ", theme.dim()));
        let acc = s.accuracy();
        right.push(Span::styled(
            format!("{:.1}%", acc * 100.0),
            theme.fg(if acc < 0.9 { theme.bad } else { theme.fg }),
        ));
        right.push(Span::styled(" acc  ", theme.dim()));
    }
    match s.remaining() {
        Some(r) => right.push(Span::styled(
            format!("{} left ", fmt_secs(r.ceil())),
            theme.bold(if r < 5.0 { theme.bad } else { theme.accent3 }),
        )),
        None => right.push(Span::styled(
            format!("{} ", fmt_secs(s.elapsed())),
            theme.dim(),
        )),
    }
    let rw: u16 = right.iter().map(|s| s.width() as u16).sum();
    let [l, r] = Layout::horizontal([Constraint::Fill(1), Constraint::Length(rw)]).areas(area);
    f.render_widget(Line::from(left), l);
    f.render_widget(Line::from(right), r);
}

/// Every letter in learning order: unlocked ones coloured by confidence,
/// the focus key on a pill, locked ones dim.
fn draw_unlock_bar(f: &mut Frame, area: Rect, app: &App) {
    let theme = &app.theme;
    let order = app.layout.learning_order();
    let allowed = app.allowed_keys();
    let target = app.target_ms();
    let mut spans: Vec<Span> = Vec::new();
    for c in &order {
        let unlocked = allowed.contains(c);
        let stat = app.lp().key(*c);
        if Some(*c) == app.drill.focus {
            spans.pop();
            spans.extend(pill(c.to_string(), theme.bg, theme.accent, true));
            continue;
        }
        let style = if !unlocked {
            theme.faint()
        } else if stat.is_some_and(|s| s.mastered(target)) {
            theme.fg(theme.good)
        } else {
            let conf = stat.map_or(0.0, |s| s.confidence(target)).min(1.0);
            theme.fg(theme.heat(1.0 - conf))
        };
        spans.push(Span::styled(format!("{c}"), style));
        spans.push(Span::raw(" "));
    }
    if let Some(fk) = app.drill.focus {
        let conf = app
            .lp()
            .key(fk)
            .map_or(0.0, |s| s.confidence(target))
            .min(1.0);
        let (fill, empty) = super::bar(conf, 8);
        spans.push(Span::styled("   focus ", theme.dim()));
        spans.push(Span::styled(format!("{fk} "), theme.bold(theme.accent)));
        spans.push(Span::styled(
            fill,
            Style::new()
                .fg(theme.heat(1.0 - conf).c())
                .bg(theme.faint.c()),
        ));
        spans.push(Span::styled(empty, Style::new().bg(theme.faint.c())));
        spans.push(Span::styled(format!(" {:.0}%", conf * 100.0), theme.dim()));
    }
    f.render_widget(Line::from(spans).centered(), area);
}

/// Splits the target into display lines of at most `width` chars, breaking
/// after spaces (or at newlines). Returns [start, end) ranges.
pub fn wrap(target: &[char], width: usize) -> Vec<(usize, usize)> {
    let width = width.max(2);
    let n = target.len();
    let mut lines = Vec::new();
    let mut s = 0;
    while s < n {
        let mut e = s;
        let mut last_space = None;
        let mut newline = false;
        while e < n && e - s < width {
            if target[e] == '\n' {
                e += 1;
                newline = true;
                break;
            }
            if target[e] == ' ' {
                last_space = Some(e);
            }
            e += 1;
        }
        if newline || e >= n {
            lines.push((s, e));
            s = e;
            continue;
        }
        let end = if target[e] == ' ' || target[e] == '\n' {
            e + 1
        } else if let Some(sp) = last_space {
            sp + 1
        } else {
            e
        };
        lines.push((s, end));
        s = end;
    }
    if lines.is_empty() {
        lines.push((0, 0));
    }
    lines
}

/// Writes one character cell, clipped to the buffer.
fn put(buf: &mut Buffer, x: i32, y: u16, ch: char, style: Style) {
    let a = buf.area;
    if x < a.x as i32 || x >= a.right() as i32 || y < a.y || y >= a.bottom() {
        return;
    }
    buf[(x as u16, y)].set_char(ch).set_style(style);
}

/// The drill text. Fresh letters glow and cool, clean words get a sweep of
/// light, mistakes glitch and shake the whole line. Returns the caret cell.
fn draw_text(buf: &mut Buffer, area: Rect, app: &App, spaced: bool) -> Option<(u16, u16)> {
    let theme = &app.theme;
    let s = &app.session;
    let fb = &app.fb;
    let now = Instant::now();
    let anim = app.animations();
    let full = anim == Animations::Full;
    let moving = anim != Animations::Off;
    let lines = wrap(&s.target, area.width.saturating_sub(1) as usize);
    let cursor = s.cursor();
    let line_of = |i: usize| {
        lines
            .iter()
            .position(|(a, b)| i >= *a && i < *b)
            .unwrap_or(lines.len() - 1)
    };
    let cur_line = line_of(cursor);
    let step: u16 = if spaced { 2 } else { 1 };
    let rows = (area.height / step) as usize;
    let mut first = cur_line.saturating_sub(1);
    if first + rows > lines.len() {
        first = lines.len().saturating_sub(rows);
    }

    // A mistake shakes the text like a head saying no.
    let miss_age = fb.last_miss();
    let shake = match miss_age {
        Some(age) if full && age < 0.45 => (fx::wobble(age, 7.0, 8.0) * 2.4).round() as i32,
        _ => 0,
    };
    let pulse = if moving {
        (app.t * 6.0).sin() * 0.5 + 0.5
    } else {
        1.0
    };
    let stung = s
        .last_blocked
        .is_some_and(|t| t.elapsed().as_millis() < 250)
        || miss_age.is_some_and(|a| a < 0.25);
    let cursor_bg = if stung {
        theme.bad
    } else {
        theme.accent.lerp(theme.accent2, pulse * 0.35)
    };
    let light = theme.is_light();
    let hot = if light {
        theme.accent
    } else {
        Rgb(255, 255, 255).lerp(theme.accent3, 0.35)
    };
    let shine = if light {
        theme.accent2
    } else {
        Rgb(255, 255, 255).lerp(theme.gradient[2], 0.4)
    };
    let focus = app.drill.focus;
    let shown = fb.shown_at.elapsed().as_secs_f64();
    let mut caret = None;

    for (row, (li, &(a, b))) in lines.iter().enumerate().skip(first).take(rows).enumerate() {
        let y = area.y + row as u16 * step;
        let ahead = li > cur_line + 1;
        for (j, i) in (a..b).enumerate() {
            let x = area.x as i32 + j as i32 + shake;
            let t = s.target[i];
            let glyph = if t == '\n' { '↵' } else { t };
            let (mut ch, mut style) = if i < cursor {
                let typed = s.input[i];
                if typed == t {
                    let mut col = if s.ever_wrong[i] {
                        theme.warn
                    } else {
                        theme.fg
                    };
                    let mut bold = false;
                    if moving {
                        if let Some((_, at)) = fb.hits.iter().rev().find(|(k, _)| *k == i) {
                            let age = (now - *at).as_secs_f64();
                            col = hot.lerp(col, fx::ease_out(age / 0.45));
                            bold = age < 0.12;
                        }
                        for (ws, we, at) in &fb.words {
                            if (*ws..*we).contains(&i) {
                                let age = (now - *at).as_secs_f64();
                                let head = age / 0.35 * (we - ws + 4) as f64 - 2.0;
                                let d = (i - ws) as f64 - head;
                                col = col.lerp(shine, 0.9 * (-d * d / 1.3).exp());
                            }
                        }
                    }
                    let mut st = theme.fg(col);
                    if bold {
                        st = st.add_modifier(Modifier::BOLD);
                    }
                    (if t == '\n' { ' ' } else { t }, st)
                } else {
                    let fresh = fb
                        .misses
                        .iter()
                        .rev()
                        .find(|(k, _)| *k == i)
                        .map(|(_, at)| (now - *at).as_secs_f64())
                        .filter(|age| moving && *age < 0.24);
                    match fresh {
                        // Freshly wrong: the letter glitches before settling.
                        Some(age) => (
                            fx::glitch_char(i as u32, age),
                            Style::new()
                                .fg(theme.bg.c())
                                .bg(theme.bad.c())
                                .add_modifier(Modifier::BOLD),
                        ),
                        None => (
                            if t == ' ' || t == '\n' { '·' } else { t },
                            Style::new()
                                .fg(theme.bad.c())
                                .bg(theme.fade(theme.bad, 0.22).c())
                                .add_modifier(Modifier::UNDERLINED),
                        ),
                    }
                }
            } else if i == cursor {
                caret = Some((x.max(0) as u16, y));
                (
                    glyph,
                    Style::new()
                        .bg(cursor_bg.c())
                        .fg(theme.bg.c())
                        .add_modifier(Modifier::BOLD),
                )
            } else {
                let base = if ahead {
                    theme.fade(theme.dim, 0.7)
                } else {
                    theme.dim
                };
                let col = if t == '\n' {
                    theme.faint
                } else if Some(t) == focus {
                    theme.fade(theme.accent2, 0.85)
                } else {
                    base
                };
                (glyph, theme.fg(col))
            };
            // New text washes in from the left.
            if moving && shown < 0.6 {
                let k = fx::clamp01((shown * 160.0 - (j + row * 12) as f64) / 14.0);
                if let Some(ratatui::style::Color::Rgb(r, g, b)) = style.fg {
                    style = style.fg(theme.bg.lerp(Rgb(r, g, b), k).c());
                }
                if k < 0.3 && ch != ' ' && i > cursor {
                    ch = '·';
                }
            }
            put(buf, x, y, ch, style);
        }
        if spaced && li == cur_line {
            caret_trail(buf, app, area, &lines, cursor, y + 1, shake, stung);
        }
    }
    caret
}

/// An elastic underline: the caret's tail lags behind as it jumps and
/// snaps after it, so fast typing stretches it like a comet.
#[allow(clippy::too_many_arguments)]
fn caret_trail(
    buf: &mut Buffer,
    app: &App,
    area: Rect,
    lines: &[(usize, usize)],
    cursor: usize,
    y: u16,
    shake: i32,
    stung: bool,
) {
    let theme = &app.theme;
    let col_of = |i: usize| -> Option<(usize, f64)> {
        let li = lines.iter().position(|(a, b)| i >= *a && i < *b)?;
        Some((li, (i - lines[li].0) as f64))
    };
    let Some((line, head)) = col_of(cursor) else {
        return;
    };
    let (from, at) = app.fb.caret_from;
    let mut tail = head;
    if app.animations() != Animations::Off
        && let Some((fl, fc)) = col_of(from)
        && fl == line
    {
        tail = fc + (head - fc) * fx::ease_out(at.elapsed().as_secs_f64() / 0.22);
    }
    let color = if stung { theme.bad } else { theme.accent };
    let (lo, hi) = (tail.min(head), tail.max(head));
    let span = (hi - lo).max(1.0);
    for c in lo.floor() as i32..=hi.ceil() as i32 {
        // Brightest at the head, thinning toward the tail.
        let k = 1.0 - (head - c as f64).abs() / (span + 1.0);
        let x = area.x as i32 + c + shake;
        put(buf, x, y, '▔', theme.fg(theme.fade(color, 0.25 + 0.75 * k)));
    }
}

/// The progress rail: a gradient line with a bright head. A new combo tier
/// sends a pulse racing outward along it.
fn draw_progress(buf: &mut Buffer, area: Rect, app: &App, caret_x: Option<u16>) {
    let theme = &app.theme;
    let w = area.width as usize;
    if w == 0 {
        return;
    }
    let exact = app.session.progress() * w as f64;
    let done = (exact.floor() as usize).min(w);
    let half = exact - done as f64 >= 0.5;
    let wave = app
        .fb
        .tier_up
        .map(|(t, at)| (t, at.elapsed().as_secs_f64()))
        .filter(|(_, age)| *age < 0.7 && app.animations() == Animations::Full);
    let origin = caret_x.map_or(done as f64, |x| x.saturating_sub(area.x) as f64);
    for i in 0..w {
        let (glyph, mut col) = if i < done {
            let c = Rgb::gradient(&theme.gradient, i as f64 / w as f64);
            let head = 1.0 - ((done - i) as f64 / 6.0).min(1.0);
            ('━', c.lerp(theme.fg, head * 0.6))
        } else if i == done && half {
            ('╸', theme.fg)
        } else {
            ('─', theme.faint)
        };
        if let Some((t, age)) = wave {
            let d = (i as f64 - origin).abs() - age * 90.0;
            let k = (-(d / 4.0).powi(2)).exp() * (1.0 - age / 0.7);
            col = col.lerp(tier_color(theme, t).lerp(theme.fg, 0.4), k);
        }
        put(buf, area.x as i32 + i as i32, area.y, glyph, theme.fg(col));
    }
}

/// The combo meter. A new tier's name decodes itself; a broken combo
/// crumbles and falls; a run of mistakes gets told to slow down.
fn draw_combo(buf: &mut Buffer, area: Rect, app: &App) {
    let theme = &app.theme;
    let s = &app.session;
    let anim = app.animations();

    if let Some((at, n)) = s.combo_broken {
        let age = at.elapsed().as_secs_f64();
        if age < 1.1 {
            let tier = fx::tier_of(n);
            let msg = format!("×{n} {}", TIERS[tier].1.to_uppercase());
            let w = msg.chars().count() as f64;
            let x0 = area.x as f64 + (area.width as f64 - w) / 2.0;
            for (i, ch) in msg.chars().enumerate() {
                if ch == ' ' {
                    continue;
                }
                // A beat of cracked red, then every letter drops.
                let t = if anim == Animations::Full {
                    (age - 0.12).max(0.0)
                } else {
                    0.0
                };
                let h = fx::hash(i as u32, n);
                let r1 = (h & 0xffff) as f64 / 65535.0;
                let r2 = (h >> 16) as f64 / 65535.0;
                let x = x0 + i as f64 + (r1 - 0.5) * 10.0 * t;
                let y = area.y as f64 + (-2.0 - 3.0 * r2) * t + 22.0 * t * t;
                let fade = 1.0 - fx::clamp01(age / 1.1);
                let col = theme.fade(
                    tier_color(theme, tier).lerp(theme.bad, (age * 6.0).min(1.0)),
                    fade,
                );
                let (xi, yi) = (x.round() as i32, y.round());
                if yi < 0.0 || yi >= buf.area.bottom() as f64 {
                    continue;
                }
                let yi = yi as u16;
                let blank = xi >= 0
                    && (xi as u16) < buf.area.right()
                    && buf[(xi as u16, yi)].symbol() == " ";
                if blank || yi == area.y {
                    put(buf, xi, yi, ch, theme.bold(col));
                }
            }
            return;
        }
    }

    if s.combo < TIERS[1].0 {
        let misses = app.fb.recent_misses(3.0);
        if misses >= 3
            && let Some(age) = app.fb.last_miss()
        {
            let msg = "slow down · accuracy first";
            let k = 1.0 - fx::smoothstep(2.2, 3.0, age);
            let jolt = if anim == Animations::Full {
                (fx::wobble(age, 9.0, 14.0) * 1.5).round() as i32
            } else {
                0
            };
            let x0 = area.x as i32 + (area.width as i32 - msg.chars().count() as i32) / 2 + jolt;
            for (i, ch) in msg.chars().enumerate() {
                put(
                    buf,
                    x0 + i as i32,
                    area.y,
                    ch,
                    theme.bold(theme.fade(theme.bad, k)),
                );
            }
        }
        return;
    }
    let tier = fx::tier_of(s.combo);
    let lo = TIERS[tier].0;
    let hi = TIERS.get(tier + 1).map_or(lo * 2, |t| t.0);
    let frac = (s.combo - lo) as f64 / (hi - lo).max(1) as f64;
    let col = tier_color(theme, tier);
    let glow = if anim == Animations::Off {
        1.0
    } else {
        (app.t * 8.0).sin() * 0.15 + 0.85
    };
    let fresh = app
        .fb
        .tier_up
        .filter(|(t, _)| *t == tier)
        .map(|(_, at)| at.elapsed().as_secs_f64())
        .filter(|age| *age < 0.6 && anim != Animations::Off);
    let name: String = TIERS[tier]
        .1
        .to_uppercase()
        .chars()
        .enumerate()
        .map(|(j, c)| match fresh {
            Some(age) if age < 0.08 + j as f64 * 0.035 => fx::glitch_char(j as u32, age),
            _ => c,
        })
        .collect();
    let name_col = match fresh {
        Some(age) => theme.fg.lerp(col, age / 0.6),
        None => col,
    };
    let (fill, empty) = super::bar(frac, 10);
    let line = Line::from(vec![
        Span::styled(format!("×{} ", s.combo), theme.bold(theme.fade(col, glow))),
        Span::styled(format!("{name} "), theme.bold(name_col)),
        Span::styled(fill, Style::new().fg(col.c()).bg(theme.faint.c())),
        Span::styled(empty, Style::new().bg(theme.faint.c())),
    ])
    .centered();
    ratatui::widgets::Widget::render(line, area, buf);
}

#[cfg(test)]
mod tests {
    use super::wrap;

    fn lines(text: &str, w: usize) -> Vec<String> {
        let cs: Vec<char> = text.chars().collect();
        wrap(&cs, w)
            .iter()
            .map(|(a, b)| cs[*a..*b].iter().collect())
            .collect()
    }

    #[test]
    fn wraps_after_spaces() {
        assert_eq!(lines("aaa bbb ccc", 7), vec!["aaa bbb ", "ccc"]);
        assert_eq!(lines("aaa bbb ccc", 4), vec!["aaa ", "bbb ", "ccc"]);
        assert_eq!(lines("abcdefgh", 3), vec!["abc", "def", "gh"]);
    }

    #[test]
    fn wraps_at_newlines_and_covers_all_text() {
        assert_eq!(
            lines("fn x() {\n    y\n}", 40),
            vec!["fn x() {\n", "    y\n", "}"]
        );
        let text = "the quick brown fox jumps over the lazy dog";
        assert_eq!(lines(text, 10).concat(), text);
    }
}
