mod analytics;
mod app;
mod config;
mod content;
mod layout;
mod session;
mod stats;
mod trainer;
mod ui;

use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use clap::{Parser, ValueEnum};

use crate::{config::Config, stats::Profile, trainer::Mode};

/// kbtrain — a psychologically grounded terminal typing trainer.
#[derive(Parser, Debug)]
#[command(version, about)]
struct Cli {
    /// Keyboard layout to train on (see --list-layouts).
    #[arg(short, long)]
    layout: Option<String>,

    /// Colour theme (see --list-themes).
    #[arg(short, long)]
    theme: Option<String>,

    /// Practise your own text file (remembered for "My text").
    #[arg(short, long, value_name = "PATH")]
    file: Option<PathBuf>,

    /// Jump straight into a mode.
    #[arg(short, long, value_enum)]
    mode: Option<CliMode>,

    /// Use a different progress file (default: in your data directory).
    #[arg(long, value_name = "PATH")]
    profile: Option<PathBuf>,

    /// Use a different config file.
    #[arg(long, value_name = "PATH")]
    config: Option<PathBuf>,

    /// Print a summary of your progress and exit.
    #[arg(long)]
    stats: bool,

    /// List available layouts and exit.
    #[arg(long)]
    list_layouts: bool,

    /// List available themes and exit.
    #[arg(long)]
    list_themes: bool,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum CliMode {
    Learn,
    Review,
    Transitions,
    Words,
    Test,
    Code,
    File,
}

impl From<CliMode> for Mode {
    fn from(m: CliMode) -> Self {
        match m {
            CliMode::Learn => Mode::Learn,
            CliMode::Review => Mode::Review,
            CliMode::Transitions => Mode::Ngrams,
            CliMode::Words => Mode::Words,
            CliMode::Test => Mode::Test,
            CliMode::Code => Mode::Code,
            CliMode::File => Mode::File,
        }
    }
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let config_path = cli.config.clone().or_else(Config::default_path);
    let mut config = Config::load(config_path.as_ref())?;

    if cli.list_layouts {
        for l in layout::builtin_defs().iter().chain(&config.layouts) {
            println!("{:<12} {}", l.id, l.name);
        }
        return Ok(());
    }
    if cli.list_themes {
        for t in ui::theme::THEMES {
            println!("{t}");
        }
        return Ok(());
    }

    let mut changed = false;
    let file_given = cli.file.is_some();
    if let Some(id) = cli.layout {
        config.layout = id;
        changed = true;
    }
    if let Some(t) = cli.theme {
        if !ui::theme::THEMES.contains(&t.as_str()) {
            bail!("unknown theme '{t}' (try --list-themes)");
        }
        config.theme = t;
        changed = true;
    }
    if let Some(f) = cli.file {
        let abs =
            std::fs::canonicalize(&f).with_context(|| format!("can't open {}", f.display()))?;
        config.text_file = Some(abs);
        changed = true;
    }

    let profile_path = cli.profile.or_else(Profile::default_path);
    let profile = match &profile_path {
        Some(p) => Profile::load(p)?,
        None => Profile::default(),
    };

    let mut app = app::App::new(config, config_path.clone(), profile, profile_path)?;
    if changed && let Some(p) = &config_path {
        app.config.save(p).ok();
    }
    if cli.stats {
        print_stats(&app);
        return Ok(());
    }

    let mut terminal = ratatui::init();
    if let Some(m) = cli.mode {
        app.start(m.into());
    } else if file_given {
        app.start(Mode::File);
    }
    let result = app.run(&mut terminal);
    ratatui::restore();
    result
}

fn print_stats(app: &app::App) {
    let p = &app.profile;
    let goal = app.config.daily_goal_secs();
    let hist: Vec<_> = p
        .history
        .iter()
        .filter(|h| h.layout == app.layout.id)
        .collect();
    let best = hist.iter().map(|h| h.wpm).fold(0.0, f64::max);
    let recent: Vec<f64> = hist.iter().rev().take(10).map(|h| h.wpm).collect();
    let avg = if recent.is_empty() {
        0.0
    } else {
        recent.iter().sum::<f64>() / recent.len() as f64
    };
    println!("kbtrain · {}", app.layout.name);
    println!("  drills        {}", p.history.len());
    println!("  practice time {}", ui::fmt_secs(p.total_secs()));
    println!(
        "  today         {:.0} / {} min",
        p.today_secs() / 60.0,
        app.config.daily_goal_minutes
    );
    println!(
        "  streak        {} days (longest {})",
        p.streak(goal),
        p.longest_streak(goal)
    );
    println!("  speed         {avg:.0} wpm average (last 10), {best:.0} best");
    println!(
        "  keys          {} mastered, {} unlocked",
        app.mastered_count(),
        app.allowed_keys().len()
    );
    println!("  review        {} items due", app.due_count());
}
