use ratatui::{
    Frame,
    buffer::Buffer,
    layout::{Constraint, Flex, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

use super::{
    bar, fmt_secs,
    fx::{self, Light, Pixels, Scene},
    hints,
    home::sparkline,
    key_label, panel,
    theme::Rgb,
};
use crate::{
    app::{App, Results},
    config::Animations,
};

/// How a drill went, which decides how the screen reacts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Verdict {
    /// A personal best: the sky lights up.
    Best,
    /// Accurate and at least as fast as usual.
    Clean,
    Solid,
    /// Accurate but well below the usual pace.
    Slow,
    /// Under 90% accuracy: the colour drains out and the neon sputters.
    Sloppy,
}

impl Verdict {
    fn of(r: &Results, avg: Option<f64>) -> Self {
        let avg = avg.unwrap_or(r.wpm);
        if r.accuracy < 0.9 {
            Verdict::Sloppy
        } else if r.is_pb() {
            Verdict::Best
        } else if r.accuracy >= 0.97 && r.wpm >= avg {
            Verdict::Clean
        } else if r.wpm < avg * 0.85 {
            Verdict::Slow
        } else {
            Verdict::Solid
        }
    }

    fn line(self, r: &Results) -> String {
        let pick = |opts: [&str; 3]| opts[(r.wpm * 10.0) as usize % 3].to_string();
        match self {
            Verdict::Best => pick([
                "faster than you have ever been here.",
                "a new high-water mark.",
                "you just outran yourself.",
            ]),
            Verdict::Clean => pick([
                "clean and quick. that's the rhythm.",
                "crisp. barely a stray key.",
                "smooth. keep that cadence.",
            ]),
            Verdict::Solid => pick([
                "solid run. steady wins.",
                "good work. tighten up the misses next.",
                "on track. again?",
            ]),
            Verdict::Slow => pick([
                "slower than your usual. shake it off.",
                "a step back. find the rhythm again.",
                "heavy fingers. go again.",
            ]),
            Verdict::Sloppy => match (r.wpm * 10.0) as usize % 3 {
                0 => format!(
                    "{:.0}% accuracy. that's guessing, not typing.",
                    r.accuracy * 100.0
                ),
                1 => "too many misses. slow down until it's clean.".into(),
                _ => "speed means nothing at this accuracy.".into(),
            },
        }
    }
}

/// Speeds of earlier drills in the same mode and layout (oldest first),
/// including this one as the last entry.
fn mode_history(app: &App, r: &Results) -> Vec<f64> {
    let mode_id = r.mode.id(app.config.test_seconds);
    app.profile
        .history
        .iter()
        .filter(|h| h.layout == app.layout.id && h.mode == mode_id)
        .map(|h| h.wpm)
        .collect()
}

pub fn draw(f: &mut Frame, area: Rect, app: &App, scene: &mut Scene) {
    let theme = &app.theme;
    let Some(r) = &app.results else { return };
    let anim = app.animations() != Animations::Off;
    let age = if anim {
        r.at.elapsed().as_secs_f64()
    } else {
        10.0
    };
    let hist = mode_history(app, r);
    let avg = (hist.len() > 1)
        .then(|| hist[..hist.len() - 1].iter().sum::<f64>() / (hist.len() - 1) as f64);
    let verdict = Verdict::of(r, avg);
    let big = area.height >= 30 && area.width >= 60;
    let unlock_h = if r.unlocked.is_some() { 3 } else { 0 };

    let [
        top,
        _,
        hero,
        said,
        stats_row,
        unlock,
        _,
        chart,
        _,
        bottom,
        footer,
    ] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(if big { fx::FONT_H + 1 } else { 1 }),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(unlock_h),
        Constraint::Length(1),
        Constraint::Fill(1),
        Constraint::Length(1),
        Constraint::Length(if area.height >= 24 { 8 } else { 0 }),
        Constraint::Length(1),
    ])
    .areas(area);

    // Title, or the personal-best banner typing itself in, letters glowing
    // white-hot and cooling into the gradient.
    if verdict == Verdict::Best {
        let text = "✦  NEW PERSONAL BEST  ✦";
        let n = text.chars().count();
        let x0 = top.x + top.width.saturating_sub(n as u16) / 2;
        let hot = Rgb(255, 255, 255).lerp(theme.accent3, 0.3);
        for (i, ch) in text.chars().enumerate() {
            let at = 0.5 + i as f64 * 0.035;
            if age < at {
                continue;
            }
            let base = Rgb::gradient(&theme.gradient, i as f64 / n as f64);
            let col = hot.lerp(base, fx::ease_out((age - at) / 0.5));
            put_str(
                f.buffer_mut(),
                x0 + i as u16,
                top.y,
                &ch.to_string(),
                theme.bold(col),
            );
        }
    } else {
        let mut s = vec![Span::styled(r.mode.label(), theme.bold(theme.accent2))];
        if !r.title.is_empty() {
            s.push(Span::styled(format!(" · {}", r.title), theme.dim()));
        }
        match r.prev_best {
            None => s.push(Span::styled("  · first run", theme.fg(theme.accent3))),
            Some(b) => s.push(Span::styled(format!("  · best {b:.0} wpm"), theme.dim())),
        }
        f.render_widget(Line::from(s).centered(), top);
    }

    let center = draw_hero(f.buffer_mut(), hero, app, r, verdict, age, big);

    // The verdict, in words. A bad one arrives with a shake.
    if age > 0.75 {
        let msg = verdict.line(r);
        let col = match verdict {
            Verdict::Sloppy => theme.bad,
            Verdict::Slow => theme.warn,
            Verdict::Best | Verdict::Clean => theme.fg,
            Verdict::Solid => theme.dim,
        };
        let k = fx::ease_out((age - 0.75) / 0.4);
        let jolt = if verdict == Verdict::Sloppy {
            (fx::wobble(age - 0.75, 8.0, 7.0) * 3.0).round() as i32
        } else {
            0
        };
        let w = msg.chars().count() as i32;
        let x = said.x as i32 + (said.width as i32 - w) / 2 + jolt;
        let style = if verdict == Verdict::Sloppy {
            theme.bold(col)
        } else {
            theme.fg(theme.fade(col, k)).add_modifier(Modifier::ITALIC)
        };
        put_str(f.buffer_mut(), x.max(0) as u16, said.y, &msg, style);
    }

    // The numbers, arriving one after another.
    let stat = |label: &'static str, value: String, c: Rgb| (value, label, c);
    let acc_col = if r.accuracy >= 0.97 {
        theme.good
    } else if r.accuracy >= 0.9 {
        theme.fg
    } else {
        theme.bad
    };
    let stats = [
        stat("accuracy", format!("{:.1}%", r.accuracy * 100.0), acc_col),
        stat(
            "errors",
            format!("{}", r.errors),
            if r.errors == 0 { theme.good } else { theme.fg },
        ),
        stat("raw", format!("{:.0}", r.raw_wpm), theme.fg),
        stat("consistency", format!("{:.0}%", r.consistency), theme.fg),
        stat("time", fmt_secs(r.secs), theme.fg),
        stat("max combo", format!("×{}", r.max_combo), theme.accent3),
    ];
    let spans: Vec<Span> = stats
        .iter()
        .enumerate()
        .flat_map(|(i, (value, label, c))| {
            let k = fx::ease_out((age - 0.35 - i as f64 * 0.07) / 0.3);
            vec![
                Span::styled(value.clone(), theme.bold(theme.fade(*c, k))),
                Span::styled(format!(" {label}   "), theme.fg(theme.fade(theme.dim, k))),
            ]
        })
        .collect();
    f.render_widget(Line::from(spans).centered(), stats_row);

    if let Some(k) = r.unlocked {
        draw_unlock(f.buffer_mut(), unlock, app, k, age, scene);
    }

    draw_chart(f, chart, app, age);
    if bottom.height > 0 {
        draw_bottom(f, bottom, app, age);
    }
    f.render_widget(
        hints(
            &[
                ("enter", "next drill"),
                ("r", "retry"),
                ("s", "stats"),
                ("esc", "menu"),
            ],
            theme,
        ),
        footer,
    );

    // How the room reacts.
    if let Some((cx, cy)) = center {
        match verdict {
            Verdict::Sloppy => {}
            Verdict::Slow => {}
            _ => {
                let rings = if verdict == Verdict::Best { 3 } else { 1 };
                for i in 0..rings {
                    let a = age - 0.8 - i as f64 * 0.25;
                    if (0.0..1.6).contains(&a) {
                        scene.lights.push(Light {
                            ring: a * 4.5,
                            ..Light::blob(
                                cx,
                                cy,
                                10.0,
                                3.2,
                                Rgb::gradient(&theme.gradient, i as f64 / 2.0),
                                0.3 * (1.0 - a / 1.6),
                            )
                        });
                    }
                }
            }
        }
    }
    match verdict {
        // Both are full-screen changes, so they move in coarse steps.
        Verdict::Best => {
            let rise = (fx::smoothstep(0.6, 2.0, age) * 6.0).ceil() / 6.0;
            scene.aurora = Some((0.6 * rise, (app.t * 8.0).floor() / 8.0));
        }
        Verdict::Sloppy => {
            scene.drain = 0.75 * (fx::smoothstep(0.5, 1.6, age) * 8.0).ceil() / 8.0;
        }
        _ => {}
    }
}

