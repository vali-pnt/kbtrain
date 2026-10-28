use std::time::Duration;

use anyhow::{Result, anyhow};
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::DefaultTerminal;

use crate::{
    config::Config,
    layout::{self, Layout},
    session::{ErrorMode, Outcome, Session},
    stats::LayoutProfile,
    trainer::Trainer,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    Typing,
    Results,
}

pub struct App {
    pub config: Config,
    pub layout: Layout,
    pub stats: LayoutProfile,
    pub trainer: Trainer,
    pub session: Session,
    pub screen: Screen,
    /// Number of keys (in learning order) currently unlocked.
    pub unlocked: usize,
    /// Last key pressed and whether it was correct — drives keyboard flash.
    pub last_press: Option<(char, bool)>,
    quit: bool,
}

impl App {
    pub fn new(config: Config) -> Result<Self> {
        let layout = layout::builtin()
            .into_iter()
            .find(|l| l.id == config.layout)
            .ok_or_else(|| anyhow!("unknown layout '{}' (try --list-layouts)", config.layout))?;
        let stats = LayoutProfile::default();
        let trainer = Trainer::new();
        let unlocked = 26;
        let session = Session::new(
            &trainer.drill(&layout, &stats, unlocked, config.drill_length),
            ErrorMode::default(),
        );
        Ok(Self {
            config,
            layout,
            stats,
            trainer,
            session,
            screen: Screen::Typing,
            unlocked,
            last_press: None,
            quit: false,
        })
    }

    pub fn run(&mut self, terminal: &mut DefaultTerminal) -> Result<()> {
        while !self.quit {
            terminal.draw(|f| crate::ui::draw(f, self))?;
            // Poll so live stats (WPM, timer) keep refreshing while idle.
            if event::poll(Duration::from_millis(50))? {
                if let Event::Key(key) = event::read()? {
                    if key.kind == KeyEventKind::Press {
                        self.on_key(key);
                    }
                }
            }
        }
        Ok(())
    }

    fn new_drill(&mut self) {
        let text = self
            .trainer
            .drill(&self.layout, &self.stats, self.unlocked, self.config.drill_length);
        self.session = Session::new(&text, ErrorMode::default());
        self.last_press = None;
        self.screen = Screen::Typing;
    }

    fn on_key(&mut self, key: KeyEvent) {
        let ctrl_c = key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c');
        if ctrl_c || key.code == KeyCode::Esc {
            self.quit = true;
            return;
        }
        match (self.screen, key.code) {
            (Screen::Typing, KeyCode::Tab) => self.new_drill(),
            (Screen::Typing, KeyCode::Backspace) => self.session.backspace(),
            (Screen::Typing, KeyCode::Char(c)) => {
                if let Outcome::Typed { expected, correct, latency_ms, context, .. } = self.session.type_char(c) {
                    self.stats.record(expected, correct, latency_ms, &context);
                    self.last_press = Some((c, correct));
                }
                if self.session.is_done() {
                    self.screen = Screen::Results;
                }
            }
            (Screen::Results, KeyCode::Enter | KeyCode::Tab | KeyCode::Char(' ')) => {
                self.new_drill()
            }
            _ => {}
        }
    }
}
