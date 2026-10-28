use std::collections::BTreeMap;

use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    symbols::Marker,
    text::{Line, Span},
    widgets::{Axis, Chart, Dataset, GraphType, Paragraph},
};

use super::{bar, fmt_secs, hints, key_label, keyboard, panel, theme::Theme};
use crate::{
    analytics,
    app::{App, STATS_TABS},
    layout::Finger,
    stats::{Stat, latency_to_wpm, now_ts, today},
};

pub fn draw(f: &mut Frame, area: Rect, app: &App) {
    let theme = &app.theme;
    let [tabs, _, body, footer] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Fill(1),
        Constraint::Length(1),
    ])
    .areas(area);

    let mut spans = vec![
        Span::styled(format!(" {} ", app.layout.name), theme.bold(theme.accent2)),
        Span::raw("  "),
    ];
    for (i, name) in STATS_TABS.iter().enumerate() {
        let style = if i == app.stats_tab {
            Style::new()
                .bg(theme.accent.c())
                .fg(theme.bg.c())
                .add_modifier(Modifier::BOLD)
        } else {
            theme.dim()
        };
        spans.push(Span::styled(format!(" {} {name} ", i + 1), style));
        spans.push(Span::raw(" "));
    }
    f.render_widget(Line::from(spans), tabs);

    let body = Rect {
        x: body.x + 1,
        width: body.width.saturating_sub(2),
        ..body
    };
    match app.stats_tab {
        0 => progress(f, body, app),
        1 => keys(f, body, app),
        2 => transitions(f, body, app),
        3 => fingers(f, body, app),
        _ => calendar(f, body, app),
    }
    f.render_widget(
        hints(&[("←→", "tab"), ("1-5", "jump"), ("esc", "back")], theme),
        footer,
    );
}

fn empty(f: &mut Frame, area: Rect, theme: &Theme, msg: &str) {
    let r = super::center(area, area.width, 3);
    f.render_widget(
        Paragraph::new(vec![
            Line::styled(msg.to_string(), theme.dim()).centered(),
            Line::raw(""),
            Line::styled("Finish a drill and come back.", theme.faint()).centered(),
        ]),
        r,
    );
}

fn line_chart<'a>(
    title: &'a str,
    theme: &'a Theme,
    sets: Vec<Dataset<'a>>,
    max_x: f64,
    y: [f64; 2],
    unit: &str,
) -> Chart<'a> {
    let lab = |s: String| Span::styled(s, theme.faint());
    Chart::new(sets)
        .block(panel(title, theme))
        .x_axis(
            Axis::default()
                .bounds([0.0, max_x.max(1.0)])
                .style(theme.faint())
                .labels([lab("1".into()), lab(format!("{max_x:.0}"))]),
        )
        .y_axis(Axis::default().bounds(y).style(theme.faint()).labels([
            lab(format!("{:.0}{unit}", y[0])),
            lab(format!("{:.0}{unit}", y[1])),
        ]))
}

