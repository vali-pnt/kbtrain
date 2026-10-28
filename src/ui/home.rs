use ratatui::{
    Frame,
    layout::{Constraint, Flex, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Clear, Paragraph, Wrap},
};

use super::{bar, center, fx, gradient_line, hints, panel, theme::Rgb, theme::Theme};
use crate::{
    app::{App, MENU, MenuItem},
    config::Animations,
    trainer::Mode,
};

const TITLE: &str = "KBTRAIN";
const MENU_W: u16 = 38;
const INFO_W: u16 = 40;

pub fn draw(f: &mut Frame, area: Rect, app: &App) {
    let theme = &app.theme;
    let anim = app.animations() != Animations::Off;
    let t = if anim { app.t } else { 0.0 };
    let big = area.width >= 50 && area.height >= 22;
    let title_h = if big { fx::FONT_H + 2 } else { 2 };
    let panel_h = MENU.len() as u16 + 2;

    // Backdrop: stars in the sky, the sun on the horizon, a grid below.
    let free = area.height.saturating_sub(1 + title_h + panel_h + 3);
    let horizon = if free >= 6 {
        title_h + 1 + free
    } else {
        title_h + 3
    };
    let sky = Rect {
        height: horizon.min(area.height),
        ..area
    };
    stars_or_plain(f, sky, app, t);
    if area.height > title_h + 4 {
        // Start below the tagline so the sun never sits behind text.
        let skip = title_h + 1;
        let bd = Rect {
            y: area.y + skip,
            height: area.height - skip,
            ..area
        };
        fx::synthwave(f.buffer_mut(), bd, theme, t, horizon.saturating_sub(skip));
    }

    // Title.
    if big {
        let w = fx::big_width(TITLE);
        let x = area.x + area.width.saturating_sub(w) / 2;
        let shift = t * 0.15;
        let stops = theme.gradient;
        fx::big_text(
            f.buffer_mut(),
            x,
            area.y + 1,
            TITLE,
            |p| {
                let q = (p + shift).rem_euclid(2.0);
                Rgb::gradient(&stops, if q > 1.0 { 2.0 - q } else { q })
            },
            Some(theme.fade(theme.accent, 0.35)),
        );
        let tag = Line::from(Span::styled(
            "type in the neon",
            theme.fg(theme.dim).add_modifier(Modifier::ITALIC),
        ))
        .centered();
        f.render_widget(
            tag,
            Rect {
                y: area.y + fx::FONT_H + 2,
                height: 1,
                ..area
            },
        );
    } else {
        f.render_widget(
            gradient_line("KBTRAIN", theme, t * 0.15).centered(),
            Rect {
                y: area.y,
                height: 1,
                ..area
            },
        );
    }

    // Panels.
    let top = if free >= 6 {
        area.y + horizon + 1
    } else {
        area.y + title_h + 1
    };
    let avail = area.bottom().saturating_sub(top + 2);
    let ph = panel_h.min(avail);
    if ph < 4 {
        return;
    }
    let wide = area.width >= MENU_W + INFO_W + 4;
    let cols: Vec<Constraint> = if wide {
        vec![Constraint::Length(MENU_W), Constraint::Length(INFO_W)]
    } else {
        vec![Constraint::Length(MENU_W.min(area.width))]
    };
    let row = Rect {
        y: top,
        height: ph,
        ..area
    };
    let rects = Layout::horizontal(cols)
        .flex(Flex::Center)
        .spacing(2)
        .split(row);
    draw_menu(f, rects[0], app, t);
    if wide {
        draw_info(f, rects[1], app);
    }

    // Description of the selected item, then key hints.
    let desc_y = top + ph;
    if desc_y < area.bottom().saturating_sub(1) {
        let d = match MENU[app.menu_sel] {
            MenuItem::Mode(m) => m.description(),
            MenuItem::Stats => {
                "Heatmaps, progress charts, finger analysis and your streak calendar."
            }
            MenuItem::Settings => "Layout, theme, keyboard, error handling, goals and more.",
            MenuItem::Quit => "See you tomorrow. Your streak is waiting.",
        };
        let r = center(
            Rect {
                y: desc_y,
                height: 1,
                ..area
            },
            (MENU_W + INFO_W + 2).min(area.width),
            1,
        );
        f.render_widget(Clear, r);
        f.render_widget(
            Paragraph::new(Line::styled(d, theme.dim()).centered()).style(theme.base()),
            r,
        );
    }
    let foot = Rect {
        y: area.bottom() - 1,
        height: 1,
        ..area
    };
    f.render_widget(Clear, foot);
    f.render_widget(
        Paragraph::new(hints(
            &[
                ("↑↓", "choose"),
                ("←→", "option"),
                ("enter", "start"),
                ("?", "keys"),
                ("q", "quit"),
            ],
            theme,
        ))
        .style(theme.base()),
        foot,
    );
}

