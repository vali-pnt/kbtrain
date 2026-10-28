pub mod fx;
mod home;
mod keyboard;
mod results;
mod settings;
mod stats;
pub mod theme;
mod typing;

use ratatui::{
    Frame,
    layout::{Constraint, Flex, Layout, Rect},
    style::{Modifier, Style},
    symbols::border,
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
};

use crate::app::{App, Screen};
use theme::{Rgb, Theme};

pub fn draw(f: &mut Frame, app: &App) {
    let theme = &app.theme;
    f.render_widget(Block::new().style(theme.base()), f.area());
    match app.screen {
        Screen::Home => home::draw(f, f.area(), app),
        Screen::Typing => typing::draw(f, f.area(), app),
        Screen::Results => results::draw(f, f.area(), app),
        Screen::Stats => stats::draw(f, f.area(), app),
        Screen::Settings => settings::draw(f, f.area(), app),
    }
    let area = f.area();
    app.particles.render(f.buffer_mut(), area, theme);
    if app.show_help {
        draw_help(f, app);
    }
    if let Some((msg, _)) = &app.toast {
        draw_toast(f, msg, theme);
    }
    if !app.truecolor {
        theme::downgrade(f.buffer_mut());
    }
}

/// A rounded panel with a styled title.
pub fn panel<'a>(title: &'a str, theme: &Theme) -> Block<'a> {
    let block = Block::new()
        .borders(Borders::ALL)
        .border_set(border::ROUNDED)
        .border_style(theme.fg(theme.faint))
        .style(Style::new().bg(theme.panel.c()).fg(theme.fg.c()));
    if title.is_empty() {
        block
    } else {
        block.title(Span::styled(
            format!(" {title} "),
            theme.bold(theme.accent2),
        ))
    }
}

/// Text coloured along the theme gradient, shifting with `t` when animated.
pub fn gradient_line<'a>(text: &str, theme: &Theme, shift: f64) -> Line<'a> {
    let n = text.chars().count().max(1) as f64;
    Line::from(
        text.chars()
            .enumerate()
            .map(|(i, c)| {
                let p = ((i as f64 / n) + shift).rem_euclid(2.0);
                let p = if p > 1.0 { 2.0 - p } else { p };
                Span::styled(
                    c.to_string(),
                    Style::new()
                        .fg(Rgb::gradient(&theme.gradient, p).c())
                        .add_modifier(Modifier::BOLD),
                )
            })
            .collect::<Vec<_>>(),
    )
}

pub fn center(area: Rect, w: u16, h: u16) -> Rect {
    let [r] = Layout::horizontal([Constraint::Length(w.min(area.width))])
        .flex(Flex::Center)
        .areas(area);
    let [r] = Layout::vertical([Constraint::Length(h.min(area.height))])
        .flex(Flex::Center)
        .areas(r);
    r
}

/// A bar of `width` cells filled to `frac`, using eighth blocks.
pub fn bar(frac: f64, width: u16) -> (String, String) {
    const EIGHTHS: [char; 8] = [' ', '▏', '▎', '▍', '▌', '▋', '▊', '▉'];
    let w = width as usize;
    let total = (frac.clamp(0.0, 1.0) * w as f64 * 8.0).round() as usize;
    let (full, rem) = (total / 8, total % 8);
    let mut filled = "█".repeat(full);
    if rem > 0 && full < w {
        filled.push(EIGHTHS[rem]);
    }
    let used = full + usize::from(rem > 0 && full < w);
    (filled, " ".repeat(w.saturating_sub(used)))
}

/// Footer of key hints: [("tab", "new drill"), …].
pub fn hints<'a>(items: &[(&'a str, &'a str)], theme: &Theme) -> Line<'a> {
    let mut spans = Vec::new();
    for (i, (k, v)) in items.iter().enumerate() {
        if i > 0 {
            spans.push(Span::styled("  ·  ", theme.faint()));
        }
        spans.push(Span::styled(*k, theme.fg(theme.accent2)));
        spans.push(Span::styled(format!(" {v}"), theme.dim()));
    }
    Line::from(spans).centered()
}

pub fn fmt_secs(secs: f64) -> String {
    let s = secs.max(0.0).round() as u64;
    if s >= 3600 {
        format!("{}h {:02}m", s / 3600, (s % 3600) / 60)
    } else {
        format!("{}:{:02}", s / 60, s % 60)
    }
}

/// Labels a visible key: space, newline and others need symbols.
pub fn key_label(c: char) -> String {
    match c {
        ' ' => "␣".into(),
        '\n' => "↵".into(),
        c => c.to_string(),
    }
}

