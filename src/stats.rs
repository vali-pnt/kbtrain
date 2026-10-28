//! Per-key performance model. Tracks latency and accuracy with exponential
//! moving averages so recent performance matters more than old history.

use std::collections::HashMap;

const ALPHA: f64 = 0.15;

#[derive(Debug, Clone, Default)]
pub struct KeyStat {
    pub hits: u32,
    pub misses: u32,
    /// EWMA of inter-key latency in milliseconds.
    pub latency_ms: Option<f64>,
    /// EWMA of error rate in [0, 1].
    pub error_rate: f64,
}

impl KeyStat {
    pub fn record(&mut self, correct: bool, latency_ms: Option<f64>) {
        if correct {
            self.hits += 1;
        } else {
            self.misses += 1;
        }
        let e = if correct { 0.0 } else { 1.0 };
        self.error_rate = self.error_rate * (1.0 - ALPHA) + e * ALPHA;
        if let (true, Some(l)) = (correct, latency_ms) {
            self.latency_ms = Some(match self.latency_ms {
                Some(prev) => prev * (1.0 - ALPHA) + l * ALPHA,
                None => l,
            });
        }
    }

    /// Normalised weakness score in [0, 1]; higher means needs more practice.
    pub fn weakness(&self) -> f64 {
        let lat = self.latency_ms.map_or(1.0, |l| (l / 600.0).clamp(0.0, 1.0));
        (0.6 * lat + 0.4 * self.error_rate * 4.0).clamp(0.0, 1.0)
    }
}

#[derive(Debug, Default)]
pub struct Stats {
    pub keys: HashMap<char, KeyStat>,
}

impl Stats {
    pub fn record(&mut self, ch: char, correct: bool, latency_ms: Option<f64>) {
        self.keys
            .entry(ch.to_ascii_lowercase())
            .or_default()
            .record(correct, latency_ms);
    }

    pub fn weakness(&self, ch: char) -> Option<f64> {
        self.keys.get(&ch.to_ascii_lowercase()).map(KeyStat::weakness)
    }
}
