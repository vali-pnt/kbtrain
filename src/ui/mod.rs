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

use crate::{
    app::{App, Screen},
    config::Animations,
};
use fx::Scene;
use theme::{Rgb, Theme};

pub fn draw(f: &mut Frame, app: &App) {
    let theme = &app.theme;
    let anim = app.animations();
    f.render_widget(Block::new().style(theme.base()), f.area());
    let mut scene = Scene::default();
    match app.screen {
        Screen::Home => home::draw(f, f.area(), app),
        Screen::Typing => typing::draw(f, f.area(), app, &mut scene),
        Screen::Results => results::draw(f, f.area(), app, &mut scene),
        Screen::Stats => stats::draw(f, f.area(), app),
        Screen::Settings => settings::draw(f, f.area(), app),
    }
    if anim != Animations::Full {
        scene.lights.clear();
        scene.edge = None;
        scene.aurora = None;
    }
    // Shading is too subtle for 256 colours: it would only add banding.
    if app.truecolor {
        fx::light(f.buffer_mut(), theme, &scene, true);
    } else if scene.drain > 0.0 {
        fx::drain(f.buffer_mut(), theme, scene.drain);
    }
    if app.show_help {
        draw_help(f, app);
    }
    if let Some((msg, at)) = &app.toast {
        draw_toast(f, msg, at.elapsed().as_secs_f64(), app);
    }
    // Each screen rises out of the background.
    let age = app.screen_at.elapsed().as_secs_f64();
    if anim == Animations::Full && age < 0.22 {
        // Three steps: smooth enough to read as a fade, cheap to redraw.
        let k = (fx::ease_out(age / 0.22) * 3.0).ceil() / 3.0;
        let bg = theme.bg;
        fx::map_colors(f.buffer_mut(), |c| bg.lerp(c, k));
    }
    if !app.truecolor {
        theme::downgrade(f.buffer_mut());
    }
}

/// A card: a solid surface with soft half-cell edges and a tab-like title.
pub fn panel<'a>(title: &'a str, theme: &Theme) -> Block<'a> {
    let block = Block::new()
        .borders(Borders::ALL)
        .border_set(border::QUADRANT_INSIDE)
        .border_style(Style::new().fg(theme.panel.c()).bg(theme.bg.c()))
        .style(Style::new().bg(theme.panel.c()).fg(theme.fg.c()));
    if title.is_empty() {
        block
    } else {
        block.title(Span::styled(
            format!(" {title} "),
            Style::new()
                .bg(theme.panel.c())
                .fg(theme.dim.c())
                .add_modifier(Modifier::BOLD),
        ))
    }
}