fn draw_help(f: &mut Frame, app: &App) {
    let theme = &app.theme;
    let rows: &[(&str, &str)] = &[
        ("↑ ↓ / j k", "choose"),
        ("← → / h l", "change option (test length, language)"),
        ("enter", "start"),
        ("1-9", "jump to a mode"),
        ("s  ,", "stats  ·  settings"),
        ("", ""),
        ("tab / ctrl+n", "new drill (ctrl+n in code)"),
        ("ctrl+r", "restart the same text"),
        ("ctrl+k / F2", "cycle keyboard style"),
        ("ctrl+t / F3", "cycle theme"),
        ("ctrl+w", "delete word"),
        ("esc", "back to menu"),
        ("ctrl+c", "quit"),
    ];
    let area = center(f.area(), 56, rows.len() as u16 + 4);
    f.render_widget(Clear, area);
    let lines: Vec<Line> = rows
        .iter()
        .map(|(k, v)| {
            Line::from(vec![
                Span::styled(format!("{k:>14}  "), theme.fg(theme.accent2)),
                Span::styled(*v, theme.fg(theme.fg)),
            ])
        })
        .collect();
    f.render_widget(Paragraph::new(lines).block(panel("keys", theme)), area);
}

fn draw_toast(f: &mut Frame, msg: &str, theme: &Theme) {
    let area = f.area();
    let w = (msg.chars().count() as u16 + 4).min(area.width);
    if area.height < 3 {
        return;
    }
    let r = Rect::new(area.x + (area.width - w) / 2, area.bottom() - 2, w, 1);
    f.render_widget(Clear, r);
    f.render_widget(
        Paragraph::new(Line::from(format!(" {msg} ")).centered()).style(
            Style::new()
                .bg(theme.accent.c())
                .fg(theme.bg.c())
                .add_modifier(Modifier::BOLD),
        ),
        r,
    );
}

#[cfg(test)]
mod tests {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use ratatui::{Terminal, backend::TestBackend};

    use crate::{
        app::{App, STATS_TABS, Screen},
        config::{Config, KeyboardMode},
        stats::Profile,
        trainer::Mode,
    };