fn put_str(buf: &mut Buffer, x: u16, y: u16, s: &str, style: Style) {
    let a = buf.area;
    if y >= a.bottom() {
        return;
    }
    for (cx, ch) in (x..).zip(s.chars()) {
        if cx >= a.right() {
            break;
        }
        buf[(cx, y)].set_char(ch).set_style(style);
    }
}

/// The speed in big digits that gather out of scattered dust. Good runs get
/// a glint of light across them; sloppy ones sputter like failing neon.
/// Returns the number's centre for the lighting.
fn draw_hero(
    buf: &mut Buffer,
    hero: Rect,
    app: &App,
    r: &Results,
    verdict: Verdict,
    age: f64,
    big: bool,
) -> Option<(f64, f64)> {
    let theme = &app.theme;
    let num = format!("{:.0}", r.wpm);
    if !big {
        let line = Line::from(vec![
            Span::styled(num, theme.bold(theme.accent)),
            Span::styled(" wpm", theme.dim()),
        ])
        .centered();
        ratatui::widgets::Widget::render(line, hero, buf);
        return Some((hero.x as f64 + hero.width as f64 / 2.0, hero.y as f64 + 0.5));
    }
    let nw = fx::big_width(&num);
    let w = nw + 5;
    let x = hero.x + hero.width.saturating_sub(w) / 2;
    let y = hero.y;
    let sloppy = verdict == Verdict::Sloppy;

    // Failing neon: stutters hard at first, then the odd dropout.
    let still = app.animations() == Animations::Off;
    let frame = (app.t * 18.0) as u32;
    let lit = if still || !sloppy || age < 0.7 {
        1.0
    } else if age < 2.4 {
        if fx::hash(frame, 7) % 100 < 38 {
            0.22
        } else {
            1.0
        }
    } else if fx::hash(frame, 11) % 1000 < 25 {
        0.35
    } else {
        1.0
    };
    let glint = if !still && matches!(verdict, Verdict::Best | Verdict::Clean) && age > 1.0 {
        Some(((age - 1.0) % 3.2) / 0.7 * (nw as f64 + 12.0) - 6.0)
    } else {
        None
    };

    let cells = fx::big_cells(&num);
    let mut px = Pixels::new(Rect {
        x: 0,
        y: 0,
        width: buf.area.right(),
        height: buf.area.bottom(),
    });
    let mut landed = Vec::new();
    for (k, (dx, dy)) in cells.iter().enumerate() {
        let h = fx::hash(k as u32, 77);
        let r1 = (h & 0x3ff) as f64 / 1023.0;
        let r2 = ((h >> 10) & 0x3ff) as f64 / 1023.0;
        let r3 = ((h >> 20) & 0x3ff) as f64 / 1023.0;
        let p = fx::clamp01((age - r3 * 0.3) / 0.5);
        let (tx, ty) = ((x + dx) as f64, (y + dy) as f64);
        let mut col = if sloppy {
            theme.dim.lerp(theme.bad, 0.35)
        } else {
            Rgb::gradient(&theme.gradient, *dx as f64 / nw as f64)
        };
        if p >= 1.0 {
            if let Some(g) = glint {
                let d = *dx as f64 + *dy as f64 * 0.7 - g;
                col = col.lerp(Rgb(255, 255, 255), 0.75 * (-d * d / 2.5).exp());
            }
            landed.push((tx as u16, ty as u16, theme.fade(col, lit)));
            continue;
        }
        // In flight: a single pixel of dust curving in toward its place.
        let e = fx::ease_out(p);
        let sx = tx + (r1 - 0.5) * 70.0;
        let sy = ty + (r2 - 0.5) * 18.0;
        let fx_ = sx + (tx - sx) * e;
        let fy = sy + (ty - sy) * e;
        px.set(
            fx_.round() as i32,
            (fy * 2.0).round() as i32,
            theme.fade(col, 0.3 + 0.7 * p),
        );
    }
    // Dust renders onto empty cells only; it must not scribble over text.
    for cy in buf.area.top()..buf.area.bottom() {
        for cx in buf.area.left()..buf.area.right() {
            let (top, bottom) = (
                px.get(cx as i32, cy as i32 * 2),
                px.get(cx as i32, cy as i32 * 2 + 1),
            );
            if top.is_none() && bottom.is_none() {
                continue;
            }
            let cell = &mut buf[(cx, cy)];
            if cell.symbol() != " " {
                continue;
            }
            match (top, bottom) {
                (Some(t), _) => {
                    cell.set_char('▀').set_fg(t.c());
                }
                (None, Some(b)) => {
                    cell.set_char('▄').set_fg(b.c());
                }
                _ => {}
            }
        }
    }
    // A soft shadow under the landed digits, then the digits.
    let shadow = theme.fade(if sloppy { theme.faint } else { theme.accent }, 0.22 * lit);
    for (cx, cy, _) in &landed {
        put_str(buf, cx + 1, cy + 1, "█", theme.fg(shadow));
    }
    for (cx, cy, col) in &landed {
        put_str(buf, *cx, *cy, "█", theme.fg(*col));
    }
    if age > 0.5 {
        let label_x = x + nw + 2;
        put_str(
            buf,
            label_x,
            y + fx::FONT_H - 1,
            "wpm",
            theme.bold(theme.dim),
        );
    }
    Some((
        x as f64 + nw as f64 / 2.0,
        y as f64 + fx::FONT_H as f64 / 2.0,
    ))
}