fn progress(f: &mut Frame, area: Rect, app: &App) {
    let theme = &app.theme;
    let hist: Vec<_> = app
        .profile
        .history
        .iter()
        .filter(|h| h.layout == app.layout.id)
        .collect();
    let all = &app.profile.history;
    let best = hist.iter().map(|h| h.wpm).fold(0.0, f64::max);
    let summary = Line::from(vec![
        Span::styled(format!("{}", all.len()), theme.bold(theme.fg)),
        Span::styled(" drills   ", theme.dim()),
        Span::styled(fmt_secs(app.profile.total_secs()), theme.bold(theme.fg)),
        Span::styled(" practised   ", theme.dim()),
        Span::styled(
            format!("{}", app.lp().total_keystrokes()),
            theme.bold(theme.fg),
        ),
        Span::styled(" keystrokes   ", theme.dim()),
        Span::styled(format!("{best:.0}"), theme.bold(theme.accent)),
        Span::styled(" best wpm   ", theme.dim()),
        Span::styled(
            format!(
                "{}/{}",
                app.mastered_count(),
                app.layout.learning_order().len()
            ),
            theme.bold(theme.good),
        ),
        Span::styled(" keys mastered", theme.dim()),
    ]);
    let [top, charts] = Layout::vertical([Constraint::Length(2), Constraint::Fill(1)]).areas(area);
    f.render_widget(summary.centered(), top);
    if hist.len() < 2 {
        return empty(
            f,
            charts,
            theme,
            "Progress charts appear after a couple of drills.",
        );
    }

    let n = hist.len() as f64;
    let wpm: Vec<(f64, f64)> = hist
        .iter()
        .enumerate()
        .map(|(i, h)| (i as f64 + 1.0, h.wpm))
        .collect();
    let mut pb = 0.0;
    let pbs: Vec<(f64, f64)> = hist
        .iter()
        .enumerate()
        .map(|(i, h)| {
            pb = f64::max(pb, h.wpm);
            (i as f64 + 1.0, pb)
        })
        .collect();
    let acc: Vec<(f64, f64)> = hist
        .iter()
        .enumerate()
        .map(|(i, h)| (i as f64 + 1.0, h.accuracy * 100.0))
        .collect();
    let mastered: Vec<(f64, f64)> = hist
        .iter()
        .enumerate()
        .map(|(i, h)| (i as f64 + 1.0, h.mastered as f64))
        .collect();
    let max_w = (best / 20.0).ceil().max(1.0) * 20.0;
    let min_acc = acc
        .iter()
        .map(|a| a.1)
        .fold(100.0, f64::min)
        .min(90.0)
        .floor();

    let [upper, lower] =
        Layout::vertical([Constraint::Percentage(55), Constraint::Percentage(45)]).areas(charts);
    let [l, r] =
        Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)]).areas(lower);
    f.render_widget(
        line_chart(
            "speed (wpm) · running best",
            theme,
            vec![
                Dataset::default()
                    .marker(Marker::Braille)
                    .graph_type(GraphType::Line)
                    .style(theme.fg(theme.accent3))
                    .data(&pbs),
                Dataset::default()
                    .marker(Marker::Braille)
                    .graph_type(GraphType::Line)
                    .style(theme.bold(theme.accent))
                    .data(&wpm),
            ],
            n,
            [0.0, max_w],
            "",
        ),
        upper,
    );
    f.render_widget(
        line_chart(
            "accuracy",
            theme,
            vec![
                Dataset::default()
                    .marker(Marker::Braille)
                    .graph_type(GraphType::Line)
                    .style(theme.fg(theme.good))
                    .data(&acc),
            ],
            n,
            [min_acc, 100.0],
            "%",
        ),
        l,
    );
    let total = app.layout.learning_order().len() as f64;
    f.render_widget(
        line_chart(
            "keys mastered",
            theme,
            vec![
                Dataset::default()
                    .marker(Marker::Braille)
                    .graph_type(GraphType::Line)
                    .style(theme.fg(theme.accent2))
                    .data(&mastered),
            ],
            n,
            [0.0, total],
            "",
        ),
        r,
    );
}

fn keys(f: &mut Frame, area: Rect, app: &App) {
    let theme = &app.theme;
    let kb_h = keyboard::large_height(true);
    let show_kb = area.width >= keyboard::large_width(4) && area.height >= kb_h + 6;
    let [kb, table] = Layout::vertical([
        Constraint::Length(if show_kb { kb_h } else { 0 }),
        Constraint::Fill(1),
    ])
    .areas(area);
    if show_kb {
        let opts = keyboard::Opts {
            next: None,
            alpha: 1.0,
            ripples: &[],
            allowed: None,
            numbers: true,
        };
        keyboard::draw_large(f.buffer_mut(), kb, app, &opts);
    }

    let target = app.target_ms();
    let now = now_ts();
    let mut rows: Vec<(char, &Stat)> = app
        .lp()
        .keys
        .iter()
        .filter(|(c, s)| !c.is_whitespace() && s.count() >= 3)
        .map(|(c, s)| (*c, s))
        .collect();
    if rows.is_empty() {
        return empty(f, table, theme, "No keystrokes on this layout yet.");
    }
    rows.sort_by(|a, b| b.1.weakness(target).total_cmp(&a.1.weakness(target)));
    let cols = (table.width / 46).clamp(1, 3) as usize;
    let per = table.height.saturating_sub(3) as usize;
    let header = Line::styled(
        " key   speed   errors  hits  confidence  review",
        theme.faint(),
    );
    let areas = Layout::horizontal(vec![Constraint::Fill(1); cols]).split(table);
    for (ci, a) in areas.iter().enumerate() {
        let mut lines = vec![header.clone()];
        for (c, s) in rows.iter().skip(ci * per).take(per) {
            let conf = s.confidence(target).min(1.0);
            let (fill, rest) = bar(conf, 8);
            let review = if s.memory.reps == 0 {
                "new".to_string()
            } else {
                let d = s.memory.due_in_days(now);
                if d <= 0.0 {
                    "due".into()
                } else if d < 1.0 {
                    format!("{:.0}h", d * 24.0)
                } else {
                    format!("{d:.0}d")
                }
            };
            let heat = theme.heat(s.weakness(target));
            lines.push(Line::from(vec![
                Span::styled(format!("  {}  ", key_label(*c)), theme.bold(heat)),
                Span::styled(
                    format!("{:>5}", s.wpm().map_or("-".into(), |w| format!("{w:.0}"))),
                    theme.fg(theme.fg),
                ),
                Span::styled(" wpm ", theme.faint()),
                Span::styled(
                    format!("{:>5.1}%", s.error_rate * 100.0),
                    theme.fg(if s.error_rate > 0.08 {
                        theme.bad
                    } else {
                        theme.dim
                    }),
                ),
                Span::styled(format!("{:>6} ", s.count()), theme.dim()),
                Span::styled(fill, theme.fg(heat)),
                Span::styled(rest, Style::new().bg(theme.faint.c())),
                Span::styled(format!("   {review}"), theme.dim()),
            ]));
        }
        f.render_widget(
            Paragraph::new(lines).block(panel(if ci == 0 { "weakest first" } else { "" }, theme)),
            *a,
        );
    }
}

