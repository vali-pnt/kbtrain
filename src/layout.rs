//! Keyboard layouts: physical key grid, characters, and finger assignment.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Finger {
    LPinky,
    LRing,
    LMiddle,
    LIndex,
    RIndex,
    RMiddle,
    RRing,
    RPinky,
}

#[derive(Debug, Clone)]
pub struct Key {
    pub ch: char,
    pub row: usize,
    pub col: usize,
    pub finger: Finger,
    pub home: bool,
}

#[derive(Debug, Clone)]
pub struct Layout {
    pub id: &'static str,
    pub name: &'static str,
    pub rows: Vec<Vec<Key>>,
}

/// Row-staggered ANSI geometry: 3 letter rows of 10 keys (+ punctuation).
const ROW_LEN: [usize; 3] = [10, 11, 10];
/// Home key columns on the middle row.
const HOME_COLS: [usize; 8] = [0, 1, 2, 3, 6, 7, 8, 9];

fn finger_for(col: usize) -> Finger {
    use Finger::*;
    match col {
        0 => LPinky,
        1 => LRing,
        2 => LMiddle,
        3 | 4 => LIndex,
        5 | 6 => RIndex,
        7 => RMiddle,
        8 => RRing,
        _ => RPinky,
    }
}

fn build(id: &'static str, name: &'static str, rows: [&str; 3]) -> Layout {
    let rows = rows
        .iter()
        .enumerate()
        .map(|(r, s)| {
            s.chars()
                .take(ROW_LEN[r])
                .enumerate()
                .map(|(c, ch)| Key {
                    ch,
                    row: r,
                    col: c,
                    finger: finger_for(c),
                    home: r == 1 && HOME_COLS.contains(&c),
                })
                .collect()
        })
        .collect();
    Layout { id, name, rows }
}

pub fn builtin() -> Vec<Layout> {
    vec![
        build("qwerty", "QWERTY", ["qwertyuiop", "asdfghjkl;'", "zxcvbnm,./"]),
        build("colemak", "Colemak", ["qwfpgjluy;", "arstdhneio'", "zxcvbkm,./"]),
        build("colemak-dh", "Colemak-DH", ["qwfpbjluy;", "arstgmneio'", "zxcdvkh,./"]),
        build("dvorak", "Dvorak", ["',.pyfgcrl", "aoeuidhtns-", ";qjkxbmwvz"]),
    ]
}

impl Layout {
    pub fn by_id(id: &str) -> Option<Layout> {
        builtin().into_iter().find(|l| l.id == id)
    }

    pub fn key(&self, ch: char) -> Option<&Key> {
        let ch = ch.to_ascii_lowercase();
        self.rows.iter().flatten().find(|k| k.ch == ch)
    }

    /// Letters ordered from easiest (home row, strong fingers) to hardest.
    /// Used for progressive key unlocking.
    pub fn learning_order(&self) -> Vec<char> {
        let mut keys: Vec<&Key> = self
            .rows
            .iter()
            .flatten()
            .filter(|k| k.ch.is_ascii_alphabetic())
            .collect();
        keys.sort_by_key(|k| (k.row.abs_diff(1), (k.col as i32 - 4).abs().min((k.col as i32 - 5).abs())));
        keys.into_iter().map(|k| k.ch).collect()
    }
}
