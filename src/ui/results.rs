use ratatui::{
    Frame,
    layout::{Constraint, Flex, Layout, Rect},
    style::{Modifier, Style},
    symbols::Marker,
    text::{Line, Span},
    widgets::{Axis, Chart, Dataset, GraphType, Paragraph},
};

use super::{
    bar, fmt_secs, fx, gradient_line, hints, home::sparkline, key_label, panel, theme::Rgb,
};
use crate::{app::App, config::Animations};

pub fn draw(f: &mut Frame, area: Rect, app: &App) {
    let theme = &app.theme;
    let Some(r) = &app.results else { return };
    let anim = app.animations() != Animations::Off;
    let age = r.at.elapsed().as_secs_f64();
    let big = area.height >= 30 && area.width >= 60;
    let unlock_h = if r.unlocked.is_some() { 2 } else { 0 };

    let [top, _, hero, stats_row, unlock, _, chart, _, bottom, footer] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(if big { fx::FONT_H + 1 } else { 1 }),
        Constraint::Length(1),
        Constraint::Length(unlock_h),
        Constraint::Length(1),
        Constraint::Fill(1),
        Constraint::Length(1),
        Constraint::Length(if area.height >= 24 { 8 } else { 0 }),
        Constraint::Length(1),
    ])
    .areas(area);

    // Title / personal best banner.
    let title = if r.is_pb() {
        gradient_line(
            "✦  NEW PERSONAL BEST  ✦",
            theme,
            if anim { app.t * 0.6 } else { 0.0 },
        )
        .centered()
    } else {
        let mut s = vec![Span::styled(r.mode.label(), theme.bold(theme.accent2))];
        if !r.title.is_empty() {
            s.push(Span::styled(format!(" · {}", r.title), theme.dim()));
        }
        match r.prev_best {
            None => s.push(Span::styled("  · first run", theme.fg(theme.accent3))),
            Some(b) => s.push(Span::styled(format!("  · best {b:.0} wpm"), theme.dim())),
        }
        Line::from(s).centered()
    };
    f.render_widget(title, top);

    // The big number counts up for a moment.
    let shown = if anim {
        r.wpm * (age / 0.6).min(1.0)
    } else {
        r.wpm
    };
    if big {
        let num = format!("{shown:.0}");
        let w = fx::big_width(&num) + 5;
        let x = hero.x + hero.width.saturating_sub(w) / 2;
        let stops = theme.gradient;
        fx::big_text(
            f.buffer_mut(),
            x,
            hero.y,
            &num,
            |p| Rgb::gradient(&stops, p),
            Some(theme.fade(theme.accent, 0.3)),
        );
        let label_x = x + fx::big_width(&num) + 2;
        f.render_widget(
            Span::styled("wpm", theme.bold(theme.dim)),
            Rect {
                x: label_x,
                y: hero.y + fx::FONT_H - 1,
                width: 3.min(hero.right().saturating_sub(label_x)),
                height: 1,
            },
        );
    } else {
        f.render_widget(
            Line::from(vec![
                Span::styled(format!("{shown:.0}"), theme.bold(theme.accent)),
                Span::styled(" wpm", theme.dim()),
            ])
            .centered(),
            hero,
        );
    }

    let stat = |label: &'static str, value: String, c: Rgb| {
        vec![
            Span::styled(value, theme.bold(c)),
            Span::styled(format!(" {label}   "), theme.dim()),
        ]
    };
    let acc_col = if r.accuracy >= 0.97 {
        theme.good
    } else if r.accuracy >= 0.92 {
        theme.fg
    } else {
        theme.bad
    };
    let spans = [
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
    ]
    .concat();
    f.render_widget(Line::from(spans).centered(), stats_row);

    if let Some(k) = r.unlocked {
        let pulse = if anim {
            (app.t * 4.0).sin() * 0.5 + 0.5
        } else {
            1.0
        };
        let line = Line::from(vec![
            Span::styled("new key unlocked  ", theme.bold(theme.accent3)),
            Span::styled(
                format!(" {k} "),
                Style::new()
                    .bg(theme.accent.lerp(theme.accent3, pulse * 0.5).c())
                    .fg(theme.bg.c())
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!(
                    "  {} finger",
                    app.layout.finger_of(k).map_or("", |f| f.name())
                ),
                theme.dim(),
            ),
        ])
        .centered();
        f.render_widget(
            line,
            Rect {
                y: unlock.y + 1,
                height: 1,
                ..unlock
            },
        );
    }

    draw_chart(f, chart, app);
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
}

fn draw_chart(f: &mut Frame, area: Rect, app: &App) {
    let theme = &app.theme;
    let Some(r) = &app.results else { return };
    if area.height < 5 || r.timeline.len() < 2 {
        return;
    }
    let area = super::center(area, area.width.min(100), area.height);
    let wpm: Vec<(f64, f64)> = r.timeline.iter().map(|(t, w, _, _)| (*t, *w)).collect();
    let raw: Vec<(f64, f64)> = r.timeline.iter().map(|(t, _, raw, _)| (*t, *raw)).collect();
    let errs: Vec<(f64, f64)> = r
        .timeline
        .iter()
        .filter(|e| e.3 > 0)
        .map(|(t, _, raw, _)| (*t, *raw))
        .collect();
    let max_y = raw.iter().chain(&wpm).map(|p| p.1).fold(10.0, f64::max);
    let max_y = (max_y / 20.0).ceil() * 20.0;
    let max_x = r.timeline.last().map_or(1.0, |p| p.0);
    let datasets = vec![
        Dataset::default()
            .marker(Marker::Braille)
            .graph_type(GraphType::Line)
            .style(theme.faint())
            .data(&raw),
        Dataset::default()
            .marker(Marker::Braille)
            .graph_type(GraphType::Line)
            .style(theme.bold(theme.accent))
            .data(&wpm),
        Dataset::default()
            .marker(Marker::Dot)
            .graph_type(GraphType::Scatter)
            .style(theme.fg(theme.bad))
            .data(&errs),
    ];
    let label = |s: String| Span::styled(s, theme.faint());
    let chart = Chart::new(datasets)
        .block(panel("speed", theme))
        .x_axis(
            Axis::default()
                .bounds([1.0, max_x.max(2.0)])
                .style(theme.faint())
                .labels([label("1s".into()), label(format!("{max_x:.0}s"))]),
        )
        .y_axis(
            Axis::default()
                .bounds([0.0, max_y])
                .style(theme.faint())
                .labels([
                    label("0".into()),
                    label(format!("{:.0}", max_y / 2.0)),
                    label(format!("{max_y:.0}")),
                ]),
        );
    f.render_widget(chart, area);
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
                Style::new()
                    .bg(theme.fade(theme.bad, 0.5).c())
                    .fg(theme.fg.c()),
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