fn transitions(f: &mut Frame, area: Rect, app: &App) {
    let theme = &app.theme;
    let lp = app.lp();
    let target = app.target_ms();
    if lp.bigrams.is_empty() {
        return empty(f, area, theme, "No letter transitions recorded yet.");
    }
    let letters: Vec<char> = app.layout.learning_order().into_iter().take(16).collect();
    let grid_w = (letters.len() * 3 + 6) as u16;
    let [grid, lists] = Layout::horizontal([
        Constraint::Length(grid_w.min(area.width / 2 + 10)),
        Constraint::Fill(1),
    ])
    .areas(area);

    let mut lines = vec![Line::from(
        std::iter::once(Span::styled("    ", theme.faint()))
            .chain(
                letters
                    .iter()
                    .map(|c| Span::styled(format!(" {c} "), theme.dim())),
            )
            .collect::<Vec<_>>(),
    )];
    for a in &letters {
        let mut spans = vec![Span::styled(format!("  {a} "), theme.dim())];
        for b in &letters {
            let g: String = [*a, *b].iter().collect();
            let cell = match lp.bigrams.get(&g).filter(|s| s.count() >= 2) {
                Some(s) => Span::styled(
                    "   ",
                    Style::new().bg(theme.fade(theme.heat(s.weakness(target)), 0.85).c()),
                ),
                None => Span::styled(" · ", theme.faint()),
            };
            spans.push(cell);
        }
        lines.push(Line::from(spans));
    }
    lines.push(Line::raw(""));
    lines.push(Line::from(vec![
        Span::styled("  row → column   ", theme.faint()),
        Span::styled("  ", Style::new().bg(theme.heat(0.0).c())),
        Span::styled(" fluent  ", theme.dim()),
        Span::styled("  ", Style::new().bg(theme.heat(1.0).c())),
        Span::styled(" needs work", theme.dim()),
    ]));
    f.render_widget(
        Paragraph::new(lines).block(panel("bigram heatmap", theme)),
        grid,
    );

    let slow = top(&lp.bigrams, |s| s.latency_ms.unwrap_or(0.0), 3);
    let errs = top(&lp.bigrams, |s| s.error_rate, 3);
    let tri = top(&lp.trigrams, |s| s.latency_ms.unwrap_or(0.0), 3);
    let fmt =
        |items: &[(&String, &Stat)], metric: &dyn Fn(&Stat) -> String| -> Vec<Line<'static>> {
            items
                .iter()
                .map(|(g, s)| {
                    let kind = {
                        let cs: Vec<char> = g.chars().collect();
                        match analytics::classify(&app.layout, cs[0], cs[1]) {
                            Some(analytics::Transition::SameFinger) => "same finger",
                            Some(analytics::Transition::SameHand) => "roll",
                            Some(analytics::Transition::Alternate) => "alternate",
                            _ => "repeat",
                        }
                    };
                    Line::from(vec![
                        Span::styled(
                            format!(" {g:<4}"),
                            theme.bold(theme.heat(s.weakness(target))),
                        ),
                        Span::styled(format!("{:>9}", metric(s)), theme.fg(theme.fg)),
                        Span::styled(format!("  {kind}"), theme.faint()),
                    ])
                })
                .collect()
        };
    let rows = Layout::vertical([Constraint::Fill(1); 3]).split(lists);
    let wpm = |s: &Stat| {
        s.latency_ms
            .map_or("-".into(), |l| format!("{:.0} wpm", latency_to_wpm(l)))
    };
    f.render_widget(
        Paragraph::new(fmt(&slow, &wpm)).block(panel("slowest pairs", theme)),
        rows[0],
    );
    f.render_widget(
        Paragraph::new(fmt(&errs, &|s| format!("{:.0}% err", s.error_rate * 100.0)))
            .block(panel("most mistyped pairs", theme)),
        rows[1],
    );
    f.render_widget(
        Paragraph::new(fmt(&tri, &wpm)).block(panel("slowest triples", theme)),
        rows[2],
    );
}

