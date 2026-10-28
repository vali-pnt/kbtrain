use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    text::{Line, Span},
    widgets::{Paragraph, Wrap},
};

use super::Theme;
use crate::app::App;

pub fn draw(f: &mut Frame, area: Rect, app: &App, theme: &Theme) {
    let s = &app.session;
    let spans: Vec<Span> = s
        .target
        .iter()
        .enumerate()
        .map(|(i, &t)| {
            let style = match s.input.get(i) {
                Some(&c) if c == t => theme.correct,
                Some(_) => theme.wrong,
                None if i == s.input.len() => theme.cursor,
                None => theme.pending,
            };
            // Make mistyped spaces visible.
            let shown = match (s.input.get(i), t) {
                (Some(&c), ' ') if c != ' ' => '·',
                _ => t,
            };
            Span::styled(shown.to_string(), style)
        })
        .collect();

    let width = area.width.min(72);
    let [_, col, _] = Layout::horizontal([
        Constraint::Fill(1),
        Constraint::Length(width),
        Constraint::Fill(1),
    ])
    .areas(area);
    let [_, text, _] = Layout::vertical([
        Constraint::Fill(1),
        Constraint::Length(4),
        Constraint::Fill(1),
    ])
    .areas(col);

    f.render_widget(Paragraph::new(Line::from(spans)).wrap(Wrap { trim: false }), text);
}

pub fn draw_results(f: &mut Frame, area: Rect, app: &App, theme: &Theme) {
    let s = &app.session;
    let lines = vec![
        Line::styled(format!("{:.0} wpm", s.wpm()), theme.accent).centered(),
        Line::styled(format!("{:.1}% accuracy", s.accuracy() * 100.0), theme.dim).centered(),
        Line::raw(""),
        Line::styled("enter to continue", theme.dim).centered(),
    ];
    let [_, mid, _] = Layout::vertical([
        Constraint::Fill(1),
        Constraint::Length(4),
        Constraint::Fill(1),
    ])
    .areas(area);
    f.render_widget(Paragraph::new(lines), mid);
}