fn stars_or_plain(f: &mut Frame, sky: Rect, app: &App, t: f64) {
    if !app.theme.is_light() {
        fx::stars(f.buffer_mut(), sky, &app.theme, t, 12);
    }
}

fn draw_menu(f: &mut Frame, area: Rect, app: &App, t: f64) {
    let theme = &app.theme;
    f.render_widget(Clear, area);
    let block = panel("modes", theme);
    let inner = block.inner(area);
    f.render_widget(block, area);

    let lines: Vec<Line> = MENU
        .iter()
        .enumerate()
        .take(inner.height as usize)
        .map(|(i, item)| {
            let sel = i == app.menu_sel;
            let label = match item {
                MenuItem::Mode(m) => m.label(),
                MenuItem::Stats => "Stats",
                MenuItem::Settings => "Settings",
                MenuItem::Quit => "Quit",
            };
            let num = if i < 9 {
                format!("{}", i + 1)
            } else {
                " ".into()
            };
            let (extra, extra_style) = menu_extra(*item, app, sel);
            let pulse = (t * 4.0).sin() * 0.5 + 0.5;
            let row_bg = if sel {
                theme.panel.lerp(theme.accent, 0.18 + 0.06 * pulse)
            } else {
                theme.panel
            };
            let base = Style::new().bg(row_bg.c());
            let marker = if sel { "▸" } else { " " };
            let used = 6 + label.chars().count() + extra.chars().count();
            let pad = (inner.width as usize).saturating_sub(used + 1);
            Line::from(vec![
                Span::styled(format!(" {marker} "), base.fg(theme.accent.c())),
                Span::styled(
                    format!("{num}  "),
                    base.fg(if sel { theme.dim } else { theme.faint }.c()),
                ),
                Span::styled(
                    label,
                    if sel {
                        base.fg(theme.fg.c()).add_modifier(Modifier::BOLD)
                    } else {
                        base.fg(theme.dim.c())
                    },
                ),
                Span::styled(" ".repeat(pad), base),
                Span::styled(extra, extra_style.bg(row_bg.c())),
                Span::styled(" ", base),
            ])
        })
        .collect();
    f.render_widget(Paragraph::new(lines), inner);
}

/// The right-aligned detail on a menu row.
fn menu_extra(item: MenuItem, app: &App, sel: bool) -> (String, Style) {
    let theme = &app.theme;
    let opt = |s: String| {
        if sel {
            (format!("◂ {s} ▸"), theme.fg(theme.accent2))
        } else {
            (s, theme.fg(theme.dim))
        }
    };
    match item {
        MenuItem::Mode(Mode::Learn) => {
            let n = app.allowed_keys().len();
            let total = app.layout.learning_order().len();
            (format!("{n}/{total} keys"), theme.dim())
        }
        MenuItem::Mode(Mode::Review) => match app.due_count() {
            0 => ("all caught up".into(), theme.faint()),
            n => (format!("{n} due"), theme.fg(theme.accent3)),
        },
        MenuItem::Mode(Mode::Test) => opt(format!("{}s", app.config.test_seconds)),
        MenuItem::Mode(Mode::Code) => opt(app.config.code_language.clone()),
        MenuItem::Mode(Mode::File) => match &app.file_text {
            Some((_, _, name)) => (truncate(name, 16), theme.dim()),
            None => ("--file to set".into(), theme.faint()),
        },
        _ => (String::new(), theme.dim()),
    }
}

fn truncate(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        s.to_string()
    } else {
        format!("{}…", s.chars().take(n - 1).collect::<String>())
    }
}