fn fingers(f: &mut Frame, area: Rect, app: &App) {
    let theme = &app.theme;
    let lp = app.lp();
    if lp.total_keystrokes() == 0 {
        return empty(f, area, theme, "No keystrokes on this layout yet.");
    }
    let fs = analytics::fingers(&app.layout, lp);
    let [left, right] =
        Layout::horizontal([Constraint::Percentage(55), Constraint::Percentage(45)]).areas(area);

    let max_share = fs.iter().map(|f| f.share).fold(0.01, f64::max);
    let bw = left.width.saturating_sub(40).max(6);
    let mut lines = vec![Line::styled(
        format!(
            " {:<13}{:<w$}   load   speed   errors",
            "",
            "",
            w = bw as usize
        ),
        theme.faint(),
    )];
    for s in &fs {
        let (fill, rest) = bar(s.share / max_share, bw);
        let col = theme.finger(s.finger);
        lines.push(Line::from(vec![
            Span::styled(format!(" {:<13}", s.finger.name()), theme.fg(col)),
            Span::styled(fill, theme.fg(col)),
            Span::styled(rest, theme.faint()),
            Span::styled(format!(" {:>5.1}%", s.share * 100.0), theme.fg(theme.fg)),
            Span::styled(
                format!(
                    " {:>5}",
                    s.latency_ms
                        .map_or("-".into(), |l| format!("{:.0}", latency_to_wpm(l)))
                ),
                theme.fg(theme.fg),
            ),
            Span::styled(" wpm", theme.faint()),
            Span::styled(
                format!(" {:>5.1}%", s.error_rate * 100.0),
                theme.fg(if s.error_rate > 0.08 {
                    theme.bad
                } else {
                    theme.dim
                }),
            ),
        ]));
    }
    let left_hand: f64 = fs
        .iter()
        .filter(|s| s.finger <= Finger::LIndex)
        .map(|s| s.share)
        .sum();
    let right_hand: f64 = fs
        .iter()
        .filter(|s| (Finger::RIndex..=Finger::RPinky).contains(&s.finger))
        .map(|s| s.share)
        .sum();
    let hands = (left_hand + right_hand).max(1e-9);
    lines.push(Line::raw(""));
    let (lf, rf) = bar(left_hand / hands, bw + 13);
    lines.push(Line::from(vec![
        Span::styled(" hands  L ", theme.dim()),
        Span::styled(lf, theme.fg(theme.fingers[0])),
        Span::styled(
            rf.replace(' ', "█"),
            theme.fg(theme.fingers[7].lerp(theme.accent2, 0.5)),
        ),
        Span::styled(
            format!(
                " R   {:.0}% / {:.0}%",
                left_hand / hands * 100.0,
                right_hand / hands * 100.0
            ),
            theme.dim(),
        ),
    ]));
    f.render_widget(Paragraph::new(lines).block(panel("fingers", theme)), left);

    let tr = analytics::transitions(&app.layout, lp);
    let rows = analytics::rows(&app.layout, lp);
    let (sfb, alt, home) = analytics::layout_profile(&app.layout, app.trainer.words());
    let speed = |l: Option<f64>| l.map_or("-".into(), |l| format!("{:.0} wpm", latency_to_wpm(l)));
    let tline = |name: &str, (share, lat): (f64, Option<f64>), col| {
        Line::from(vec![
            Span::styled(format!(" {name:<13}"), theme.fg(col)),
            Span::styled(format!("{:>5.1}%", share * 100.0), theme.fg(theme.fg)),
            Span::styled(format!("   {:>8}", speed(lat)), theme.dim()),
        ])
    };
    let mut lines = vec![
        Line::styled(" your transitions", theme.faint()),
        tline("alternation", tr.alternate, theme.good),
        tline("same hand", tr.same_hand, theme.accent2),
        tline("same finger", tr.same_finger, theme.bad),
        Line::raw(""),
        Line::styled(" rows", theme.faint()),
    ];
    for (name, share) in ["number", "top", "home", "bottom"].iter().zip(rows) {
        let (fill, rest) = bar(share, 12);
        lines.push(Line::from(vec![
            Span::styled(format!(" {name:<9}"), theme.dim()),
            Span::styled(fill, theme.fg(theme.accent)),
            Span::styled(rest, theme.faint()),
            Span::styled(format!(" {:>5.1}%", share * 100.0), theme.fg(theme.fg)),
        ]));
    }
    lines.extend([
        Line::raw(""),
        Line::styled(format!(" {} on English", app.layout.name), theme.faint()),
        Line::from(vec![
            Span::styled(format!(" {sfb:.1}%",), theme.bold(theme.fg)),
            Span::styled(" same-finger  ", theme.dim()),
            Span::styled(format!("{alt:.0}%"), theme.bold(theme.fg)),
            Span::styled(" alternation", theme.dim()),
        ]),
        Line::from(vec![
            Span::styled(format!(" {home:.0}%"), theme.bold(theme.fg)),
            Span::styled(" of letters on the home row", theme.dim()),
        ]),
    ]);
    f.render_widget(
        Paragraph::new(lines).block(panel("hands & transitions", theme)),
        right,
    );
}