/// The newly unlocked key as a keycap that pops in with a flash.
fn draw_unlock(buf: &mut Buffer, area: Rect, app: &App, k: char, age: f64, scene: &mut Scene) {
    let theme = &app.theme;
    if area.height < 3 {
        return;
    }
    let before = "new key unlocked   ";
    let finger = app.layout.finger_of(k).map_or("", |f| f.name());
    let after = format!("   {finger} finger");
    let cap_w: u16 = 7;
    let total = before.chars().count() as u16 + cap_w + after.chars().count() as u16;
    let x = area.x + area.width.saturating_sub(total) / 2;
    let cx = x + before.chars().count() as u16;
    let y = area.y;
    let show = fx::clamp01((age - 0.9) / 0.25);
    if show <= 0.0 {
        return;
    }
    put_str(
        buf,
        x,
        y + 1,
        before,
        theme.bold(theme.fade(theme.accent3, show)),
    );
    put_str(
        buf,
        cx + cap_w,
        y + 1,
        &after,
        theme.fg(theme.fade(theme.dim, show)),
    );
    let pulse = (app.t * 4.0).sin() * 0.5 + 0.5;
    let flash = 1.0 - fx::clamp01((age - 0.9) / 0.45);
    let face = theme
        .accent
        .lerp(theme.accent3, pulse * 0.4)
        .lerp(Rgb(255, 255, 255), flash);
    let lip = face.lerp(Rgb(0, 0, 0), 0.45);
    let inner = (cap_w - 2) as usize;
    put_str(
        buf,
        cx,
        y,
        &format!("▗{}▖", "▄".repeat(inner)),
        theme.fg(face),
    );
    put_str(buf, cx, y + 1, "▐", theme.fg(face));
    put_str(
        buf,
        cx + 1,
        y + 1,
        &format!("{:^inner$}", k),
        Style::new()
            .fg(theme.bg.c())
            .bg(face.c())
            .add_modifier(Modifier::BOLD),
    );
    put_str(buf, cx + cap_w - 1, y + 1, "▌", theme.fg(face));
    put_str(
        buf,
        cx,
        y + 2,
        &format!("▝{}▘", "▀".repeat(inner)),
        theme.fg(lip),
    );
    let a = age - 0.9;
    if (0.0..1.2).contains(&a) {
        scene.lights.push(Light {
            ring: a * 4.0,
            ..Light::blob(
                cx as f64 + cap_w as f64 / 2.0,
                y as f64 + 1.5,
                6.0,
                2.0,
                theme.accent3,
                0.35 * (1.0 - a / 1.2),
            )
        });
    }
}

