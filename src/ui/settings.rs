use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

use super::{center, gradient_line, hints, panel};
use crate::app::App;

/// Rows that are actions (enter) rather than values (← →).
const ACTIONS: [usize; 2] = [17, 18];

pub fn draw(f: &mut Frame, area: Rect, app: &App) {
    let theme = &app.theme;
    let rows = app.settings_rows();
    let h = (rows.len() as u16 + 2).min(area.height.saturating_sub(5));
    let box_area = center(area, 64.min(area.width), h + 4);
    let [title, list, help, _, footer] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(h),
        Constraint::Length(2),
        Constraint::Fill(1),
        Constraint::Length(1),
    ])
    .areas(Rect {
        height: area.height.saturating_sub(box_area.y - area.y),
        ..box_area
    });
    f.render_widget(
        gradient_line("SETTINGS", theme, app.t * 0.1).centered(),
        title,
    );

    let block = panel("", theme);
    let inner = block.inner(list);
    f.render_widget(block, list);
    let visible = inner.height as usize;
    let first = app.settings_sel.saturating_sub(visible.saturating_sub(1));
    let lines: Vec<Line> = rows
        .iter()
        .enumerate()
        .skip(first)
        .take(visible)
        .map(|(i, (label, value, _))| {
            let sel = i == app.settings_sel;
            let bg = if sel {
                theme.panel.lerp(theme.accent, 0.2)
            } else {
                theme.panel
            };
            let base = Style::new().bg(bg.c());
            let action = ACTIONS.contains(&i);
            let shown = if action {
                if value.is_empty() {
                    "enter ↵".to_string()
                } else {
                    value.clone()
                }
            } else if sel {
                format!("◂ {value} ▸")
            } else {
                value.clone()
            };
            let value_style = if action && !value.is_empty() {
                base.fg(theme.bad.c()).add_modifier(Modifier::BOLD)
            } else if sel {
                base.fg(theme.accent2.c()).add_modifier(Modifier::BOLD)
            } else {
                base.fg(theme.fg.c())
            };
            let pad = (inner.width as usize)
                .saturating_sub(label.chars().count() + shown.chars().count() + 4);
            Line::from(vec![
                Span::styled(if sel { " ▸ " } else { "   " }, base.fg(theme.accent.c())),
                Span::styled(
                    *label,
                    if sel {
                        base.fg(theme.fg.c())
                    } else {
                        base.fg(theme.dim.c())
                    },
                ),
                Span::styled(" ".repeat(pad), base),
                Span::styled(shown, value_style),
                Span::styled(" ", base),
            ])
        })
        .collect();
    f.render_widget(Paragraph::new(lines), inner);

    if let Some((_, _, desc)) = rows.get(app.settings_sel) {
        f.render_widget(
            Paragraph::new(Line::styled(*desc, theme.dim()).centered()),
            help,
        );
    }
    let foot = Rect {
        y: area.bottom().saturating_sub(1),
        ..footer
    };
    f.render_widget(
        hints(
            &[
                ("↑↓", "choose"),
                ("←→", "change"),
                ("enter", "action"),
                ("esc", "back"),
            ],
            theme,
        ),
        foot,
    );
}