fn calendar(f: &mut Frame, area: Rect, app: &App) {
    let theme = &app.theme;
    let goal = app.config.daily_goal_secs();
    let weeks = ((area.width.saturating_sub(10)) / 2).clamp(4, 52) as i64;
    let today = today();
    use chrono::Datelike;
    let offset = today.weekday().num_days_from_monday() as i64;
    let start = today - chrono::Days::new((weeks * 7 - 7 + offset) as u64);

    let mut lines = Vec::new();
    for dow in 0..7 {
        let name = ["mon", "", "wed", "", "fri", "", "sun"][dow as usize];
        let mut spans = vec![Span::styled(format!(" {name:<4}"), theme.faint())];
        for w in 0..weeks {
            let d = start + chrono::Days::new((w * 7 + dow) as u64);
            if d > today {
                spans.push(Span::raw("  "));
                continue;
            }
            let secs = app.profile.days.get(&d).map_or(0.0, |r| r.secs);
            let span = if secs <= 0.0 {
                Span::styled("■ ", theme.fg(theme.fade(theme.faint, 0.7)))
            } else {
                let t = (secs / goal).min(1.0);
                let col = if t >= 1.0 {
                    theme.good
                } else {
                    theme.fade(theme.accent2, 0.3 + 0.6 * t)
                };
                Span::styled(if d == today { "◆ " } else { "■ " }, theme.fg(col))
            };
            spans.push(span);
        }
        lines.push(Line::from(spans));
    }
    let days_practised = app.profile.days.values().filter(|d| d.secs > 0.0).count();
    let goal_days = app.profile.days.values().filter(|d| d.secs >= goal).count();
    lines.extend([
        Line::raw(""),
        Line::from(vec![
            Span::styled("      ■", theme.fg(theme.fade(theme.faint, 0.7))),
            Span::styled(" none  ", theme.dim()),
            Span::styled("■", theme.fg(theme.fade(theme.accent2, 0.5))),
            Span::styled(" some  ", theme.dim()),
            Span::styled("■", theme.fg(theme.good)),
            Span::styled(
                format!(" goal met ({} min)", app.config.daily_goal_minutes),
                theme.dim(),
            ),
        ]),
        Line::raw(""),
        Line::from(vec![
            Span::styled("      ✦ ", theme.fg(theme.accent3)),
            Span::styled(
                format!("{}", app.profile.streak(goal)),
                theme.bold(theme.accent3),
            ),
            Span::styled(" day streak   ", theme.dim()),
            Span::styled(
                format!("{}", app.profile.longest_streak(goal)),
                theme.bold(theme.fg),
            ),
            Span::styled(" longest   ", theme.dim()),
            Span::styled(format!("{goal_days}"), theme.bold(theme.fg)),
            Span::styled(" goal days   ", theme.dim()),
            Span::styled(format!("{days_practised}"), theme.bold(theme.fg)),
            Span::styled(" days practised", theme.dim()),
        ]),
    ]);
    let h = (lines.len() as u16 + 2).min(area.height);
    let r = Rect { height: h, ..area };
    f.render_widget(
        Paragraph::new(lines).block(panel("streak calendar", theme)),
        r,
    );
}

/// The eight n-grams with the highest `key`, among those seen `min` times.
fn top(m: &BTreeMap<String, Stat>, key: impl Fn(&Stat) -> f64, min: u32) -> Vec<(&String, &Stat)> {
    let mut v: Vec<(&String, &Stat)> = m.iter().filter(|(_, s)| s.count() >= min).collect();
    v.sort_by(|a, b| key(b.1).total_cmp(&key(a.1)));
    v.into_iter().take(8).collect()
}