/// A rounded pill: `text` on a `bg` surface with half-cell round ends.
pub fn pill<'a>(text: String, fg: Rgb, bg: Rgb, bold: bool) -> Vec<Span<'a>> {
    let mut style = Style::new().fg(fg.c()).bg(bg.c());
    if bold {
        style = style.add_modifier(Modifier::BOLD);
    }
    vec![
        Span::styled("▐", Style::new().fg(bg.c())),
        Span::styled(text, style),
        Span::styled("▌", Style::new().fg(bg.c())),
    ]
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

/// Footer of key hints, each key drawn as a little keycap.
pub fn hints<'a>(items: &[(&'a str, &'a str)], theme: &Theme) -> Line<'a> {
    let mut spans = Vec::new();
    let cap = theme.panel.lerp(theme.faint, 0.6);
    for (i, (k, v)) in items.iter().enumerate() {
        if i > 0 {
            spans.push(Span::raw("   "));
        }
        spans.extend(pill(k.to_string(), theme.fg, cap, false));
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

/// A pill that rises from the bottom edge, then sinks away.
fn draw_toast(f: &mut Frame, msg: &str, age: f64, app: &App) {
    let theme = &app.theme;
    let area = f.area();
    let w = (msg.chars().count() as u16 + 4).min(area.width);
    if area.height < 3 {
        return;
    }
    let rise = if app.animations() == Animations::Off {
        1.0
    } else {
        fx::ease_out_back(age / 0.25).min(fx::ease_out((4.0 - age) / 0.3))
    };
    let lift = (rise * 2.0).round() as u16;
    if lift == 0 {
        return;
    }
    let r = Rect::new(area.x + (area.width - w) / 2, area.bottom() - lift, w, 1);
    f.render_widget(Clear, r);
    f.render_widget(
        Paragraph::new(
            Line::from(pill(format!(" {msg} "), theme.bg, theme.accent, true)).centered(),
        )
        .style(theme.base()),
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

    /// Every animation at several moments, at every size: glitching and
    /// crumbling while typing, and each kind of results screen.
    #[test]
    fn effects_render_at_every_size() {
        let mut app = seasoned_app();
        for (w, h) in SIZES {
            for theme in ["synthwave", "paper"] {
                app.theme = super::theme::Theme::by_name(theme);
                app.start(Mode::Words);
                type_chars(&mut app, 30, &[27]);
                for t in [0.0, 0.05, 0.2, 0.5, 1.0] {
                    age(&mut app, t);
                    render(&mut app, w, h);
                }
                app.screen = Screen::Results;
                for (acc, prev, unlocked) in [
                    (0.99, Some(1.0), Some('e')),
                    (0.8, Some(500.0), None),
                    (0.95, Some(500.0), None),
                    (0.99, None, None),
                ] {
                    let r = app.results.as_mut().unwrap();
                    r.accuracy = acc;
                    r.prev_best = prev;
                    r.unlocked = unlocked;
                    r.at = std::time::Instant::now();
                    for t in [0.0, 0.3, 0.9, 1.5, 3.0] {
                        age(&mut app, t);
                        render(&mut app, w, h);
                    }
                }
                app.screen = Screen::Home;
                app.menu_sel = 5;
                render(&mut app, w, h);
                age(&mut app, 0.08);
                render(&mut app, w, h);
            }
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
        // Block elements are drawn as rectangles, the way terminals render
        // them (fonts leave gaps between rows).
        let rects = |s: &str| -> Option<&[(f32, f32, f32, f32)]> {
            Some(match s {
                "█" => &[(0.0, 0.0, 1.0, 1.0)],
                "▀" => &[(0.0, 0.0, 1.0, 0.5)],
                "▄" => &[(0.0, 0.5, 1.0, 1.0)],
                "▌" => &[(0.0, 0.0, 0.5, 1.0)],
                "▐" => &[(0.5, 0.0, 1.0, 1.0)],
                "▗" => &[(0.5, 0.5, 1.0, 1.0)],
                "▖" => &[(0.0, 0.5, 0.5, 1.0)],
                "▝" => &[(0.5, 0.0, 1.0, 0.5)],
                "▘" => &[(0.0, 0.0, 0.5, 0.5)],
                "▔" => &[(0.0, 0.0, 1.0, 0.125)],
                "▁" => &[(0.0, 0.875, 1.0, 1.0)],
                "▂" => &[(0.0, 0.75, 1.0, 1.0)],
                "▃" => &[(0.0, 0.625, 1.0, 1.0)],
                "▅" => &[(0.0, 0.375, 1.0, 1.0)],
                "▆" => &[(0.0, 0.25, 1.0, 1.0)],
                "▇" => &[(0.0, 0.125, 1.0, 1.0)],
                "▏" => &[(0.0, 0.0, 0.125, 1.0)],
                "▎" => &[(0.0, 0.0, 0.25, 1.0)],
                "▍" => &[(0.0, 0.0, 0.375, 1.0)],
                "▋" => &[(0.0, 0.0, 0.625, 1.0)],
                "▊" => &[(0.0, 0.0, 0.75, 1.0)],
                "▉" => &[(0.0, 0.0, 0.875, 1.0)],
                _ => return None,
            })
        };
        let mut out = format!(
            "<html><head><meta charset='utf-8'></head><body style='margin:0;background:{bg0}'><div style='font:16px \"DejaVu Sans Mono\",monospace;margin:8px;white-space:pre'>"
        );
        for y in 0..h {
            out += "<div style='height:20px'>";
            for x in 0..w {
                let c = &buf[(x, y)];
                let bold = c.modifier.contains(ratatui::style::Modifier::BOLD);
                let under = c.modifier.contains(ratatui::style::Modifier::UNDERLINED);
                let fg = css(c.fg, &fg0);
                let cell = format!(
                    "display:inline-block;position:relative;vertical-align:top;width:10px;height:20px;line-height:20px;text-align:center;overflow:visible;color:{fg};background:{}{}{}",
                    css(c.bg, &bg0),
                    if bold { ";font-weight:bold" } else { "" },
                    if under {
                        ";text-decoration:underline"
                    } else {
                        ""
                    },
                );
                if let Some(rs) = rects(c.symbol()) {
                    out += &format!("<span style='{cell}'>");
                    for (x0, y0, x1, y1) in rs {
                        out += &format!(
                            "<i style='position:absolute;left:{}px;top:{}px;width:{}px;height:{}px;background:{fg}'></i>",
                            x0 * 10.0,
                            y0 * 20.0,
                            (x1 - x0) * 10.0,
                            (y1 - y0) * 20.0
                        );
                    }
                    out += "</span>";
                    continue;
                }
                let sym = match c.symbol() {
                    "<" => "&lt;".to_string(),
                    ">" => "&gt;".to_string(),
                    "&" => "&amp;".to_string(),
                    s => s.to_string(),
                };
                out += &format!("<span style='{cell}'>{sym}</span>");
            }
            out += "</div>";
        }
        out + "</div></body></html>"
    }

    /// Shifts every feedback timestamp `secs` into the past, as if that much
    /// time had gone by.
    fn age(app: &mut App, secs: f64) {
        app.tick();
        let d = std::time::Duration::from_secs_f64(secs);
        let fb = &mut app.fb;
        for (_, at) in fb.hits.iter_mut().chain(fb.misses.iter_mut()) {
            *at -= d;
        }
        for (_, _, at) in fb.words.iter_mut() {
            *at -= d;
        }
        fb.caret_from.1 -= d;
        fb.shown_at -= d;
        if let Some((_, at)) = fb.tier_up.as_mut() {
            *at -= d;
        }
        for r in app.ripples.iter_mut() {
            r.at -= d;
        }
        if let Some((at, _)) = app.session.combo_broken.as_mut() {
            *at -= d;
        }
        if let Some(r) = app.results.as_mut() {
            r.at -= d;
        }
        app.screen_at -= d;
    }

    /// Types `n` characters of the drill, one every 60 ms, with real
    /// timestamps spread over the past so effects look mid-flight.
    fn type_chars(app: &mut App, n: usize, wrong_at: &[usize]) {
        let from = app.session.cursor();
        let text: Vec<char> = app.drill.text.chars().skip(from).take(n).collect();
        for (i, c) in text.into_iter().enumerate() {
            age(app, 0.06);
            let typed = if wrong_at.contains(&i) { '#' } else { c };
            app.type_char_at(typed, std::time::Instant::now());
        }
    }

    /// Prints the bytes a real terminal would receive while the home screen
    /// idles for a second, then a drill is typed at ~11 keys/s (every key
    /// wrong with `SLOPPY=1`), then the drill idles.
    /// `cargo test --release redraw_cost -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn redraw_cost() {
        use ratatui::backend::CrosstermBackend;
        use std::{cell::Cell, rc::Rc};
        /// Counts what the terminal would be sent.
        struct Meter(Rc<Cell<usize>>);
        impl std::io::Write for Meter {
            fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
                self.0.set(self.0.get() + b.len());
                Ok(b.len())
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        let bytes = Rc::new(Cell::new(0));
        let mut app = seasoned_app();
        app.truecolor = true;
        app.screen = Screen::Home;
        let backend = CrosstermBackend::new(Meter(bytes.clone()));
        let mut term = Terminal::with_options(
            backend,
            ratatui::TerminalOptions {
                viewport: ratatui::Viewport::Fixed(ratatui::layout::Rect::new(0, 0, 100, 34)),
            },
        )
        .unwrap();
        let frame = |app: &mut App, term: &mut Terminal<CrosstermBackend<Meter>>| {
            std::thread::sleep(std::time::Duration::from_millis(33));
            app.tick();
            term.draw(|f| super::draw(f, app)).unwrap();
            bytes.replace(0)
        };
        let home: usize = (0..30).map(|_| frame(&mut app, &mut term)).sum();
        app.start(Mode::Learn);
        let text: Vec<char> = app.drill.text.chars().collect();
        let mut typing = 0;
        for i in 0..90 {
            if i % 3 == 0 && i / 3 < text.len() {
                let c = text[i / 3];
                let wrong = std::env::var("SLOPPY").is_ok() || i / 3 == 12 || i / 3 == 13;
                app.type_char_at(if wrong { '#' } else { c }, std::time::Instant::now());
            }
            typing += frame(&mut app, &mut term);
        }
        let idle: usize = (0..30).map(|_| frame(&mut app, &mut term)).sum();
        println!("home 1 s: {home} B   typing 3 s: {typing} B   idle 1 s: {idle} B");
    }

    /// `KBTRAIN_DUMP_HTML=dir cargo test dump_screens -- --ignored` writes
    /// each screen (and moments of its animations) as HTML; without it,
    /// prints them as text.
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
            age(&mut app, 1.0);
            save(&mut app, "home");
            app.screen = Screen::Results;
            let real = app.results.clone();
            for t in [0.3, 2.0] {
                age(&mut app, t);
                save(&mut app, &format!("results-{t}"));
            }
            let r = app.results.as_mut().unwrap();
            r.prev_best = Some(r.wpm - 3.0);
            r.accuracy = 0.98;
            r.unlocked = Some('e');
            r.at = std::time::Instant::now();
            for t in [0.5, 1.2, 3.0] {
                age(&mut app, t);
                save(&mut app, &format!("results-best-{t}"));
            }
            let r = app.results.as_mut().unwrap();
            r.prev_best = Some(r.wpm + 10.0);
            r.accuracy = 0.84;
            r.unlocked = None;
            r.at = std::time::Instant::now();
            age(&mut app, 2.5);
            save(&mut app, "results-sloppy");
            app.results = real;
            app.start(Mode::Learn);
            age(&mut app, 1.0);
            type_chars(&mut app, 30, &[]);
            save(&mut app, "typing");
            type_chars(&mut app, 4, &[3]);
            age(&mut app, 0.08);
            save(&mut app, "typing-miss");
            age(&mut app, 0.3);
            save(&mut app, "typing-crumble");
            app.start(Mode::Words);
            age(&mut app, 1.0);
            type_chars(&mut app, 10, &[]);
            save(&mut app, "typing-flow");
            type_chars(&mut app, 16, &[]);
            age(&mut app, 0.1);
            save(&mut app, "typing-tierup");
            app.start(Mode::Code);
            age(&mut app, 1.0);
            type_chars(&mut app, 40, &[5, 20]);
            age(&mut app, 1.0);
            save(&mut app, "code");
            app.screen = Screen::Stats;
            age(&mut app, 1.0);
            for tab in 0..STATS_TABS.len() {
                app.stats_tab = tab;
                save(&mut app, &format!("stats{tab}"));
            }
            app.screen = Screen::Settings;
            app.tick();
            age(&mut app, 1.0);
            save(&mut app, "settings");
            for name in ["midnight", "paper"] {
                app.theme = super::theme::Theme::by_name(name);
                app.screen = Screen::Home;
                app.tick();
                age(&mut app, 1.0);
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
