use std::{
    io::Write,
    path::PathBuf,
    time::{Duration, Instant},
};

use anyhow::{Result, anyhow};
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::DefaultTerminal;

use crate::{
    config::{Animations, ColorDepth, Config, KeyboardMode, TEST_SECONDS},
    content::{self, LANGUAGES},
    layout::{self, Layout},
    session::{ErrorMode, Outcome, Session},
    stats::{LayoutProfile, Profile, SessionRecord, now_ts, today, wpm_to_latency},
    trainer::{self, Drill, Mode, Options, Trainer},
    ui::{
        fx::Particles,
        theme::{THEMES, Theme},
    },
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    Home,
    Typing,
    Results,
    Stats,
    Settings,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuItem {
    Mode(Mode),
    Stats,
    Settings,
    Quit,
}

pub const MENU: [MenuItem; 10] = [
    MenuItem::Mode(Mode::Learn),
    MenuItem::Mode(Mode::Review),
    MenuItem::Mode(Mode::Ngrams),
    MenuItem::Mode(Mode::Words),
    MenuItem::Mode(Mode::Test),
    MenuItem::Mode(Mode::Code),
    MenuItem::Mode(Mode::File),
    MenuItem::Stats,
    MenuItem::Settings,
    MenuItem::Quit,
];

pub const STATS_TABS: [&str; 5] = ["Progress", "Keys", "Transitions", "Fingers", "Calendar"];

/// A key press echoed on the on-screen keyboard.
#[derive(Debug, Clone)]
pub struct Ripple {
    pub ch: char,
    pub at: Instant,
    pub ok: bool,
}

/// Everything the results screen shows about a finished drill.
#[derive(Debug, Clone)]
pub struct Results {
    pub mode: Mode,
    pub title: String,
    pub wpm: f64,
    pub raw_wpm: f64,
    pub accuracy: f64,
    pub consistency: f64,
    pub secs: f64,
    pub errors: u32,
    pub max_combo: u32,
    pub prev_best: Option<f64>,
    pub unlocked: Option<char>,
    pub timeline: Vec<(f64, f64, f64, u32)>,
    pub key_errors: Vec<(char, u32)>,
    pub trouble_words: Vec<String>,
    pub goal_before: f64,
    pub at: Instant,
}

impl Results {
    pub fn is_pb(&self) -> bool {
        self.prev_best.is_some_and(|b| self.wpm > b) && self.accuracy >= 0.9
    }
}

pub struct App {
    pub config: Config,
    config_path: Option<PathBuf>,
    pub layouts: Vec<Layout>,
    pub layout: Layout,
    pub profile: Profile,
    profile_path: Option<PathBuf>,
    pub trainer: Trainer,
    pub theme: Theme,
    pub truecolor: bool,
    pub screen: Screen,
    pub show_help: bool,
    pub drill: Drill,
    pub session: Session,
    pub results: Option<Results>,
    pub menu_sel: usize,
    pub stats_tab: usize,
    pub settings_sel: usize,
    pub confirm_reset: bool,
    pub particles: Particles,
    pub ripples: Vec<Ripple>,
    /// Opacity of the adaptive keyboard, animated toward its target.
    pub kb_alpha: f64,
    /// Seconds since launch; drives all animation.
    pub t: f64,
    start: Instant,
    last_tick: Instant,
    pub toast: Option<(String, Instant)>,
    /// The user's practice text: (text, is code, display name).
    pub file_text: Option<(String, bool, String)>,
    quit: bool,
}

impl App {
    pub fn new(
        config: Config,
        config_path: Option<PathBuf>,
        profile: Profile,
        profile_path: Option<PathBuf>,
    ) -> Result<Self> {
        let mut layouts = layout::builtin();
        for def in &config.layouts {
            let l = def.build().map_err(|e| anyhow!(e))?;
            layouts.retain(|x| x.id != l.id);
            layouts.push(l);
        }
        let layout = layouts
            .iter()
            .find(|l| l.id == config.layout)
            .cloned()
            .ok_or_else(|| anyhow!("unknown layout '{}' (try --list-layouts)", config.layout))?;
        let theme = Theme::by_name(&config.theme);
        let truecolor = config.color.truecolor();
        let now = Instant::now();
        let mut app = Self {
            config,
            config_path,
            layouts,
            layout,
            profile,
            profile_path,
            trainer: Trainer::new(),
            theme,
            truecolor,
            screen: Screen::Home,
            show_help: false,
            drill: Drill {
                mode: Mode::Learn,
                text: String::new(),
                title: String::new(),
                focus: None,
                time_limit: None,
                code: false,
                file_span: None,
            },
            session: Session::new("", ErrorMode::Continue),
            results: None,
            menu_sel: 0,
            stats_tab: 0,
            settings_sel: 0,
            confirm_reset: false,
            particles: Particles::default(),
            ripples: Vec::new(),
            kb_alpha: 1.0,
            t: 0.0,
            start: now,
            last_tick: now,
            toast: None,
            file_text: None,
            quit: false,
        };
        app.ensure_layout_profile();
        app.load_file_text();
        Ok(app)
    }

    fn ensure_layout_profile(&mut self) {
        let initial = self.layout.initial_unlocked();
        let lp = self.profile.layout_mut(&self.layout.id);
        if lp.unlocked == 0 {
            lp.unlocked = initial;
        }
    }

    fn load_file_text(&mut self) {
        self.file_text = None;
        let Some(path) = self.config.text_file.clone() else {
            return;
        };
        match content::load_text(&path) {
            Ok((text, _)) if text.trim().is_empty() => {
                self.notify(format!("{} is empty", path.display()))
            }
            Ok((text, code)) => {
                let name = path.file_name().map_or_else(
                    || path.display().to_string(),
                    |n| n.to_string_lossy().into_owned(),
                );
                self.file_text = Some((text, code, name));
            }
            Err(e) => self.notify(format!("couldn't load text: {e}")),
        }
    }

    pub fn lp(&self) -> &LayoutProfile {
        static EMPTY: std::sync::OnceLock<LayoutProfile> = std::sync::OnceLock::new();
        self.profile
            .layout(&self.layout.id)
            .unwrap_or_else(|| EMPTY.get_or_init(LayoutProfile::default))
    }

    fn lp_mut(&mut self) -> &mut LayoutProfile {
        self.profile.layout_mut(&self.layout.id)
    }

    pub fn target_ms(&self) -> f64 {
        wpm_to_latency(self.config.target_wpm as f64)
    }

    pub fn options(&self) -> Options {
        Options {
            target_ms: self.target_ms(),
            words: self.config.drill_length,
            progressive: self.config.progressive_unlock,
            punctuation: self.config.punctuation,
            numbers: self.config.numbers,
            capitals: self.config.capitals,
            test_secs: self.config.test_seconds,
            code_lang: self.config.code_language.clone(),
        }
    }

    pub fn allowed_keys(&self) -> Vec<char> {
        Trainer::allowed(&self.layout, self.lp(), self.config.progressive_unlock)
    }

    pub fn mastered_count(&self) -> usize {
        let target = self.target_ms();
        self.layout
            .learning_order()
            .iter()
            .filter(|c| self.lp().key(**c).is_some_and(|s| s.mastered(target)))
            .count()
    }

    pub fn due_count(&self) -> usize {
        self.lp().due_count(now_ts(), self.target_ms())
    }

    pub fn animations(&self) -> Animations {
        self.config.animations
    }

    pub fn notify(&mut self, msg: impl Into<String>) {
        self.toast = Some((msg.into(), Instant::now()));
    }

    // ───────────────────────────── main loop ─────────────────────────────

    pub fn run(&mut self, terminal: &mut DefaultTerminal) -> Result<()> {
        while !self.quit {
            self.tick();
            terminal.draw(|f| crate::ui::draw(f, self))?;
            if event::poll(self.frame_interval())? {
                match event::read()? {
                    Event::Key(key) if key.kind != KeyEventKind::Release => self.on_key(key),
                    _ => {}
                }
                // Drain queued input before redrawing so fast typing never lags.
                while event::poll(Duration::ZERO)? {
                    if let Event::Key(key) = event::read()?
                        && key.kind != KeyEventKind::Release
                    {
                        self.on_key(key);
                    }
                }
            }
        }
        self.save_profile();
        Ok(())
    }

    fn frame_interval(&self) -> Duration {
        let ms = match (self.config.animations, self.screen) {
            (Animations::Off, Screen::Typing) => 100,
            (Animations::Off, _) => 250,
            (Animations::Reduced, _) => 50,
            (Animations::Full, Screen::Typing) => 33,
            (Animations::Full, Screen::Home) => 50,
            (Animations::Full, _) => 33,
        };
        Duration::from_millis(ms)
    }

    pub fn tick(&mut self) {
        let now = Instant::now();
        let dt = (now - self.last_tick).as_secs_f64().min(0.1);
        self.last_tick = now;
        self.t = (now - self.start).as_secs_f64();
        self.particles.update(dt, 18.0);
        self.ripples
            .retain(|r| r.at.elapsed() < Duration::from_millis(700));
        if self
            .toast
            .as_ref()
            .is_some_and(|(_, at)| at.elapsed() > Duration::from_secs(4))
        {
            self.toast = None;
        }

        if self.screen == Screen::Typing {
            if self.session.tick(now) {
                self.finish();
                return;
            }
            // Timed tests never run out of words.
            if self.session.time_limit.is_some()
                && self.session.target.len() - self.session.cursor() < 200
            {
                let more = self.trainer.generate(
                    Mode::Test,
                    &self.layout,
                    self.lp(),
                    &self.options(),
                    now_ts(),
                    &mut rand::rng(),
                );
                self.session.extend(&format!(" {}", more.text));
            }
            let target = self.adaptive_target();
            let rate = if target > self.kb_alpha { 6.0 } else { 1.2 };
            let step = rate * dt;
            self.kb_alpha = if (target - self.kb_alpha).abs() <= step {
                target
            } else {
                self.kb_alpha + step * (target - self.kb_alpha).signum()
            };
        }
    }

    /// The adaptive keyboard shows itself on hesitation or errors.
    fn adaptive_target(&self) -> f64 {
        let Some(last) = self.session.last_input() else {
            return 1.0;
        };
        let avg = self
            .lp()
            .key(self.session.expected().unwrap_or(' '))
            .and_then(|s| s.latency_ms)
            .unwrap_or(400.0);
        let hesitation = Duration::from_secs_f64((avg * 2.5 / 1000.0).max(0.9));
        let recent_error = self
            .session
            .since_error()
            .is_some_and(|d| d < Duration::from_millis(1200));
        if last.elapsed() > hesitation || recent_error {
            1.0
        } else {
            0.0
        }
    }

    // ───────────────────────────── input ─────────────────────────────

    pub fn on_key(&mut self, key: KeyEvent) {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        if ctrl && key.code == KeyCode::Char('c') {
            self.quit = true;
            return;
        }
        if self.show_help {
            self.show_help = false;
            return;
        }
        match self.screen {
            Screen::Home => self.on_home_key(key),
            Screen::Typing => self.on_typing_key(key),
            Screen::Results => self.on_results_key(key),
            Screen::Stats => self.on_stats_key(key),
            Screen::Settings => self.on_settings_key(key),
        }
    }

    fn on_home_key(&mut self, key: KeyEvent) {
        let n = MENU.len();
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => self.menu_sel = (self.menu_sel + n - 1) % n,
            KeyCode::Down | KeyCode::Char('j') => self.menu_sel = (self.menu_sel + 1) % n,
            KeyCode::Left | KeyCode::Char('h') => self.cycle_menu_option(-1),
            KeyCode::Right | KeyCode::Char('l') => self.cycle_menu_option(1),
            KeyCode::Enter | KeyCode::Char(' ') => self.activate(MENU[self.menu_sel]),
            KeyCode::Char(c @ '1'..='9') => {
                let i = c as usize - '1' as usize;
                if i < n {
                    self.menu_sel = i;
                    self.activate(MENU[i]);
                }
            }
            KeyCode::Char('s') => self.activate(MenuItem::Stats),
            KeyCode::Char(',') => self.activate(MenuItem::Settings),
            KeyCode::Char('?') => self.show_help = true,
            KeyCode::Esc | KeyCode::Char('q') => self.quit = true,
            _ => {}
        }
    }

    /// ←/→ on a menu item changes its inline option (test length, language).
    fn cycle_menu_option(&mut self, dir: i32) {
        match MENU[self.menu_sel] {
            MenuItem::Mode(Mode::Test) => {
                self.config.test_seconds = cycle(&TEST_SECONDS, &self.config.test_seconds, dir);
            }
            MenuItem::Mode(Mode::Code) => {
                let langs: Vec<String> = std::iter::once("any")
                    .chain(LANGUAGES)
                    .map(String::from)
                    .collect();
                self.config.code_language = cycle(&langs, &self.config.code_language, dir);
            }
            _ => return,
        }
        self.save_config();
    }

    fn activate(&mut self, item: MenuItem) {
        match item {
            MenuItem::Mode(m) => self.start(m),
            MenuItem::Stats => self.screen = Screen::Stats,
            MenuItem::Settings => {
                self.confirm_reset = false;
                self.screen = Screen::Settings;
            }
            MenuItem::Quit => self.quit = true,
        }
    }

    pub fn start(&mut self, mode: Mode) {
        let drill = if mode == Mode::File {
            let Some((text, code, name)) = &self.file_text else {
                self.notify("No text file set. Run: kbtrain --file <path>");
                return;
            };
            let offset = self.file_offset();
            Trainer::file_drill(text, *code, name, offset, self.config.drill_length)
        } else {
            self.trainer.generate(
                mode,
                &self.layout,
                self.lp(),
                &self.options(),
                now_ts(),
                &mut rand::rng(),
            )
        };
        self.begin(drill);
    }

    fn file_offset(&self) -> usize {
        self.config
            .text_file
            .as_ref()
            .and_then(|p| self.profile.file_progress.get(&p.display().to_string()))
            .copied()
            .unwrap_or(0)
    }

    fn begin(&mut self, drill: Drill) {
        let mut s = Session::new(&drill.text, self.config.error_mode);
        s.time_limit = drill.time_limit;
        s.auto_indent = self.config.auto_indent;
        self.session = s;
        self.drill = drill;
        self.ripples.clear();
        // Keep drills calm: no leftover celebration.
        self.particles.list.clear();
        self.kb_alpha = 1.0;
        self.screen = Screen::Typing;
    }

    fn retry(&mut self) {
        let drill = self.drill.clone();
        self.begin(drill);
    }

    /// Leaves a drill early; time spent still counts toward the daily goal.
    fn abandon(&mut self) {
        if self.session.strokes.len() >= 10 {
            let secs = self.session.elapsed();
            let chars = self.session.strokes.len() as u32;
            self.profile.add_practice(today(), secs, chars);
            self.save_profile();
        }
    }

    fn on_typing_key(&mut self, key: KeyEvent) {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        let alt = key.modifiers.contains(KeyModifiers::ALT);
        match key.code {
            KeyCode::Esc => {
                self.abandon();
                self.screen = Screen::Home;
            }
            // In code, Tab is an indentation habit, so only Ctrl+N skips.
            KeyCode::Tab if !self.drill.code => {
                self.abandon();
                self.start(self.drill.mode);
            }
            KeyCode::Char('n') if ctrl => {
                self.abandon();
                self.start(self.drill.mode);
            }
            KeyCode::Char('r') if ctrl => {
                self.abandon();
                self.retry();
            }
            KeyCode::Char('k') if ctrl => self.cycle_keyboard(),
            KeyCode::F(2) => self.cycle_keyboard(),
            KeyCode::Char('t') if ctrl => self.cycle_theme(),
            KeyCode::F(3) => self.cycle_theme(),
            KeyCode::Backspace if ctrl || alt => self.session.backspace_word(),
            KeyCode::Char('w' | 'h') if ctrl => self.session.backspace_word(),
            KeyCode::Backspace => self.session.backspace(),
            KeyCode::Enter => {
                if self.session.expected() == Some('\n') {
                    self.type_char('\n');
                }
            }
            KeyCode::Char(c) if !ctrl && !alt => self.type_char(c),
            _ => {}
        }
    }

    fn type_char(&mut self, c: char) {
        self.type_char_at(c, Instant::now());
    }

    pub(crate) fn type_char_at(&mut self, c: char, now: Instant) {
        match self.session.type_char_at(c, now) {
            Outcome::Typed {
                expected,
                correct,
                latency_ms,
                context,
                ..
            } => {
                self.lp_mut()
                    .record(expected, correct, latency_ms, &context);
                self.ripples.push(Ripple {
                    ch: c,
                    at: now.min(Instant::now()),
                    ok: correct,
                });
                if !correct {
                    self.bell();
                }
                if self.session.is_done() {
                    self.finish();
                }
            }
            Outcome::Blocked => self.bell(),
            Outcome::Ignored => {}
        }
    }

    fn bell(&self) {
        if self.config.bell_on_error {
            let mut out = std::io::stdout();
            let _ = out.write_all(b"\x07");
            let _ = out.flush();
        }
    }

    fn finish(&mut self) {
        let now = now_ts();
        let target = self.target_ms();
        let mode = self.drill.mode;
        let goal_before = self.profile.today_secs();
        let s = &self.session;
        let (wpm, raw_wpm, accuracy, consistency, secs) = (
            s.wpm(),
            s.raw_wpm(),
            s.accuracy(),
            s.consistency(),
            s.elapsed(),
        );
        let (chars, errors, max_combo) = (s.strokes.len() as u32, s.errors(), s.max_combo);
        let timeline = s.timeline();
        let mut key_errors: Vec<(char, u32)> = s
            .key_errors()
            .into_iter()
            .filter(|(c, _)| !c.is_whitespace())
            .collect();
        key_errors.sort_by_key(|e| std::cmp::Reverse(e.1));
        let mut trouble_words: Vec<String> = Vec::new();
        for w in s.word_results().into_iter().filter(|w| w.errors > 0) {
            let word = w
                .word
                .trim_matches(|c: char| !c.is_alphanumeric())
                .to_string();
            if word.chars().filter(|c| c.is_alphanumeric()).count() >= 2
                && !trouble_words.contains(&word)
            {
                trouble_words.push(word);
            }
        }

        let session = std::mem::replace(&mut self.session, Session::new("", ErrorMode::Continue));
        let layout = self.layout.clone();
        let lp = self.lp_mut();
        trainer::apply_reviews(lp, &session, target, now);
        let unlocked = if matches!(mode, Mode::Learn | Mode::Review | Mode::Ngrams) {
            trainer::try_unlock(&layout, lp, target, now)
        } else {
            None
        };
        let unlocked_total = lp.unlocked as u32;
        self.session = session;

        let mode_id = mode.id(self.config.test_seconds);
        let prev_best = self.profile.best_wpm(&self.layout.id, &mode_id, 0);
        let rec = SessionRecord {
            ts: now,
            layout: self.layout.id.clone(),
            mode: mode_id,
            wpm,
            raw_wpm,
            accuracy,
            consistency,
            secs,
            chars,
            errors,
            max_combo,
            mastered: self.mastered_count() as u32,
            unlocked: unlocked_total,
        };
        self.profile.add_session(today(), rec);
        if let (Some((_, end)), Some(path)) = (self.drill.file_span, &self.config.text_file) {
            let total = self
                .file_text
                .as_ref()
                .map_or(0, |(t, _, _)| t.chars().count());
            let next = if end >= total { 0 } else { end };
            self.profile
                .file_progress
                .insert(path.display().to_string(), next);
        }
        self.save_profile();

        let results = Results {
            mode,
            title: self.drill.title.clone(),
            wpm,
            raw_wpm,
            accuracy,
            consistency,
            secs,
            errors,
            max_combo,
            prev_best,
            unlocked,
            timeline,
            key_errors,
            trouble_words,
            goal_before,
            at: Instant::now(),
        };
        if self.config.animations != Animations::Off
            && (results.is_pb() || results.unlocked.is_some())
        {
            self.celebrate();
        }
        self.results = Some(results);
        self.screen = Screen::Results;
    }

    /// Fireworks across the top of the screen.
    fn celebrate(&mut self) {
        let (w, _) = crossterm::terminal::size().unwrap_or((80, 24));
        let colors = [
            self.theme.accent,
            self.theme.accent2,
            self.theme.accent3,
            self.theme.good,
        ];
        let mut rng = rand::rng();
        for i in 0..3 {
            let x = w as f64 * (0.25 + 0.25 * i as f64);
            self.particles
                .burst(x, 4.0 + i as f64, 40, &colors, &mut rng);
        }
    }

    fn on_results_key(&mut self, key: KeyEvent) {
        // Brief guard so a keystroke at the end of a drill doesn't skip results.
        let settled = self
            .results
            .as_ref()
            .is_none_or(|r| r.at.elapsed() > Duration::from_millis(400));
        match key.code {
            KeyCode::Enter | KeyCode::Tab => self.start(self.drill.mode),
            KeyCode::Char(' ') if settled => self.start(self.drill.mode),
            KeyCode::Char('r') if settled => self.retry(),
            KeyCode::Char('s') if settled => self.screen = Screen::Stats,
            KeyCode::Esc | KeyCode::Char('q') => self.screen = Screen::Home,
            _ => {}
        }
    }

    fn on_stats_key(&mut self, key: KeyEvent) {
        let n = STATS_TABS.len();
        match key.code {
            KeyCode::Right | KeyCode::Tab | KeyCode::Char('l') => {
                self.stats_tab = (self.stats_tab + 1) % n
            }
            KeyCode::Left | KeyCode::BackTab | KeyCode::Char('h') => {
                self.stats_tab = (self.stats_tab + n - 1) % n
            }
            KeyCode::Char(c @ '1'..='5') => self.stats_tab = c as usize - '1' as usize,
            KeyCode::Esc | KeyCode::Char('q') => self.screen = Screen::Home,
            _ => {}
        }
    }

    // ───────────────────────────── settings ─────────────────────────────

    /// (label, current value, help text) for each settings row.
    pub fn settings_rows(&self) -> Vec<(&'static str, String, &'static str)> {
        let c = &self.config;
        let on = |b: bool| if b { "on" } else { "off" }.to_string();
        vec![
            (
                "Layout",
                self.layout.name.clone(),
                "Keyboard layout to train on. Each layout keeps its own stats.",
            ),
            (
                "Theme",
                self.theme.name.to_string(),
                "Colour theme. Ctrl+T cycles themes while typing.",
            ),
            (
                "Keyboard",
                c.keyboard.label().into(),
                "On-screen keyboard style. Ctrl+K cycles it while typing.",
            ),
            (
                "Errors",
                c.error_mode.label().into(),
                "Continue past mistakes, stop until fixed, or require backspace.",
            ),
            (
                "Target speed",
                format!("{} wpm", c.target_wpm),
                "Keys count as mastered at this speed; the next key unlocks.",
            ),
            (
                "Daily goal",
                format!("{} min", c.daily_goal_minutes),
                "Minutes of practice per day to keep your streak.",
            ),
            (
                "Drill length",
                format!("{} words", c.drill_length),
                "Words per lesson.",
            ),
            (
                "Progressive unlock",
                on(c.progressive_unlock),
                "Start on the home row and unlock keys as you master them.",
            ),
            (
                "Punctuation",
                on(c.punctuation),
                "Add punctuation to Words and Timed test.",
            ),
            (
                "Numbers",
                on(c.numbers),
                "Add numbers to Words and Timed test.",
            ),
            (
                "Capitals",
                on(c.capitals),
                "Capitalize sentences in Words and Timed test.",
            ),
            (
                "Code language",
                c.code_language.clone(),
                "Language for Code drills.",
            ),
            (
                "Auto-indent",
                on(c.auto_indent),
                "Skip indentation after pressing Enter in code.",
            ),
            (
                "Animations",
                c.animations.label().into(),
                "Particles, glow and motion. Reduce if it distracts.",
            ),
            (
                "Colors",
                c.color.label().into(),
                "Use 256 colors if your terminal shows odd colours.",
            ),
            (
                "Bell on error",
                on(c.bell_on_error),
                "Ring the terminal bell on mistakes.",
            ),
            (
                "Live stats",
                on(c.live_stats),
                "Show speed and accuracy while typing.",
            ),
            (
                "Unlock next key",
                String::new(),
                "Skip ahead: unlock the next key now.",
            ),
            (
                "Reset layout stats",
                if self.confirm_reset {
                    "press enter again to confirm".into()
                } else {
                    String::new()
                },
                "Erase all progress for this layout. History is kept.",
            ),
        ]
    }

    fn on_settings_key(&mut self, key: KeyEvent) {
        let n = self.settings_rows().len();
        if !matches!(key.code, KeyCode::Enter) {
            self.confirm_reset = false;
        }
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => self.settings_sel = (self.settings_sel + n - 1) % n,
            KeyCode::Down | KeyCode::Char('j') => self.settings_sel = (self.settings_sel + 1) % n,
            KeyCode::Left | KeyCode::Char('h') => self.change_setting(-1),
            KeyCode::Right | KeyCode::Char('l') | KeyCode::Char(' ') => self.change_setting(1),
            KeyCode::Enter => self.change_setting(0),
            KeyCode::Esc | KeyCode::Char('q') => self.screen = Screen::Home,
            _ => {}
        }
    }

    /// Applies a change to the selected setting (dir 0 = enter/activate).
    fn change_setting(&mut self, dir: i32) {
        let step = if dir == 0 { 1 } else { dir };
        let c = &mut self.config;
        match self.settings_sel {
            0 => {
                let ids: Vec<String> = self.layouts.iter().map(|l| l.id.clone()).collect();
                c.layout = cycle(&ids, &c.layout, step);
                self.layout = self
                    .layouts
                    .iter()
                    .find(|l| l.id == self.config.layout)
                    .cloned()
                    .unwrap();
                self.ensure_layout_profile();
            }
            1 => {
                let names: Vec<String> = THEMES.iter().map(|s| s.to_string()).collect();
                c.theme = cycle(&names, &c.theme, step);
                self.theme = Theme::by_name(&self.config.theme);
            }
            2 => c.keyboard = cycle(&KeyboardMode::ALL, &c.keyboard, step),
            3 => c.error_mode = cycle(&ErrorMode::ALL, &c.error_mode, step),
            4 => c.target_wpm = (c.target_wpm as i32 + 5 * step).clamp(10, 200) as u32,
            5 => {
                c.daily_goal_minutes = (c.daily_goal_minutes as i32 + 5 * step).clamp(5, 180) as u32
            }
            6 => c.drill_length = (c.drill_length as i32 + 5 * step).clamp(5, 200) as usize,
            7 => c.progressive_unlock ^= true,
            8 => c.punctuation ^= true,
            9 => c.numbers ^= true,
            10 => c.capitals ^= true,
            11 => {
                let langs: Vec<String> = std::iter::once("any")
                    .chain(LANGUAGES)
                    .map(String::from)
                    .collect();
                c.code_language = cycle(&langs, &c.code_language, step);
            }
            12 => c.auto_indent ^= true,
            13 => c.animations = cycle(&Animations::ALL, &c.animations, step),
            14 => {
                c.color = cycle(&ColorDepth::ALL, &c.color, step);
                self.truecolor = self.config.color.truecolor();
            }
            15 => c.bell_on_error ^= true,
            16 => c.live_stats ^= true,
            17 if dir == 0 => {
                let order = self.layout.learning_order();
                let lp = self.lp_mut();
                if lp.unlocked < order.len() {
                    let next = order[lp.unlocked];
                    lp.unlocked += 1;
                    lp.unlock_log.push((now_ts(), next));
                    self.notify(format!("Unlocked '{next}'"));
                    self.save_profile();
                } else {
                    self.notify("Every key is already unlocked");
                }
            }
            18 if dir == 0 => {
                if self.confirm_reset {
                    self.profile.layouts.remove(&self.layout.id);
                    self.ensure_layout_profile();
                    self.save_profile();
                    self.confirm_reset = false;
                    self.notify(format!("Stats reset for {}", self.layout.name));
                } else {
                    self.confirm_reset = true;
                }
            }
            _ => {}
        }
        self.save_config();
    }

    fn cycle_keyboard(&mut self) {
        self.config.keyboard = cycle(&KeyboardMode::ALL, &self.config.keyboard, 1);
        self.kb_alpha = 1.0;
        self.notify(format!("keyboard: {}", self.config.keyboard.label()));
        self.save_config();
    }

    fn cycle_theme(&mut self) {
        let names: Vec<String> = THEMES.iter().map(|s| s.to_string()).collect();
        self.config.theme = cycle(&names, &self.config.theme, 1);
        self.theme = Theme::by_name(&self.config.theme);
        self.notify(format!("theme: {}", self.config.theme));
        self.save_config();
    }

    // ───────────────────────────── persistence ─────────────────────────────

    fn save_profile(&mut self) {
        if let Some(p) = &self.profile_path
            && let Err(e) = self.profile.save(p)
        {
            self.notify(format!("couldn't save progress: {e}"));
        }
    }

    fn save_config(&mut self) {
        if let Some(p) = &self.config_path
            && let Err(e) = self.config.save(p)
        {
            self.notify(format!("couldn't save settings: {e}"));
        }
    }
}