fn draw_info(f: &mut Frame, area: Rect, app: &App) {
    let theme = &app.theme;
    f.render_widget(Clear, area);
    let block = panel("today", theme);
    let inner = block.inner(area);
    f.render_widget(block, area);
    let w = inner.width.saturating_sub(2);

    let goal = app.config.daily_goal_secs();
    let done = app.profile.today_secs();
    let frac = done / goal;
    let (fill, empty) = bar(frac, w.saturating_sub(12));
    let goal_col = if frac >= 1.0 {
        theme.good
    } else {
        theme.accent
    };
    let streak = app.profile.streak(goal);
    let best_streak = app.profile.longest_streak(goal);

    let hist: Vec<f64> = app
        .profile
        .history
        .iter()
        .filter(|r| r.layout == app.layout.id)
        .map(|r| r.wpm)
        .collect();
    let best = hist.iter().copied().fold(0.0, f64::max);
    let recent: Vec<f64> = hist.iter().rev().take(10).copied().collect();
    let avg = if recent.is_empty() {
        0.0
    } else {
        recent.iter().sum::<f64>() / recent.len() as f64
    };

    let mut lines = vec![
        Line::from(vec![
            Span::styled(" goal  ", theme.dim()),
            Span::styled(fill, theme.fg(goal_col)),
            Span::styled(empty, Style::new().bg(theme.faint.c())),
            Span::styled(
                format!(
                    " {:>2.0}/{}m",
                    (done / 60.0).floor(),
                    app.config.daily_goal_minutes
                ),
                theme.fg(theme.fg),
            ),
        ]),
        Line::from(vec![
            Span::styled(" streak ", theme.dim()),
            Span::styled(
                format!("✦ {streak} day{}", if streak == 1 { "" } else { "s" }),
                theme.bold(theme.accent3),
            ),
            Span::styled(format!("  best {best_streak}"), theme.faint()),
        ]),
        Line::raw(""),
    ];

    if app.profile.history.is_empty() {
        lines.extend([
            Line::styled(" Welcome! Start with Learn.", theme.bold(theme.accent2)),
            Line::styled(" Rest your fingers on the home row:", theme.dim()),
            Line::styled(" feel the bumps on the index keys.", theme.dim()),
            Line::styled(" Accuracy first; speed follows.", theme.dim()),
        ]);
    } else {
        lines.push(Line::from(vec![
            Span::styled(" speed  ", theme.dim()),
            Span::styled(format!("{avg:.0}"), theme.bold(theme.fg)),
            Span::styled(" avg  ", theme.dim()),
            Span::styled(format!("{best:.0}"), theme.bold(theme.accent)),
            Span::styled(" best wpm", theme.dim()),
        ]));
        let spark: Vec<f64> = hist
            .iter()
            .rev()
            .take(w as usize - 1)
            .rev()
            .copied()
            .collect();
        lines.push(Line::from(
            [vec![Span::raw(" ")], sparkline(&spark, theme)].concat(),
        ));
        lines.push(Line::raw(""));
        lines.push(Line::from(vec![
            Span::styled(" keys   ", theme.dim()),
            Span::styled(format!("{}", app.mastered_count()), theme.bold(theme.good)),
            Span::styled(
                format!(" mastered · {} unlocked", app.allowed_keys().len()),
                theme.dim(),
            ),
        ]));
    }
    lines.push(Line::from(vec![
        Span::styled(" layout ", theme.dim()),
        Span::styled(app.layout.name.clone(), theme.fg(theme.accent2)),
    ]));
    f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
}

/// Unicode block sparkline coloured by height; the best value glows.
pub fn sparkline<'a>(values: &[f64], theme: &Theme) -> Vec<Span<'a>> {
    const BARS: [char; 8] = ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];
    let (lo, hi) = values
        .iter()
        .fold((f64::MAX, f64::MIN), |(lo, hi), v| (lo.min(*v), hi.max(*v)));
    let best = values.iter().position(|v| *v >= hi);
    values
        .iter()
        .enumerate()
        .map(|(i, v)| {
            let t = if hi > lo { (v - lo) / (hi - lo) } else { 0.5 };
            let ch = BARS[((t * 7.0).round() as usize).min(7)];
            let col = if Some(i) == best {
                theme.accent3
            } else {
                Rgb::gradient(&theme.gradient, t)
            };
            Span::styled(ch.to_string(), Style::new().fg(col.c()))
        })
        .collect()
}
