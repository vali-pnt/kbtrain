mod app;
mod config;
mod layout;
mod session;
mod stats;
mod trainer;
mod ui;

use anyhow::Result;
use clap::Parser;

/// kbtrain — a psychologically grounded terminal typing trainer.
#[derive(Parser, Debug)]
#[command(version, about)]
struct Cli {
    /// Keyboard layout to train on (qwerty, colemak, colemak-dh, dvorak).
    #[arg(short, long)]
    layout: Option<String>,

    /// List available layouts and exit.
    #[arg(long)]
    list_layouts: bool,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    if cli.list_layouts {
        for l in layout::builtin() {
            println!("{:<12} {}", l.id, l.name);
        }
        return Ok(());
    }

    let mut config = config::Config::load()?;
    if let Some(id) = cli.layout {
        config.layout = id;
    }

    let mut app = app::App::new(config)?;
    let mut terminal = ratatui::init();
    let result = app.run(&mut terminal);
    ratatui::restore();
    result
}
