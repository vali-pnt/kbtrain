use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

use super::{fmt_secs, gradient_line, hints, keyboard, theme::Rgb};
use crate::{
    app::App,
    config::{Animations, KeyboardMode},
    layout::NUMBER_ROW,
    trainer::Mode,
};

/// Combo tiers: (threshold, name).
const TIERS: [(u32, &str); 5] = [
    (0, ""),
    (10, "flow"),
    (25, "surge"),
    (50, "overdrive"),
    (100, "transcendent"),
];

pub fn draw(f: &mut Frame, area: Rect, app: &App) {
    let theme = &app.theme;
    let code = app.drill.code;
    let text_rows: u16 = if code { 7 } else { 3 };
    let show_bar = matches!(app.drill.mode, Mode::Learn | Mode::Review | Mode::Ngrams)
        && app.config.progressive_unlock;

    let numbers = app.drill.text.chars().any(|c| {
        app.layout
            .locate(c)
            .is_some_and(|(k, _)| k.row == NUMBER_ROW)
    });
    let (kb_kind, kb_h) = keyboard_fit(app, area, numbers, text_rows);

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
    draw_text(f, col, app);
    draw_progress(
        f,
        Rect {
            x: col.x,
            width: col.width,
            ..progress
        },
        app,
    );
    draw_combo(f, combo, app);

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
        theme.fg(theme.fg),
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
        right.push(Span::styled(
            format!("{:.1}%", s.accuracy() * 100.0),
            theme.fg(theme.fg),
        ));
        right.push(Span::styled(" acc  ", theme.dim()));
    }
    match s.remaining() {
        Some(r) => right.push(Span::styled(
            format!("{} left ", fmt_secs(r.ceil())),
            theme.bold(theme.accent3),
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
/// the focus key highlighted, locked ones dim.
fn draw_unlock_bar(f: &mut Frame, area: Rect, app: &App) {
    let theme = &app.theme;
    let order = app.layout.learning_order();
    let allowed = app.allowed_keys();
    let target = app.target_ms();
    let mut spans = Vec::new();
    for c in &order {
        let unlocked = allowed.contains(c);
        let stat = app.lp().key(*c);
        let style = if Some(*c) == app.drill.focus {
            Style::new()
                .bg(theme.accent.c())
                .fg(theme.bg.c())
                .add_modifier(Modifier::BOLD)
        } else if !unlocked {
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
        let filled = (conf * 8.0).round() as usize;
        spans.push(Span::styled("   focus ", theme.dim()));
        spans.push(Span::styled(format!("{fk} "), theme.bold(theme.accent)));
        spans.push(Span::styled(
            "▰".repeat(filled),
            theme.fg(theme.heat(1.0 - conf)),
        ));
        spans.push(Span::styled("▱".repeat(8 - filled), theme.faint()));
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

fn draw_text(f: &mut Frame, area: Rect, app: &App) {
    let theme = &app.theme;
    let s = &app.session;
    let lines = wrap(&s.target, area.width.saturating_sub(1) as usize);
    let cursor = s.cursor();
    let cur_line = lines
        .iter()
        .position(|(a, b)| cursor >= *a && cursor < *b)
        .unwrap_or(lines.len() - 1);
    let rows = area.height as usize;
    let mut first = cur_line.saturating_sub(1);
    if first + rows > lines.len() {
        first = lines.len().saturating_sub(rows);
    }

    let anim = app.animations() != Animations::Off;
    let pulse = if anim {
        (app.t * 6.0).sin() * 0.5 + 0.5
    } else {
        1.0
    };
    let blocked = s
        .last_blocked
        .is_some_and(|t| t.elapsed().as_millis() < 250);
    let cursor_bg = if blocked {
        theme.bad
    } else {
        theme.accent.lerp(theme.accent2, pulse * 0.35)
    };
    let focus = app.drill.focus;

    let out: Vec<Line> = lines
        .iter()
        .enumerate()
        .skip(first)
        .take(rows)
        .map(|(li, &(a, b))| {
            let ahead = li > cur_line + 1;
            let spans: Vec<Span> = (a..b)
                .map(|i| {
                    let t = s.target[i];
                    let shown = match t {
                        '\n' => '↵',
                        c => c,
                    };
                    if i < cursor {
                        let typed = s.input[i];
                        if typed == t {
                            let col = if s.ever_wrong[i] {
                                theme.warn
                            } else {
                                theme.fg
                            };
                            let ch = if t == '\n' { ' ' } else { t };
                            Span::styled(ch.to_string(), theme.fg(col))
                        } else {
                            let ch = if t == ' ' || t == '\n' { '·' } else { t };
                            Span::styled(
                                ch.to_string(),
                                theme.fg(theme.bad).add_modifier(Modifier::UNDERLINED),
                            )
                        }
                    } else if i == cursor {
                        Span::styled(
                            shown.to_string(),
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
                        Span::styled(shown.to_string(), theme.fg(col))
                    }
                })
                .collect();
            Line::from(spans)
        })
        .collect();
    f.render_widget(Paragraph::new(out), area);
}

fn draw_progress(f: &mut Frame, area: Rect, app: &App) {
    let theme = &app.theme;
    let w = area.width as usize;
    let done = ((app.session.progress() * w as f64).round() as usize).min(w);
    let spans: Vec<Span> = (0..w)
        .map(|i| {
            if i < done {
                Span::styled(
                    "━",
                    Style::new().fg(Rgb::gradient(&theme.gradient, i as f64 / w.max(1) as f64).c()),
                )
            } else {
                Span::styled("─", theme.fg(theme.faint))
            }
        })
        .collect();
    f.render_widget(Line::from(spans), area);
}

fn draw_combo(f: &mut Frame, area: Rect, app: &App) {
    let theme = &app.theme;
    let s = &app.session;
    let anim = app.animations() != Animations::Off;

    if let Some((at, n)) = s
        .combo_broken
        .filter(|(at, _)| at.elapsed().as_secs_f64() < 0.7)
    {
        let age = at.elapsed().as_secs_f64();
        let shake = if anim && age < 0.3 && ((age * 40.0) as u32).is_multiple_of(2) {
            1
        } else {
            0
        };
        let msg = format!("×{n} combo broken");
        let w = msg.chars().count() as u16;
        let x = area.x + (area.width.saturating_sub(w)) / 2 + shake;
        let r = Rect {
            x,
            width: w.min(area.right().saturating_sub(x)),
            ..area
        };
        f.render_widget(
            Span::styled(msg, theme.fg(theme.fade(theme.bad, 1.0 - age))),
            r,
        );
        return;
    }
    if s.combo < TIERS[1].0 {
        return;
    }
    let tier = TIERS.iter().rposition(|(t, _)| s.combo >= *t).unwrap_or(0);
    let lo = TIERS[tier].0;
    let hi = TIERS.get(tier + 1).map_or(lo * 2, |t| t.0);
    let frac = (s.combo - lo) as f64 / (hi - lo).max(1) as f64;
    let col = Rgb::gradient(&theme.gradient, tier as f64 / (TIERS.len() - 1) as f64);
    let glow = if anim {
        (app.t * 8.0).sin() * 0.15 + 0.85
    } else {
        1.0
    };
    let seg = 10;
    let filled = (frac * seg as f64).floor() as usize;
    let line = Line::from(vec![
        Span::styled(format!("×{} ", s.combo), theme.bold(theme.fade(col, glow))),
        Span::styled(
            format!("{} ", TIERS[tier].1.to_uppercase()),
            theme.fg(col).add_modifier(Modifier::BOLD),
        ),
        Span::styled("▰".repeat(filled), theme.fg(col)),
        Span::styled("▱".repeat(seg - filled), theme.faint()),
    ])
    .centered();
    f.render_widget(line, area);
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
