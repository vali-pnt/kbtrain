//! Adaptive drill generation.
//!
//! Principles:
//! - Progressive unlocking: start with home-row keys, add one key at a time
//!   once every unlocked key is fluent (mastery learning).
//! - Targeted practice: weight word choice toward weak and overdue keys.
//! - Spaced repetition: weak keys, transitions and words resurface when
//!   their forgetting curve says they're about to be forgotten.
//! - When real words run short (few keys unlocked), generate pronounceable
//!   pseudowords from a letter-trigram model of English.

use std::collections::{BTreeMap, HashMap};

use rand::{Rng, RngExt, seq::IndexedRandom};

use crate::{
    content::{self, SNIPPETS},
    layout::Layout,
    session::Session,
    stats::{LayoutProfile, Stat, WordStat},
};

const WORDS: &str = include_str!("../assets/words_en.txt");
/// Index used for word boundaries in the letter model.
const EDGE: usize = 26;
/// Additive smoothing so unseen letter sequences stay possible but rare.
const SMOOTH: f64 = 0.02;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Learn,
    Review,
    Ngrams,
    Words,
    Test,
    Code,
    File,
}

impl Mode {
    pub fn label(self) -> &'static str {
        match self {
            Mode::Learn => "Learn",
            Mode::Review => "Review",
            Mode::Ngrams => "Transitions",
            Mode::Words => "Words",
            Mode::Test => "Timed test",
            Mode::Code => "Code",
            Mode::File => "My text",
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            Mode::Learn => "Adaptive lessons. Keys unlock one at a time as you master them.",
            Mode::Review => {
                "Spaced repetition of the keys, transitions and words you're about to forget."
            }
            Mode::Ngrams => "Drill your slowest letter pairs and triples until they flow.",
            Mode::Words => {
                "Common words on every key. Punctuation, numbers and capitals in settings."
            }
            Mode::Test => "A classic timed speed test.",
            Mode::Code => "Real code with brackets, symbols and indentation.",
            Mode::File => "Type through your own text, picking up where you left off.",
        }
    }

    /// Stable id used in session history (and personal bests).
    pub fn id(self, test_secs: u32) -> String {
        match self {
            Mode::Learn => "learn".into(),
            Mode::Review => "review".into(),
            Mode::Ngrams => "ngrams".into(),
            Mode::Words => "words".into(),
            Mode::Test => format!("test-{test_secs}"),
            Mode::Code => "code".into(),
            Mode::File => "file".into(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Drill {
    pub mode: Mode,
    pub text: String,
    /// Shown in the header: a lesson focus, code language, file name…
    pub title: String,
    pub focus: Option<char>,
    pub time_limit: Option<f64>,
    pub code: bool,
    /// Char range within the user's file, to advance their reading position.
    pub file_span: Option<(usize, usize)>,
}

impl Drill {
    fn new(mode: Mode, text: String, title: impl Into<String>) -> Self {
        Self {
            mode,
            text,
            title: title.into(),
            focus: None,
            time_limit: None,
            code: false,
            file_span: None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Options {
    pub target_ms: f64,
    pub words: usize,
    pub progressive: bool,
    pub punctuation: bool,
    pub numbers: bool,
    pub capitals: bool,
    pub test_secs: u32,
    pub code_lang: String,
}

/// Letter-trigram counts: (prev2, prev1) → counts of next letter (or EDGE).
struct Markov {
    counts: HashMap<(usize, usize), [u32; 27]>,
}

fn idx(c: char) -> Option<usize> {
    c.is_ascii_lowercase().then(|| (c as u8 - b'a') as usize)
}

impl Markov {
    fn train(words: &[&str]) -> Self {
        let mut counts: HashMap<(usize, usize), [u32; 27]> = HashMap::new();
        for w in words {
            let mut ctx = (EDGE, EDGE);
            for i in w.chars().filter_map(idx).chain([EDGE]) {
                counts.entry(ctx).or_insert([0; 27])[i] += 1;
                // Bigram backoff, keyed with EDGE + 1 as the "unknown" prev2.
                counts.entry((EDGE + 1, ctx.1)).or_insert([0; 27])[i] += 1;
                ctx = (ctx.1, i);
            }
        }
        Self { counts }
    }

    fn weight(&self, ctx: (usize, usize), next: usize) -> f64 {
        let tri = self.counts.get(&ctx).map_or(0, |c| c[next]) as f64;
        let bi = self.counts.get(&(EDGE + 1, ctx.1)).map_or(0, |c| c[next]) as f64;
        tri + 0.2 * bi
    }

    /// Probability of `next` given `ctx`, over the allowed letters plus word end.
    fn prob(&self, ctx: (usize, usize), next: usize, letters: &[usize]) -> f64 {
        let total: f64 = letters
            .iter()
            .chain([&EDGE])
            .map(|&l| self.weight(ctx, l) + SMOOTH)
            .sum();
        (self.weight(ctx, next) + SMOOTH) / total
    }

    /// Mean log-probability per transition: how English-like a word looks.
    fn plausibility(&self, w: &[usize], letters: &[usize]) -> f64 {
        let mut ctx = (EDGE, EDGE);
        let mut lp = 0.0;
        for &i in w.iter().chain([&EDGE]) {
            lp += self.prob(ctx, i, letters).ln();
            ctx = (ctx.1, i);
        }
        lp / (w.len() + 1) as f64
    }

    fn sample<R: Rng + ?Sized>(
        &self,
        letters: &[usize],
        focus: Option<usize>,
        rng: &mut R,
    ) -> Vec<usize> {
        let target_len = rng.random_range(3..=7usize);
        let mut w: Vec<usize> = Vec::new();
        let mut ctx = (EDGE, EDGE);
        loop {
            let mut options: Vec<(usize, f64)> = letters
                .iter()
                .map(|&l| {
                    let mut wt = self.weight(ctx, l) + SMOOTH;
                    if Some(l) == focus && !w.contains(&l) {
                        wt *= 3.0;
                    }
                    // No doubled letter at the start, never three in a row.
                    let n = w.len();
                    if (n == 1 && w[0] == l) || (n >= 2 && w[n - 1] == l && w[n - 2] == l) {
                        wt = 0.0;
                    }
                    (l, wt)
                })
                .collect();
            if w.len() >= 2 {
                let end = self.weight(ctx, EDGE) + SMOOTH;
                let push = if w.len() >= target_len { 20.0 } else { 0.3 };
                options.push((EDGE, end * push));
            }
            let Ok(&(next, _)) = options.choose_weighted(rng, |o| o.1) else {
                break;
            };
            if next == EDGE || w.len() >= 9 {
                break;
            }
            w.push(next);
            ctx = (ctx.1, next);
        }
        w
    }

    /// Generates a pronounceable word from `allowed` letters, favouring
    /// `focus`: samples several candidates and keeps the most English-like
    /// one that contains the focus letter (and a vowel, when one is allowed).
    fn word<R: Rng + ?Sized>(&self, allowed: &[char], focus: Option<char>, rng: &mut R) -> String {
        let letters: Vec<usize> = allowed.iter().filter_map(|&c| idx(c)).collect();
        let focus = focus.and_then(idx).filter(|f| letters.contains(f));
        let vowels: Vec<usize> = "aeiouy"
            .chars()
            .filter_map(idx)
            .filter(|v| letters.contains(v))
            .collect();
        let ok = |w: &[usize], strict: bool| {
            w.len() >= if strict { 3 } else { 2 }
                && (vowels.is_empty() || w.iter().any(|c| vowels.contains(c)))
                && focus.is_none_or(|f| w.contains(&f) || !strict)
        };
        let mut best: Option<(f64, Vec<usize>)> = None;
        for attempt in 0..60 {
            let w = self.sample(&letters, focus, rng);
            let strict = attempt < 40;
            if !ok(&w, strict) {
                continue;
            }
            let score = self.plausibility(&w, &letters);
            if best.as_ref().is_none_or(|(b, _)| score > *b) {
                best = Some((score, w));
            }
            if attempt >= 16 && best.is_some() {
                break;
            }
        }
        let w = best
            .map(|(_, w)| w)
            .unwrap_or_else(|| letters.iter().take(3).copied().collect());
        w.iter().map(|&i| (b'a' + i as u8) as char).collect()
    }
}

pub struct Trainer {
    words: Vec<&'static str>,
    model: Markov,
}

impl Default for Trainer {
    fn default() -> Self {
        Self::new()
    }
}

impl Trainer {
    pub fn new() -> Self {
        let mut words: Vec<&'static str> = Vec::new();
        for w in WORDS.split_whitespace() {
            if w.chars().all(|c| c.is_ascii_lowercase()) && !words.contains(&w) {
                words.push(w);
            }
        }
        let model = Markov::train(&words);
        Self { words, model }
    }

    pub fn words(&self) -> &[&'static str] {
        &self.words
    }

    pub fn generate<R: Rng + ?Sized>(
        &self,
        mode: Mode,
        layout: &Layout,
        profile: &LayoutProfile,
        opts: &Options,
        now: i64,
        rng: &mut R,
    ) -> Drill {
        match mode {
            Mode::Learn => self.learn(layout, profile, opts, now, rng),
            Mode::Review => self.review(layout, profile, opts, now, rng),
            Mode::Ngrams => self.ngrams(layout, profile, opts, rng),
            Mode::Words => Drill::new(Mode::Words, self.words_text(opts.words, opts, rng), ""),
            Mode::Test => {
                let mut d = Drill::new(
                    Mode::Test,
                    self.words_text(150, opts, rng),
                    format!("{}s", opts.test_secs),
                );
                d.time_limit = Some(opts.test_secs as f64);
                d
            }
            Mode::Code => self.code(&opts.code_lang, rng),
            // File drills need the file text; see `file_drill`.
            Mode::File => Drill::new(Mode::File, self.words_text(opts.words, opts, rng), ""),
        }
    }

    /// Letters currently available for practice.
    pub fn allowed(layout: &Layout, profile: &LayoutProfile, progressive: bool) -> Vec<char> {
        let order = layout.learning_order();
        let n = if progressive {
            profile
                .unlocked
                .clamp(layout.initial_unlocked(), order.len())
        } else {
            order.len()
        };
        order[..n].to_vec()
    }

    /// How urgently a key needs practice: weakness plus forgetting.
    fn key_priority(stat: Option<&Stat>, target_ms: f64, now: i64) -> f64 {
        match stat {
            None => 0.8,
            Some(s) => {
                let forgetting = if s.memory.reps > 0 {
                    1.0 - s.memory.retrievability(now)
                } else {
                    0.3
                };
                s.weakness(target_ms) + 0.4 * forgetting
            }
        }
    }

    /// The key a lesson should focus on: the newest key until it's
    /// mastered, otherwise whichever key most needs work.
    pub fn focus_key(
        allowed: &[char],
        profile: &LayoutProfile,
        target_ms: f64,
        now: i64,
    ) -> Option<char> {
        let newest = *allowed.last()?;
        if !profile.key(newest).is_some_and(|s| s.mastered(target_ms)) {
            return Some(newest);
        }
        allowed.iter().copied().max_by(|a, b| {
            Self::key_priority(profile.key(*a), target_ms, now).total_cmp(&Self::key_priority(
                profile.key(*b),
                target_ms,
                now,
            ))
        })
    }

    fn pick_word<'a, R: Rng + ?Sized>(
        pool: &[&'a str],
        weight: impl Fn(&str) -> f64,
        avoid: Option<&str>,
        rng: &mut R,
    ) -> Option<&'a str> {
        let w = pool
            .choose_weighted(rng, |w| if Some(*w) == avoid { 0.0 } else { weight(w) })
            .ok()
            .copied();
        w.or_else(|| pool.choose(rng).copied())
    }

    fn learn<R: Rng + ?Sized>(
        &self,
        layout: &Layout,
        profile: &LayoutProfile,
        opts: &Options,
        now: i64,
        rng: &mut R,
    ) -> Drill {
        let allowed = Self::allowed(layout, profile, opts.progressive);
        let focus = Self::focus_key(&allowed, profile, opts.target_ms, now);
        let pool: Vec<&str> = self
            .words
            .iter()
            .copied()
            .filter(|w| w.chars().all(|c| allowed.contains(&c)))
            .collect();
        // Mix in more real words as more keys unlock.
        let p_real = (pool.len() as f64 / 80.0).clamp(0.0, 0.8);
        let priority: HashMap<char, f64> = allowed
            .iter()
            .map(|&c| (c, Self::key_priority(profile.key(c), opts.target_ms, now)))
            .collect();
        let weight = |w: &str| {
            let p: f64 = w
                .chars()
                .map(|c| priority.get(&c).copied().unwrap_or(0.5))
                .sum::<f64>()
                / w.len() as f64;
            let boost = if focus.is_some_and(|f| w.contains(f)) {
                4.0
            } else {
                1.0
            };
            (0.1 + p) * boost
        };

        let mut out: Vec<String> = Vec::with_capacity(opts.words);
        while out.len() < opts.words {
            let prev = out.last().map(String::as_str);
            let word = if !pool.is_empty() && rng.random_bool(p_real) {
                Self::pick_word(&pool, weight, prev, rng).map(str::to_string)
            } else {
                None
            };
            let word = word.unwrap_or_else(|| self.model.word(&allowed, focus, rng));
            if !word.is_empty() && Some(word.as_str()) != prev {
                out.push(word);
            }
        }
        let mut d = Drill::new(Mode::Learn, out.join(" "), "");
        d.focus = focus;
        if let Some(f) = focus {
            d.title = format!("focus: {f}");
        }
        d
    }

    fn review<R: Rng + ?Sized>(
        &self,
        layout: &Layout,
        profile: &LayoutProfile,
        opts: &Options,
        now: i64,
        rng: &mut R,
    ) -> Drill {
        let allowed = Self::allowed(layout, profile, opts.progressive);
        let usable = |w: &str| w.chars().all(|c| allowed.contains(&c.to_ascii_lowercase()));
        let pool: Vec<&str> = self.words.iter().copied().filter(|w| usable(w)).collect();

        let mut due_words: Vec<(&String, &WordStat)> = profile
            .words
            .iter()
            .filter(|(w, s)| s.memory.is_due(now) && usable(w))
            .collect();
        due_words.sort_by(|a, b| {
            a.1.memory
                .retrievability(now)
                .total_cmp(&b.1.memory.retrievability(now))
        });
        let due_grams: Vec<&String> = profile
            .bigrams
            .iter()
            .filter(|(g, s)| s.memory.is_due(now) && s.weakness(opts.target_ms) > 0.35 && usable(g))
            .map(|(g, _)| g)
            .collect();
        let due_keys: Vec<char> = profile
            .keys
            .iter()
            .filter(|(k, s)| {
                allowed.contains(k) && s.memory.is_due(now) && s.weakness(opts.target_ms) > 0.35
            })
            .map(|(k, _)| *k)
            .collect();

        let mut items: Vec<String> = Vec::new();
        for (w, _) in due_words.iter().take(opts.words / 2) {
            items.push((*w).clone());
            items.push((*w).clone());
        }
        let containing = |frag: &str| {
            pool.iter()
                .filter(|w| w.contains(frag))
                .copied()
                .collect::<Vec<_>>()
        };
        for g in due_grams.iter().take(6) {
            let ws = containing(g);
            for _ in 0..2 {
                items.push(
                    ws.choose(rng)
                        .map(|w| w.to_string())
                        .unwrap_or_else(|| self.model.word(&allowed, g.chars().next(), rng)),
                );
            }
        }
        for k in due_keys.iter().take(6) {
            let ws = containing(&k.to_string());
            items.push(
                ws.choose(rng)
                    .map(|w| w.to_string())
                    .unwrap_or_else(|| self.model.word(&allowed, Some(*k), rng)),
            );
        }

        let title = if items.is_empty() {
            "nothing due: weak spots".to_string()
        } else {
            format!("{} due", due_words.len() + due_grams.len() + due_keys.len())
        };
        if items.len() < opts.words {
            // Top up with weakest-key practice.
            let rest = self.learn(
                layout,
                profile,
                &Options {
                    words: opts.words - items.len(),
                    ..opts.clone()
                },
                now,
                rng,
            );
            items.extend(rest.text.split(' ').map(String::from));
        }
        let items = spread(items, rng);
        Drill::new(Mode::Review, items.join(" "), title)
    }

    /// Weakest bigrams/trigrams with enough samples, or (with no data yet)
    /// common English pairs that are awkward on this layout.
    pub fn weak_ngrams(
        &self,
        layout: &Layout,
        profile: &LayoutProfile,
        allowed: &[char],
        target_ms: f64,
    ) -> Vec<String> {
        let ok = |g: &str| g.chars().all(|c| allowed.contains(&c));
        let rank = |m: &BTreeMap<String, Stat>, min: u32| {
            let mut v: Vec<(&String, f64)> = m
                .iter()
                .filter(|(g, s)| s.count() >= min && ok(g))
                .map(|(g, s)| (g, s.weakness(target_ms)))
                .filter(|(_, w)| *w > 0.25)
                .collect();
            v.sort_by(|a, b| b.1.total_cmp(&a.1));
            v.into_iter().map(|(g, _)| g.clone()).collect::<Vec<_>>()
        };
        let mut grams: Vec<String> = rank(&profile.bigrams, 5).into_iter().take(3).collect();
        grams.extend(rank(&profile.trigrams, 4).into_iter().take(2));
        if grams.len() >= 3 {
            return grams;
        }
        // Fallback: frequent same-finger pairs, then frequent pairs overall.
        let mut freq: HashMap<String, u32> = HashMap::new();
        for w in &self.words {
            let cs: Vec<char> = w.chars().collect();
            for p in cs.windows(2) {
                if p[0] != p[1] {
                    *freq.entry(p.iter().collect()).or_insert(0) += 1;
                }
            }
        }
        let mut pairs: Vec<(String, f64)> = freq
            .into_iter()
            .filter(|(g, _)| ok(g))
            .map(|(g, n)| {
                let cs: Vec<char> = g.chars().collect();
                let sfb = layout.finger_of(cs[0]) == layout.finger_of(cs[1]);
                let w = n as f64 * if sfb { 4.0 } else { 1.0 };
                (g, w)
            })
            .collect();
        pairs.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.cmp(&b.0)));
        for (g, _) in pairs {
            if grams.len() >= 4 {
                break;
            }
            if !grams.contains(&g) {
                grams.push(g);
            }
        }
        grams
    }

    fn ngrams<R: Rng + ?Sized>(
        &self,
        layout: &Layout,
        profile: &LayoutProfile,
        opts: &Options,
        rng: &mut R,
    ) -> Drill {
        let allowed = Self::allowed(layout, profile, opts.progressive);
        let grams = self.weak_ngrams(layout, profile, &allowed, opts.target_ms);
        let pool: Vec<&str> = self
            .words
            .iter()
            .copied()
            .filter(|w| w.chars().all(|c| allowed.contains(&c)))
            .collect();
        let per = (opts.words / grams.len().max(1)).max(4);
        let mut out: Vec<String> = Vec::new();
        for g in &grams {
            let with: Vec<&str> = pool
                .iter()
                .copied()
                .filter(|w| w.contains(g.as_str()))
                .collect();
            out.push(g.clone());
            for i in 0..per {
                let w = match with.choose(rng) {
                    Some(w) if i % 3 != 2 => w.to_string(),
                    _ => self.ngram_word(g, &allowed, rng),
                };
                out.push(w);
            }
            out.push(g.clone());
        }
        Drill::new(Mode::Ngrams, out.join(" "), grams.join(" · "))
    }

    /// A pseudoword that contains `gram`.
    fn ngram_word<R: Rng + ?Sized>(&self, gram: &str, allowed: &[char], rng: &mut R) -> String {
        for _ in 0..20 {
            let w = self.model.word(allowed, gram.chars().next(), rng);
            if w.contains(gram) {
                return w;
            }
        }
        let pre = self.model.word(allowed, None, rng);
        let cut = rng.random_range(0..=pre.len().min(2));
        format!("{}{gram}", &pre[..cut])
    }

    fn words_text<R: Rng + ?Sized>(&self, n: usize, opts: &Options, rng: &mut R) -> String {
        let mut out: Vec<String> = Vec::with_capacity(n);
        let mut cap_next = opts.capitals;
        while out.len() < n {
            let mut w = if opts.numbers && rng.random_bool(0.08) {
                rng.random_range(1..10_000u32).to_string()
            } else {
                let Some(w) =
                    Self::pick_word(&self.words, |_| 1.0, out.last().map(String::as_str), rng)
                else {
                    break;
                };
                w.to_string()
            };
            if cap_next || (opts.capitals && rng.random_bool(0.08)) {
                w = capitalize(&w);
            }
            cap_next = false;
            if opts.punctuation {
                let roll: f64 = rng.random();
                let end = match roll {
                    r if r < 0.08 => ".",
                    r if r < 0.15 => ",",
                    r if r < 0.17 => "?",
                    r if r < 0.19 => "!",
                    r if r < 0.21 => ";",
                    r if r < 0.22 => ":",
                    _ => "",
                };
                w.push_str(end);
                cap_next = opts.capitals && matches!(end, "." | "?" | "!");
                match rng.random_range(0..40) {
                    0 => w = format!("\"{w}\""),
                    1 => w = format!("({w})"),
                    2 => w = format!("{w}'s"),
                    _ => {}
                }
            }
            out.push(w);
        }
        out.join(" ")
    }

    fn code<R: Rng + ?Sized>(&self, lang: &str, rng: &mut R) -> Drill {
        let pool: Vec<_> = SNIPPETS
            .iter()
            .filter(|s| lang == "any" || s.lang == lang)
            .collect();
        let pool = if pool.is_empty() {
            SNIPPETS.iter().collect()
        } else {
            pool
        };
        let s = pool.choose(rng).expect("snippets are non-empty");
        let mut d = Drill::new(
            Mode::Code,
            s.code.to_string(),
            format!("{} snippet", content::language_name(s.lang)),
        );
        d.code = true;
        d
    }

    /// A passage from the user's file, starting at their reading position.
    pub fn file_drill(text: &str, code: bool, name: &str, offset: usize, words: usize) -> Drill {
        let approx = if code { words * 12 } else { words * 6 };
        let (s, e) = content::chunk(text, offset, approx, code);
        let passage: String = text.chars().skip(s).take(e - s).collect();
        let total = text.chars().count().max(1);
        let mut d = Drill::new(
            Mode::File,
            passage.trim().to_string(),
            format!("{name} · {}%", s * 100 / total),
        );
        d.code = code;
        d.file_span = Some((s, e));
        d
    }
}

fn capitalize(w: &str) -> String {
    let mut cs = w.chars();
    cs.next()
        .map(|f| f.to_uppercase().chain(cs).collect())
        .unwrap_or_default()
}

/// Shuffles so repeated items aren't adjacent when avoidable.
fn spread<R: Rng + ?Sized>(mut items: Vec<String>, rng: &mut R) -> Vec<String> {
    use rand::seq::SliceRandom;
    items.shuffle(rng);
    for i in 1..items.len() {
        if items[i] == items[i - 1]
            && let Some(j) = (i + 1..items.len()).find(|&j| items[j] != items[i])
        {
            items.swap(i, j);
        }
    }
    items
}

/// If every unlocked key is mastered, unlocks the next one.
pub fn try_unlock(
    layout: &Layout,
    profile: &mut LayoutProfile,
    target_ms: f64,
    now: i64,
) -> Option<char> {
    let order = layout.learning_order();
    profile.unlocked = profile
        .unlocked
        .clamp(layout.initial_unlocked(), order.len());
    if profile.unlocked >= order.len() {
        return None;
    }
    let all_mastered = order[..profile.unlocked]
        .iter()
        .all(|&c| profile.key(c).is_some_and(|s| s.mastered(target_ms)));
    if !all_mastered {
        return None;
    }
    let next = order[profile.unlocked];
    profile.unlocked += 1;
    profile.unlock_log.push((now, next));
    Some(next)
}

/// Feeds a finished drill into the spaced-repetition schedule.
pub fn apply_reviews(profile: &mut LayoutProfile, session: &Session, target_ms: f64, now: i64) {
    // Keys: quality from this drill's accuracy on the key and its current speed.
    let mut per_key: HashMap<char, (u32, u32)> = HashMap::new();
    for s in &session.strokes {
        if s.expected.is_alphanumeric() {
            let e = per_key
                .entry(s.expected.to_lowercase().next().unwrap_or(s.expected))
                .or_default();
            e.0 += 1;
            e.1 += u32::from(!s.correct());
        }
    }
    for (k, (n, miss)) in per_key {
        if n < 3 {
            continue;
        }
        if let Some(stat) = profile.keys.get_mut(&k) {
            let acc = 1.0 - miss as f64 / n as f64;
            let speed = stat.latency_ms.map_or(0.5, |l| (target_ms / l).min(1.0));
            stat.memory.review(now, acc.powi(2) * (0.4 + 0.6 * speed));
        }
    }

    // Bigrams that appeared in this drill.
    let mut seen: HashMap<String, u32> = HashMap::new();
    for p in session.target.windows(2) {
        if p[0].is_alphabetic() && p[1].is_alphabetic() {
            *seen
                .entry(p.iter().flat_map(|c| c.to_lowercase()).collect())
                .or_default() += 1;
        }
    }
    for (g, n) in seen {
        if n < 2 {
            continue;
        }
        if let Some(stat) = profile.bigrams.get_mut(&g) {
            let q = 1.0 - stat.weakness(target_ms);
            stat.memory.review(now, q);
        }
    }

    // Words: mistakes and slow words become (or stay) trouble words.
    let results = session.word_results();
    if results.is_empty() {
        return;
    }
    let avg = results.iter().map(|w| w.wpm()).sum::<f64>() / results.len() as f64;
    for w in results {
        let word = w
            .word
            .trim_matches(|c: char| !c.is_alphanumeric())
            .to_lowercase();
        if word.chars().count() < 2 {
            continue;
        }
        let slow = w.secs > 0.0 && w.wpm() < avg * 0.6;
        let trouble = w.errors > 0 || slow;
        match profile.words.get_mut(&word) {
            Some(ws) => {
                ws.attempts += 1;
                ws.misses += u32::from(trouble);
                ws.memory.review(now, if trouble { 0.2 } else { 0.9 });
            }
            None if trouble => {
                let mut ws = WordStat {
                    attempts: 1,
                    misses: 1,
                    ..Default::default()
                };
                ws.memory.review(now, 0.2);
                profile.words.insert(word, ws);
            }
            None => {}
        }
    }
    // Retire words that are well learned.
    profile.words.retain(|_, ws| ws.memory.stability < 60.0);
}

#[cfg(test)]
mod tests {
    use rand::{SeedableRng, rngs::StdRng};

    use super::*;
    use crate::{layout, session::ErrorMode, stats::wpm_to_latency};

    fn opts() -> Options {
        Options {
            target_ms: wpm_to_latency(35.0),
            words: 25,
            progressive: true,
            punctuation: false,
            numbers: false,
            capitals: false,
            test_secs: 30,
            code_lang: "any".into(),
        }
    }

    fn qwerty() -> Layout {
        layout::builtin()
            .into_iter()
            .find(|l| l.id == "qwerty")
            .unwrap()
    }

    #[test]
    fn learn_drills_only_use_unlocked_keys() {
        let t = Trainer::new();
        let mut rng = StdRng::seed_from_u64(7);
        for l in layout::builtin() {
            let p = LayoutProfile::default();
            let allowed = Trainer::allowed(&l, &p, true);
            let d = t.generate(Mode::Learn, &l, &p, &opts(), 0, &mut rng);
            assert_eq!(d.text.split(' ').count(), 25, "{}", l.id);
            assert!(
                d.text.chars().all(|c| c == ' ' || allowed.contains(&c)),
                "{}: {}",
                l.id,
                d.text
            );
            assert_eq!(d.focus, allowed.last().copied());
        }
    }

    #[test]
    fn pseudowords_contain_the_focus_key() {
        let t = Trainer::new();
        let mut rng = StdRng::seed_from_u64(1);
        let allowed: Vec<char> = "asdfghjkle".chars().collect();
        let hits = (0..50)
            .filter(|_| t.model.word(&allowed, Some('e'), &mut rng).contains('e'))
            .count();
        assert!(hits > 40, "{hits}");
    }

    #[test]
    fn unlocks_after_mastery() {
        let l = qwerty();
        let target = wpm_to_latency(35.0);
        let mut p = LayoutProfile::default();
        assert_eq!(try_unlock(&l, &mut p, target, 0), None);
        assert_eq!(p.unlocked, 9);
        for c in "asdfghjkl".chars() {
            for _ in 0..20 {
                p.record(c, true, Some(target * 0.7), &[]);
            }
        }
        assert_eq!(try_unlock(&l, &mut p, target, 5), Some('e'));
        assert_eq!(p.unlocked, 10);
        assert_eq!(p.unlock_log, vec![(5, 'e')]);
    }

    #[test]
    fn every_mode_generates_text() {
        let t = Trainer::new();
        let l = qwerty();
        let p = LayoutProfile::default();
        let mut rng = StdRng::seed_from_u64(3);
        let o = Options {
            punctuation: true,
            numbers: true,
            capitals: true,
            ..opts()
        };
        for m in [
            Mode::Learn,
            Mode::Review,
            Mode::Ngrams,
            Mode::Words,
            Mode::Test,
            Mode::Code,
        ] {
            let d = t.generate(m, &l, &p, &o, 0, &mut rng);
            assert!(!d.text.trim().is_empty(), "{m:?}");
        }
        assert_eq!(
            t.generate(Mode::Test, &l, &p, &o, 0, &mut rng).time_limit,
            Some(30.0)
        );
        assert!(t.generate(Mode::Code, &l, &p, &o, 0, &mut rng).code);
    }

    #[test]
    fn mistyped_words_become_due_for_review() {
        let l = qwerty();
        let target = wpm_to_latency(35.0);
        let mut p = LayoutProfile::default();
        let mut s = Session::new("ask dad", ErrorMode::Continue);
        for c in "asx dad".chars() {
            s.type_char(c);
        }
        apply_reviews(&mut p, &s, target, 0);
        assert!(p.words.contains_key("ask"));
        let later = 86_400;
        assert!(p.words["ask"].memory.is_due(later));
        let t = Trainer::new();
        let d = t.generate(
            Mode::Review,
            &l,
            &p,
            &opts(),
            later,
            &mut StdRng::seed_from_u64(2),
        );
        assert!(
            d.text.split(' ').filter(|w| *w == "ask").count() >= 2,
            "{}",
            d.text
        );
    }

    #[test]
    fn file_drill_reports_span() {
        let text = "One two three. Four five six.";
        let d = Trainer::file_drill(text, false, "notes.txt", 0, 2);
        assert_eq!(d.text, "One two three.");
        assert_eq!(d.file_span, Some((0, 14)));
    }
}
