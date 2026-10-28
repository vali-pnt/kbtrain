//! A single typing drill: target text, typed input, and keystroke timing.

use std::time::{Duration, Instant};

#[derive(Debug, Clone)]
pub struct Keystroke {
    pub expected: char,
    pub typed: char,
    pub at: Duration,
}

impl Keystroke {
    pub fn correct(&self) -> bool {
        self.expected == self.typed
    }
}

#[derive(Debug)]
pub struct Session {
    pub target: Vec<char>,
    pub input: Vec<char>,
    pub strokes: Vec<Keystroke>,
    started: Option<Instant>,
    finished: Option<Instant>,
}

impl Session {
    pub fn new(text: String) -> Self {
        Self {
            target: text.chars().collect(),
            input: Vec::new(),
            strokes: Vec::new(),
            started: None,
            finished: None,
        }
    }

    pub fn is_done(&self) -> bool {
        self.finished.is_some()
    }

    pub fn expected(&self) -> Option<char> {
        self.target.get(self.input.len()).copied()
    }

    /// Records a typed char. Returns the keystroke and its latency since the
    /// previous stroke, if any.
    pub fn type_char(&mut self, c: char) -> Option<(Keystroke, Option<f64>)> {
        let expected = self.expected()?;
        let now = Instant::now();
        let start = *self.started.get_or_insert(now);
        let at = now - start;
        let latency = self.strokes.last().map(|s| (at - s.at).as_secs_f64() * 1000.0);
        let stroke = Keystroke { expected, typed: c, at };
        self.strokes.push(stroke.clone());
        self.input.push(c);
        if self.input.len() == self.target.len() {
            self.finished = Some(now);
        }
        Some((stroke, latency))
    }

    pub fn backspace(&mut self) {
        if !self.is_done() {
            self.input.pop();
        }
    }

    pub fn elapsed(&self) -> Duration {
        match (self.started, self.finished) {
            (Some(s), Some(f)) => f - s,
            (Some(s), None) => s.elapsed(),
            _ => Duration::ZERO,
        }
    }

    /// Words per minute over correctly typed characters (5 chars = 1 word).
    pub fn wpm(&self) -> f64 {
        let secs = self.elapsed().as_secs_f64();
        if secs < 0.5 {
            return 0.0;
        }
        let correct = self
            .input
            .iter()
            .zip(&self.target)
            .filter(|(a, b)| a == b)
            .count();
        correct as f64 / 5.0 / (secs / 60.0)
    }

    pub fn accuracy(&self) -> f64 {
        if self.strokes.is_empty() {
            return 1.0;
        }
        self.strokes.iter().filter(|s| s.correct()).count() as f64 / self.strokes.len() as f64
    }
}
