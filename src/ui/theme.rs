use ratatui::style::{Color, Style};

/// Colour palette. Placeholder until the visual direction is decided.
pub struct Theme {
    pub base: Style,
    pub title: Style,
    pub accent: Style,
    pub dim: Style,
    pub pending: Style,
    pub correct: Style,
    pub wrong: Style,
    pub cursor: Style,
    pub key: Style,
    pub key_home: Style,
    pub key_next: Style,
    /// Heatmap endpoints: strong → weak.
    pub heat_cool: (u8, u8, u8),
    pub heat_hot: (u8, u8, u8),
}

impl Default for Theme {
    fn default() -> Self {
        let bg = Color::Rgb(18, 18, 28);
        Self {
            base: Style::new().bg(bg).fg(Color::Rgb(200, 200, 220)),
            title: Style::new().bg(Color::Rgb(140, 110, 255)).fg(bg),
            accent: Style::new().fg(Color::Rgb(140, 110, 255)),
            dim: Style::new().fg(Color::Rgb(100, 100, 130)),
            pending: Style::new().fg(Color::Rgb(90, 90, 120)),
            correct: Style::new().fg(Color::Rgb(220, 220, 240)),
            wrong: Style::new().fg(Color::Rgb(255, 90, 110)),
            cursor: Style::new().bg(Color::Rgb(140, 110, 255)).fg(bg),
            key: Style::new().fg(Color::Rgb(150, 150, 180)),
            key_home: Style::new().fg(Color::Rgb(200, 200, 230)),
            key_next: Style::new().bg(Color::Rgb(140, 110, 255)).fg(bg),
            heat_cool: (60, 200, 170),
            heat_hot: (255, 90, 110),
        }
    }
}

impl Theme {
    /// Interpolates the heatmap colour for a weakness score in [0, 1].
    pub fn heat(&self, t: f64) -> Color {
        let t = t.clamp(0.0, 1.0);
        let lerp = |a: u8, b: u8| (a as f64 + (b as f64 - a as f64) * t) as u8;
        let (c, h) = (self.heat_cool, self.heat_hot);
        Color::Rgb(lerp(c.0, h.0), lerp(c.1, h.1), lerp(c.2, h.2))
    }
}