/// The element after (or before) `cur` in `items`, wrapping around.
fn cycle<T: Clone + PartialEq>(items: &[T], cur: &T, dir: i32) -> T {
    let n = items.len() as i32;
    let i = items.iter().position(|x| x == cur).unwrap_or(0) as i32;
    items[((i + dir).rem_euclid(n)) as usize].clone()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app() -> App {
        App::new(Config::default(), None, Profile::default(), None).unwrap()
    }

    fn press(app: &mut App, code: KeyCode) {
        app.on_key(KeyEvent::new(code, KeyModifiers::NONE));
    }

    #[test]
    fn full_drill_flow_records_history() {
        let mut a = app();
        a.start(Mode::Learn);
        assert_eq!(a.screen, Screen::Typing);
        let text = a.drill.text.clone();
        for c in text.chars() {
            press(&mut a, KeyCode::Char(c));
        }
        assert_eq!(a.screen, Screen::Results);
        assert_eq!(a.profile.history.len(), 1);
        assert_eq!(a.profile.history[0].mode, "learn");
        assert!(a.lp().total_keystrokes() as usize >= text.chars().count());
        press(&mut a, KeyCode::Enter);
        assert_eq!(a.screen, Screen::Typing);
        press(&mut a, KeyCode::Esc);
        assert_eq!(a.screen, Screen::Home);
    }

    #[test]
    fn menu_cycles_test_length_and_settings_change() {
        let mut a = app();
        a.menu_sel = 4;
        press(&mut a, KeyCode::Right);
        assert_eq!(a.config.test_seconds, 60);
        a.screen = Screen::Settings;
        a.settings_sel = 0;
        press(&mut a, KeyCode::Right);
        assert_eq!(a.layout.id, "colemak");
        assert!(a.profile.layouts.contains_key("colemak"));
        a.settings_sel = 4;
        press(&mut a, KeyCode::Left);
        assert_eq!(a.config.target_wpm, 30);
    }

    #[test]
    fn reset_requires_confirmation() {
        let mut a = app();
        a.lp_mut().record('a', true, Some(100.0), &[]);
        a.screen = Screen::Settings;
        a.settings_sel = 18;
        press(&mut a, KeyCode::Enter);
        assert!(a.lp().total_keystrokes() > 0);
        press(&mut a, KeyCode::Enter);
        assert_eq!(a.lp().total_keystrokes(), 0);
    }

    #[test]
    fn file_mode_without_file_stays_home() {
        let mut a = app();
        a.start(Mode::File);
        assert_eq!(a.screen, Screen::Home);
        assert!(a.toast.is_some());
    }

    #[test]
    fn code_drill_accepts_enter() {
        let mut a = app();
        a.start(Mode::Code);
        let text = a.drill.text.clone();
        let mut chars = text.chars().peekable();
        while let Some(c) = chars.next() {
            if a.screen != Screen::Typing {
                break;
            }
            if c == '\n' {
                press(&mut a, KeyCode::Enter);
                // Indentation is skipped automatically.
                while chars.peek() == Some(&' ') {
                    chars.next();
                }
            } else {
                press(&mut a, KeyCode::Char(c));
            }
        }
        assert_eq!(a.screen, Screen::Results);
        assert_eq!(a.results.as_ref().unwrap().errors, 0);
    }
}