/// Speed over the drill as a filled area that draws itself from left to
/// right; mistakes are marked in red.
fn draw_chart(f: &mut Frame, area: Rect, app: &App, age: f64) {
    let theme = &app.theme;
    let Some(r) = &app.results else { return };
    if area.height < 5 || r.timeline.len() < 2 {
        return;
    }
    let area = super::center(area, area.width.min(100), area.height);
    let max_x = r.timeline.last().map_or(1.0, |p| p.0).max(2.0);
    let title = format!("speed · {max_x:.0}s");
    let block = panel(&title, theme);
    let inner = block.inner(area);
    f.render_widget(block, area);
    let max_y = r.timeline.iter().map(|p| p.1.max(p.2)).fold(10.0, f64::max);
    let max_y = (max_y / 20.0).ceil() * 20.0;

    let gutter = 4;
    if inner.width <= gutter + 4 || inner.height < 2 {
        return;
    }
    let plot = Rect {
        x: inner.x + gutter,
        width: inner.width - gutter,
        ..inner
    };
    let labels = [
        (0, format!("{max_y:>3.0}")),
        (inner.height / 2, format!("{:>3.0}", max_y / 2.0)),
        (inner.height - 1, "  0".to_string()),
    ];
    for (dy, s) in labels {
        put_str(f.buffer_mut(), inner.x, inner.y + dy, &s, theme.faint());
    }

    let mut px = Pixels::new(plot);
    let (w, h) = (px.width(), px.height());
    let sample = |t: f64, which: usize| -> f64 {
        let tl = &r.timeline;
        let i = tl.partition_point(|p| p.0 < t).clamp(1, tl.len() - 1);
        let (a, b) = (&tl[i - 1], &tl[i]);
        let va = if which == 1 { a.1 } else { a.2 };
        let vb = if which == 1 { b.1 } else { b.2 };
        let k = if b.0 > a.0 {
            (t - a.0) / (b.0 - a.0)
        } else {
            1.0
        };
        va + (vb - va) * k.clamp(0.0, 1.0)
    };
    let to_y = |v: f64| ((h - 1) as f64 * (1.0 - v / max_y)).round() as i32;
    let reveal = (fx::ease_out((age - 0.2) / 1.1) * w as f64) as usize;
    let time_of = |x: usize| 1.0 + (max_x - 1.0) * x as f64 / (w - 1).max(1) as f64;
    for x in 0..w.min(reveal + 1) {
        let t = time_of(x);
        let y = to_y(sample(t, 1));
        let ry = to_y(sample(t, 2));
        let col = Rgb::gradient(&theme.gradient, 1.0 - y as f64 / h as f64);
        for yy in y.max(0)..h as i32 {
            let depth = (yy - y) as f64 / (h as i32 - y).max(1) as f64;
            let k = 0.08 + 0.5 * (1.0 - depth).powf(1.6);
            px.set(x as i32, yy, theme.panel.lerp(col, k));
        }
        if ry < y {
            px.set(x as i32, ry, theme.panel.lerp(theme.dim, 0.5));
        }
        let head = if x + 1 >= reveal && reveal < w {
            1.0
        } else {
            0.0
        };
        px.set(x as i32, y, col.lerp(theme.fg, 0.25 + 0.6 * head));
    }
    // Mistakes: a red tick on the curve.
    for (t, wpm, _, errs) in &r.timeline {
        if *errs == 0 {
            continue;
        }
        let x = ((t - 1.0) / (max_x - 1.0).max(1e-9) * (w - 1) as f64).round() as usize;
        if x > reveal {
            continue;
        }
        let y = to_y(*wpm);
        px.set(x as i32, y - 1, theme.bad);
        px.set(x as i32, y - 2, theme.bad);
    }
    px.render(f.buffer_mut(), plot);
}

