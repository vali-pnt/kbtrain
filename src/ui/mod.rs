mod keyboard;
mod theme;
mod typing;

use ratatui::{
    Frame,
    layout::{Constraint, Layout},
    style::Stylize,
    text::{Line, Span},
    widgets::Paragraph,
};

use crate::app::{App, Screen};
pub use theme::Theme;

pub fn draw(f: &mut Frame, app: &App) {
    let theme = Theme::default();
    f.render_widget(ratatui::widgets::Block::new().style(theme.base), f.area());

    let [header, body, kb, footer] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Fill(1),
        Constraint::Length(keyboard::HEIGHT),
        Constraint::Length(1),
    ])
    .margin(1)
    .areas(f.area());

    draw_header(f, header, app, &theme);
    match app.screen {
        Screen::Typing => typing::draw(f, body, app, &theme),
        Screen::Results => typing::draw_results(f, body, app, &theme),
    }
    keyboard::draw(f, kb, app, &theme);

    let hint = Line::from(vec![
        Span::styled("tab", theme.accent),
        Span::styled(" new drill  ", theme.dim),
        Span::styled("esc", theme.accent),
        Span::styled(" quit", theme.dim),
    ])
    .centered();
    f.render_widget(hint, footer);
}

fn draw_header(f: &mut Frame, area: ratatui::layout::Rect, app: &App, theme: &Theme) {
    let s = &app.session;
    let line = Line::from(vec![
        Span::styled(" kbtrain ", theme.title).bold(),
        Span::styled(format!("  {}  ", app.layout.name), theme.dim),
        Span::styled(format!("{:>5.0} wpm", s.wpm()), theme.accent),
        Span::styled(format!("  {:>5.1}% acc", s.accuracy() * 100.0), theme.dim),
        Span::styled(format!("  {:>5.1}s", s.elapsed()), theme.dim),
    ]);
    f.render_widget(Paragraph::new(line), area);
}