    fn render(app: &mut App, w: u16, h: u16) -> String {
        let mut term = Terminal::new(TestBackend::new(w, h)).unwrap();
        app.tick();
        term.draw(|f| super::draw(f, app)).unwrap();
        let buf = term.backend().buffer().clone();
        (0..h)
            .map(|y| {
                (0..w)
                    .map(|x| buf[(x, y)].symbol().to_string())
                    .collect::<String>()
                    .trim_end()
                    .to_string()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// An app that has finished several drills, typed with human-like timing.
    fn seasoned_app() -> App {
        use rand::{RngExt, SeedableRng, rngs::StdRng};
        let mut rng = StdRng::seed_from_u64(9);
        let mut app = App::new(Config::default(), None, Profile::default(), None).unwrap();
        let mut t = std::time::Instant::now();
        for mode in [
            Mode::Learn,
            Mode::Learn,
            Mode::Words,
            Mode::Learn,
            Mode::Code,
            Mode::Learn,
        ] {
            app.start(mode);
            let text: Vec<char> = app.drill.text.chars().collect();
            for c in text {
                if app.screen != Screen::Typing {
                    break;
                }
                t += std::time::Duration::from_millis(rng.random_range(120..320));
                let typed = if rng.random_bool(0.04) { '#' } else { c };
                app.type_char_at(typed, t);
            }
        }
        app
    }

    const SIZES: [(u16, u16); 6] = [(20, 6), (40, 12), (80, 24), (100, 30), (160, 50), (60, 40)];

    #[test]
    fn every_screen_renders_at_every_size() {
        let mut app = seasoned_app();
        for (w, h) in SIZES {
            app.screen = Screen::Home;
            render(&mut app, w, h);
            app.screen = Screen::Results;
            render(&mut app, w, h);
            app.screen = Screen::Settings;
            render(&mut app, w, h);
            app.screen = Screen::Stats;
            for tab in 0..STATS_TABS.len() {
                app.stats_tab = tab;
                render(&mut app, w, h);
            }
            for kb in KeyboardMode::ALL {
                app.config.keyboard = kb;
                for mode in [
                    Mode::Learn,
                    Mode::Code,
                    Mode::Test,
                    Mode::Ngrams,
                    Mode::Review,
                ] {
                    app.start(mode);
                    render(&mut app, w, h);
                    app.on_key(KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE));
                    render(&mut app, w, h);
                }
            }
            app.show_help = true;
            render(&mut app, w, h);
            app.show_help = false;
        }
    }

    #[test]
    fn fresh_profile_renders() {
        let mut app = App::new(Config::default(), None, Profile::default(), None).unwrap();
        for tab in 0..STATS_TABS.len() {
            app.screen = Screen::Stats;
            app.stats_tab = tab;
            render(&mut app, 100, 30);
        }
        let home = render(&mut app_home(), 100, 32);
        assert!(home.contains("Learn") && home.contains("Welcome"), "{home}");
    }

    fn app_home() -> App {
        App::new(Config::default(), None, Profile::default(), None).unwrap()
    }

    #[test]
    fn all_themes_render() {
        let mut app = seasoned_app();
        for name in super::theme::THEMES {
            app.theme = super::theme::Theme::by_name(name);
            app.truecolor = false;
            app.screen = Screen::Home;
            render(&mut app, 100, 30);
        }
    }

    /// Renders the frame as an HTML page with true cell colours.
    fn render_html(app: &mut App, w: u16, h: u16) -> String {
        use ratatui::style::Color;
        app.truecolor = true;
        let mut term = Terminal::new(TestBackend::new(w, h)).unwrap();
        app.tick();
        term.draw(|f| super::draw(f, app)).unwrap();
        let buf = term.backend().buffer().clone();
        let css = |c: Color, dflt: &str| match c {
            Color::Rgb(r, g, b) => format!("#{r:02x}{g:02x}{b:02x}"),
            _ => dflt.to_string(),
        };
        let bg0 = format!(
            "#{:02x}{:02x}{:02x}",
            app.theme.bg.0, app.theme.bg.1, app.theme.bg.2
        );
        let fg0 = format!(
            "#{:02x}{:02x}{:02x}",
            app.theme.fg.0, app.theme.fg.1, app.theme.fg.2
        );
        let mut out = format!(
            "<html><body style='margin:0;background:{bg0}'><pre style='font:16px/1.15 \"DejaVu Sans Mono\",monospace;margin:8px'>"
        );
        for y in 0..h {
            for x in 0..w {
                let c = &buf[(x, y)];
                let bold = c.modifier.contains(ratatui::style::Modifier::BOLD);
                let sym = match c.symbol() {
                    "<" => "&lt;".to_string(),
                    ">" => "&gt;".to_string(),
                    "&" => "&amp;".to_string(),
                    s => s.to_string(),
                };
                out += &format!(
                    "<span style='color:{};background:{}{}'>{}</span>",
                    css(c.fg, &fg0),
                    css(c.bg, &bg0),
                    if bold { ";font-weight:bold" } else { "" },
                    sym
                );
            }
            out += "\n";
        }
        out + "</pre></body></html>"
    }

    /// `KBTRAIN_DUMP_HTML=dir cargo test dump_screens -- --ignored` writes
    /// each screen as HTML; without it, prints them as text.
    #[test]
    #[ignore]
    fn dump_screens() {
        let mut app = seasoned_app();
        let (w, h) = (100, 34);
        if let Ok(dir) = std::env::var("KBTRAIN_DUMP_HTML") {
            let mut n = 0;
            let mut save = |app: &mut App, name: &str| {
                n += 1;
                std::fs::write(format!("{dir}/{n:02}-{name}.html"), render_html(app, w, h))
                    .unwrap();
            };
            app.screen = Screen::Home;
            save(&mut app, "home");
            app.screen = Screen::Results;
            save(&mut app, "results");
            app.start(Mode::Learn);
            let mut t = std::time::Instant::now();
            for c in app.drill.text.chars().take(30).collect::<Vec<_>>() {
                t += std::time::Duration::from_millis(170);
                app.type_char_at(c, t);
            }
            save(&mut app, "typing");
            app.start(Mode::Code);
            for c in app.drill.text.chars().take(40).collect::<Vec<_>>() {
                t += std::time::Duration::from_millis(170);
                app.type_char_at(if c == 'e' { 'x' } else { c }, t);
            }
            save(&mut app, "code");
            app.screen = Screen::Stats;
            for tab in 0..STATS_TABS.len() {
                app.stats_tab = tab;
                save(&mut app, &format!("stats{tab}"));
            }
            app.screen = Screen::Settings;
            save(&mut app, "settings");
            for name in ["midnight", "paper"] {
                app.theme = super::theme::Theme::by_name(name);
                app.screen = Screen::Home;
                save(&mut app, &format!("home-{name}"));
            }
            return;
        }
        app.screen = Screen::Home;
        println!("{}\n{}", render(&mut app, w, h), "=".repeat(w as usize));
        app.screen = Screen::Results;
        println!("{}\n{}", render(&mut app, w, h), "=".repeat(w as usize));
        app.start(Mode::Learn);
        for c in app.drill.text.chars().take(14).collect::<Vec<_>>() {
            app.on_key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE));
        }
        println!("{}\n{}", render(&mut app, w, h), "=".repeat(w as usize));
        app.start(Mode::Code);
        println!("{}\n{}", render(&mut app, w, h), "=".repeat(w as usize));
        app.screen = Screen::Stats;
        for tab in 0..STATS_TABS.len() {
            app.stats_tab = tab;
            println!("{}\n{}", render(&mut app, w, h), "=".repeat(w as usize));
        }
        app.screen = Screen::Settings;
        println!("{}", render(&mut app, w, h));
    }
}
