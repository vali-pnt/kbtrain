use std::{fs, path::PathBuf};

use anyhow::{Context, Result};
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};

use crate::{layout::LayoutDef, session::ErrorMode};

/// How the on-screen keyboard is shown while typing.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum KeyboardMode {
    /// Large finger-zoned keys with heatmap and press ripple.
    #[default]
    Large,
    /// Hidden while typing fluently; fades in on hesitation or errors.
    Adaptive,
    /// One-line-per-row heatmap strip.
    Compact,
    Hidden,
}

impl KeyboardMode {
    pub const ALL: [KeyboardMode; 4] = [
        KeyboardMode::Large,
        KeyboardMode::Adaptive,
        KeyboardMode::Compact,
        KeyboardMode::Hidden,
    ];

    pub fn label(self) -> &'static str {
        match self {
            KeyboardMode::Large => "large",
            KeyboardMode::Adaptive => "adaptive (fades)",
            KeyboardMode::Compact => "compact heatmap",
            KeyboardMode::Hidden => "hidden",
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Animations {
    #[default]
    Full,
    Reduced,
    Off,
}

impl Animations {
    pub const ALL: [Animations; 3] = [Animations::Full, Animations::Reduced, Animations::Off];

    pub fn label(self) -> &'static str {
        match self {
            Animations::Full => "full",
            Animations::Reduced => "reduced",
            Animations::Off => "off",
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ColorDepth {
    #[default]
    Auto,
    Truecolor,
    Ansi256,
}

impl ColorDepth {
    pub const ALL: [ColorDepth; 3] = [ColorDepth::Auto, ColorDepth::Truecolor, ColorDepth::Ansi256];

    pub fn label(self) -> &'static str {
        match self {
            ColorDepth::Auto => "auto",
            ColorDepth::Truecolor => "truecolor",
            ColorDepth::Ansi256 => "256 colors",
        }
    }

    /// Whether to emit 24-bit colour.
    pub fn truecolor(self) -> bool {
        match self {
            ColorDepth::Truecolor => true,
            ColorDepth::Ansi256 => false,
            ColorDepth::Auto => {
                let ct = std::env::var("COLORTERM").unwrap_or_default();
                let apple = std::env::var("TERM_PROGRAM").is_ok_and(|p| p == "Apple_Terminal");
                (ct.contains("truecolor") || ct.contains("24bit")) && !apple
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub layout: String,
    pub theme: String,
    pub keyboard: KeyboardMode,
    pub error_mode: ErrorMode,
    /// Words per drill.
    pub drill_length: usize,
    /// Speed at which a key counts as mastered (and the next unlocks).
    pub target_wpm: u32,
    pub daily_goal_minutes: u32,
    pub test_seconds: u32,
    /// Start on the home row and unlock keys as they're mastered.
    pub progressive_unlock: bool,
    pub punctuation: bool,
    pub numbers: bool,
    pub capitals: bool,
    pub code_language: String,
    /// Skip indentation automatically after a newline in code.
    pub auto_indent: bool,
    pub animations: Animations,
    pub color: ColorDepth,
    pub bell_on_error: bool,
    pub live_stats: bool,
    /// Text file for "My text" practice.
    pub text_file: Option<PathBuf>,
    /// User-defined keyboard layouts.
    pub layouts: Vec<LayoutDef>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            layout: "qwerty".into(),
            theme: "synthwave".into(),
            keyboard: KeyboardMode::Large,
            error_mode: ErrorMode::Continue,
            drill_length: 25,
            target_wpm: 35,
            daily_goal_minutes: 10,
            test_seconds: 30,
            progressive_unlock: true,
            punctuation: false,
            numbers: false,
            capitals: false,
            code_language: "any".into(),
            auto_indent: true,
            animations: Animations::Full,
            color: ColorDepth::Auto,
            bell_on_error: false,
            live_stats: true,
            text_file: None,
            layouts: Vec::new(),
        }
    }
}

pub const TEST_SECONDS: [u32; 4] = [15, 30, 60, 120];

impl Config {
    pub fn default_path() -> Option<PathBuf> {
        ProjectDirs::from("", "", "kbtrain").map(|d| d.config_dir().join("config.toml"))
    }

    pub fn load(path: Option<&PathBuf>) -> Result<Self> {
        match path {
            Some(p) if p.exists() => {
                let text =
                    fs::read_to_string(p).with_context(|| format!("reading {}", p.display()))?;
                toml::from_str(&text).with_context(|| format!("parsing {}", p.display()))
            }
            _ => Ok(Self::default()),
        }
    }

    pub fn save(&self, path: &PathBuf) -> Result<()> {
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)?;
        }
        fs::write(path, toml::to_string_pretty(self)?)?;
        Ok(())
    }

    pub fn daily_goal_secs(&self) -> f64 {
        self.daily_goal_minutes as f64 * 60.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrips_and_tolerates_partial_files() {
        let c = Config {
            keyboard: KeyboardMode::Adaptive,
            error_mode: ErrorMode::Backspace,
            ..Config::default()
        };
        let back: Config = toml::from_str(&toml::to_string_pretty(&c).unwrap()).unwrap();
        assert_eq!(c, back);
        let partial: Config = toml::from_str("layout = \"dvorak\"\nerror_mode = \"stop\"").unwrap();
        assert_eq!(partial.layout, "dvorak");
        assert_eq!(partial.error_mode, ErrorMode::Stop);
        assert_eq!(partial.target_wpm, 35);
    }

    #[test]
    fn readme_example_config_is_valid() {
        let readme = include_str!("../README.md");
        let start = readme.find("```toml\n").expect("README has a toml example") + 8;
        let end = start + readme[start..].find("```").unwrap();
        let c: Config = toml::from_str(&readme[start..end]).unwrap();
        assert_eq!(c.keyboard, KeyboardMode::Adaptive);
        assert_eq!(c.color, ColorDepth::Auto);
        assert_eq!(c.layouts.len(), 1);
        let l = c.layouts[0].build().unwrap();
        assert_eq!(l.learning_order().len(), 26);
        let all: Config = toml::from_str(
            "color = \"ansi256\"\nerror_mode = \"backspace\"\nanimations = \"reduced\"",
        )
        .unwrap();
        assert_eq!(all.color, ColorDepth::Ansi256);
    }
}
