//! On-screen keyboard: shows the next key, flashes the last press, and
//! colours each key by the learner's weakness on it.

use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

use super::Theme;
use crate::{app::App, stats::wpm_to_latency};

pub const HEIGHT: u16 = 3;
const KEY_W: usize = 4;
/// Horizontal stagger (in cells) for each row, mimicking a real keyboard.
const STAGGER: [usize; 3] = [0, 1, 3];

pub fn draw(f: &mut Frame, area: Rect, app: &App, theme: &Theme) {
    let next = app.session.expected().map(|c| c.to_ascii_lowercase());
    let total_w = 11 * KEY_W + STAGGER[2];
    let pad = (area.width as usize).saturating_sub(total_w) / 2;

    let lines: Vec<Line> = app
        .layout
        .rows
        .iter()
        .skip(1)
        .enumerate()
        .map(|(r, row)| {
            let mut spans = vec![Span::raw(" ".repeat(pad + STAGGER[r.min(2)]))];
            for key in row {
                let mut style = match app.stats.key(key.ch).map(|s| s.weakness(wpm_to_latency(35.0))) {
                    Some(w) => Style::new().fg(theme.heat(w)),
                    None if key.home => theme.key_home,
                    None => theme.key,
                };
                if key.home {
                    style = style.add_modifier(Modifier::UNDERLINED);
                }
                if Some(key.ch) == next {
                    style = theme.key_next;
                } else if let Some((c, ok)) = app.last_press {
                    if c.to_ascii_lowercase() == key.ch {
                        style = if ok { theme.correct } else { theme.wrong }
                            .add_modifier(Modifier::REVERSED);
                    }
                }
                spans.push(Span::styled(format!(" {} ", key.ch), style));
                spans.push(Span::raw(" "));
            }
            Line::from(spans)
        })
        .collect();

    f.render_widget(Paragraph::new(lines), area);
}