fn draw_bottom(f: &mut Frame, area: Rect, app: &App, age: f64) {
    let theme = &app.theme;
    let Some(r) = &app.results else { return };
    let w = area.width.min(100);
    let [left, right] =
        Layout::horizontal([Constraint::Length(w / 2), Constraint::Length(w - w / 2)])
            .flex(Flex::Center)
            .areas(area);

    // Missed keys and trouble words.
    let mut lines = Vec::new();
    if r.key_errors.is_empty() {
        lines.push(Line::styled(
            " flawless — no missed keys",
            theme.fg(theme.good),
        ));
    } else {
        let mut spans = vec![Span::styled(" keys  ", theme.dim())];
        for (c, n) in r.key_errors.iter().take(8) {
            spans.push(Span::styled(
                format!(" {} ", key_label(*c)),
                // Pure red survives the colour drain of a sloppy run.
                Style::new()
                    .bg(theme.bad.c())
                    .fg(theme.bg.c())
                    .add_modifier(Modifier::BOLD),
            ));
            spans.push(Span::styled(format!("{n} "), theme.dim()));
        }
        lines.push(Line::from(spans));
    }
    if !r.trouble_words.is_empty() {
        lines.push(Line::raw(""));
        lines.push(Line::styled(" queued for review", theme.dim()));
        let words: Vec<String> = r.trouble_words.iter().take(6).cloned().collect();
        lines.push(Line::styled(
            format!(" {}", words.join("  ")),
            theme.fg(theme.accent3),
        ));
    }
    let lb = panel("mistakes", theme);
    f.render_widget(Paragraph::new(lines).block(lb), left);

    // Recent trend and the daily goal filling up.
    let inner_w = right.width.saturating_sub(4) as usize;
    let mode_id = r.mode.id(app.config.test_seconds);
    let hist: Vec<f64> = app
        .profile
        .history
        .iter()
        .filter(|h| h.layout == app.layout.id && h.mode == mode_id)
        .map(|h| h.wpm)
        .collect();
    let spark: Vec<f64> = hist.iter().rev().take(inner_w).rev().copied().collect();
    let avg = if hist.len() > 1 {
        hist[..hist.len() - 1].iter().sum::<f64>() / (hist.len() - 1) as f64
    } else {
        r.wpm
    };
    let delta = r.wpm - avg;
    let goal = app.config.daily_goal_secs();
    let now = app.profile.today_secs();
    let fill_t = if app.animations() == Animations::Off {
        1.0
    } else {
        ((age - 0.3) / 1.0).clamp(0.0, 1.0)
    };
    let shown = r.goal_before + (now - r.goal_before) * fill_t;
    let (filled, empty) = bar(shown / goal, right.width.saturating_sub(16));
    let reached = shown >= goal;
    let streak = app.profile.streak(goal);

    let lines = vec![
        Line::from([vec![Span::raw(" ")], sparkline(&spark, theme)].concat()),
        Line::from(vec![
            Span::styled(
                format!(" {:+.0} wpm", delta),
                theme.bold(if delta >= 0.0 { theme.good } else { theme.warn }),
            ),
            Span::styled(" vs your average", theme.dim()),
        ]),
        Line::raw(""),
        Line::from(vec![
            Span::styled(" goal ", theme.dim()),
            Span::styled(
                filled,
                theme.fg(if reached { theme.good } else { theme.accent }),
            ),
            Span::styled(empty, Style::new().bg(theme.faint.c())),
            Span::styled(
                format!(
                    " {:.0}/{}m",
                    (shown / 60.0).floor(),
                    app.config.daily_goal_minutes
                ),
                theme.fg(theme.fg),
            ),
        ]),
        Line::from(vec![
            Span::styled(" streak ", theme.dim()),
            Span::styled(format!("✦ {streak}"), theme.bold(theme.accent3)),
            Span::styled(
                if reached && r.goal_before < goal {
                    "  goal reached today!"
                } else {
                    ""
                },
                theme.fg(theme.good),
            ),
        ]),
    ];
    f.render_widget(Paragraph::new(lines).block(panel("progress", theme)), right);
}
