//! Finger, hand and transition analysis derived from the learner profile.

use crate::{
    layout::{Finger, Hand, Layout},
    stats::LayoutProfile,
};

#[derive(Debug, Clone)]
pub struct FingerStat {
    pub finger: Finger,
    /// Share of all keystrokes.
    pub share: f64,
    pub latency_ms: Option<f64>,
    pub error_rate: f64,
}

/// Load, speed and accuracy per finger.
pub fn fingers(layout: &Layout, lp: &LayoutProfile) -> Vec<FingerStat> {
    let total = lp.total_keystrokes().max(1) as f64;
    Finger::ALL
        .iter()
        .map(|&finger| {
            let stats: Vec<_> = lp
                .keys
                .iter()
                .filter(|(c, _)| layout.finger_of(**c) == Some(finger))
                .map(|(_, s)| s)
                .collect();
            let presses: u64 = stats.iter().map(|s| s.count() as u64).sum();
            let weighted = |f: &dyn Fn(&crate::stats::Stat) -> Option<f64>| {
                let (sum, n) = stats
                    .iter()
                    .filter_map(|s| f(s).map(|v| (v * s.count() as f64, s.count() as f64)))
                    .fold((0.0, 0.0), |a, b| (a.0 + b.0, a.1 + b.1));
                (n > 0.0).then(|| sum / n)
            };
            FingerStat {
                finger,
                share: presses as f64 / total,
                latency_ms: weighted(&|s| s.latency_ms),
                error_rate: weighted(&|s| Some(s.error_rate)).unwrap_or(0.0),
            }
        })
        .collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Transition {
    /// Hands alternate.
    Alternate,
    /// Same hand, different fingers (a roll).
    SameHand,
    /// Same finger, different keys: the slowest kind.
    SameFinger,
    /// Same key twice.
    Repeat,
}

pub fn classify(layout: &Layout, a: char, b: char) -> Option<Transition> {
    if a == b {
        return Some(Transition::Repeat);
    }
    let (fa, fb) = (layout.finger_of(a)?, layout.finger_of(b)?);
    Some(if fa == fb {
        Transition::SameFinger
    } else if fa.hand() == fb.hand() && fa.hand() != Hand::Thumb {
        Transition::SameHand
    } else {
        Transition::Alternate
    })
}

#[derive(Debug, Clone, Default)]
pub struct TransitionStats {
    /// (share of bigrams, mean latency) for alternate / same-hand / same-finger.
    pub alternate: (f64, Option<f64>),
    pub same_hand: (f64, Option<f64>),
    pub same_finger: (f64, Option<f64>),
}

pub fn transitions(layout: &Layout, lp: &LayoutProfile) -> TransitionStats {
    let mut acc = [(0u64, 0.0f64, 0u64); 3];
    for (g, s) in &lp.bigrams {
        let cs: Vec<char> = g.chars().collect();
        let i = match classify(layout, cs[0], cs[1]) {
            Some(Transition::Alternate) => 0,
            Some(Transition::SameHand) => 1,
            Some(Transition::SameFinger) => 2,
            _ => continue,
        };
        acc[i].0 += s.count() as u64;
        if let Some(l) = s.latency_ms {
            acc[i].1 += l * s.hits as f64;
            acc[i].2 += s.hits as u64;
        }
    }
    let total: u64 = acc.iter().map(|a| a.0).sum();
    let f = |a: (u64, f64, u64)| {
        (
            a.0 as f64 / total.max(1) as f64,
            (a.2 > 0).then(|| a.1 / a.2 as f64),
        )
    };
    TransitionStats {
        alternate: f(acc[0]),
        same_hand: f(acc[1]),
        same_finger: f(acc[2]),
    }
}

/// Share of keystrokes on each row: number, top, home, bottom.
pub fn rows(layout: &Layout, lp: &LayoutProfile) -> [f64; 4] {
    let mut n = [0u64; 4];
    for (c, s) in &lp.keys {
        if let Some((k, _)) = layout.locate(*c) {
            n[k.row] += s.count() as u64;
        }
    }
    let total = n.iter().sum::<u64>().max(1) as f64;
    n.map(|v| v as f64 / total)
}

/// How a layout handles English: (same-finger %, alternation %, home-row %),
/// measured over the bigrams and letters of `words`.
pub fn layout_profile(layout: &Layout, words: &[&str]) -> (f64, f64, f64) {
    let (mut sfb, mut alt, mut pairs, mut home, mut letters) = (0u32, 0u32, 0u32, 0u32, 0u32);
    for w in words {
        let cs: Vec<char> = w.chars().collect();
        for c in &cs {
            letters += 1;
            if layout
                .locate(*c)
                .is_some_and(|(k, _)| k.row == crate::layout::HOME_ROW)
            {
                home += 1;
            }
        }
        for p in cs.windows(2) {
            pairs += 1;
            match classify(layout, p[0], p[1]) {
                Some(Transition::SameFinger) => sfb += 1,
                Some(Transition::Alternate) => alt += 1,
                _ => {}
            }
        }
    }
    let pct = |a: u32, b: u32| a as f64 * 100.0 / b.max(1) as f64;
    (pct(sfb, pairs), pct(alt, pairs), pct(home, letters))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout;

    #[test]
    fn classifies_transitions() {
        let q = layout::builtin()
            .into_iter()
            .find(|l| l.id == "qwerty")
            .unwrap();
        assert_eq!(classify(&q, 'e', 'd'), Some(Transition::SameFinger));
        assert_eq!(classify(&q, 'a', 'l'), Some(Transition::Alternate));
        assert_eq!(classify(&q, 'a', 's'), Some(Transition::SameHand));
        assert_eq!(classify(&q, 'o', 'o'), Some(Transition::Repeat));
    }

    #[test]
    fn colemak_keeps_more_on_the_home_row_than_qwerty() {
        let ls = layout::builtin();
        let get = |id: &str| ls.iter().find(|l| l.id == id).unwrap();
        let words = [
            "the", "and", "that", "have", "for", "not", "with", "this", "there", "into",
        ];
        let (_, _, q_home) = layout_profile(get("qwerty"), &words);
        let (_, _, c_home) = layout_profile(get("colemak"), &words);
        assert!(c_home > q_home * 1.5, "{c_home} vs {q_home}");
    }

    #[test]
    fn finger_shares_sum_to_one() {
        let q = layout::builtin()
            .into_iter()
            .find(|l| l.id == "qwerty")
            .unwrap();
        let mut lp = LayoutProfile::default();
        for c in "the quick brown fox".chars() {
            lp.record(c, true, Some(200.0), &[]);
        }
        let total: f64 = fingers(&q, &lp).iter().map(|f| f.share).sum();
        assert!((total - 1.0).abs() < 1e-9);
    }
}
