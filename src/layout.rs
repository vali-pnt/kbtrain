//! Keyboard layouts: physical key grid, characters (with shift layer), and
//! touch-typing finger assignment.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Finger {
    LPinky,
    LRing,
    LMiddle,
    LIndex,
    RIndex,
    RMiddle,
    RRing,
    RPinky,
    Thumb,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hand {
    Left,
    Right,
    Thumb,
}

impl Finger {
    pub const ALL: [Finger; 9] = [
        Finger::LPinky,
        Finger::LRing,
        Finger::LMiddle,
        Finger::LIndex,
        Finger::RIndex,
        Finger::RMiddle,
        Finger::RRing,
        Finger::RPinky,
        Finger::Thumb,
    ];

    pub fn index(self) -> usize {
        self as usize
    }

    pub fn hand(self) -> Hand {
        match self {
            Finger::LPinky | Finger::LRing | Finger::LMiddle | Finger::LIndex => Hand::Left,
            Finger::Thumb => Hand::Thumb,
            _ => Hand::Right,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Finger::LPinky => "left pinky",
            Finger::LRing => "left ring",
            Finger::LMiddle => "left middle",
            Finger::LIndex => "left index",
            Finger::RIndex => "right index",
            Finger::RMiddle => "right middle",
            Finger::RRing => "right ring",
            Finger::RPinky => "right pinky",
            Finger::Thumb => "thumb",
        }
    }
}

#[derive(Debug, Clone)]
pub struct Key {
    pub ch: char,
    pub shift: char,
    /// 0 = number row, 1 = top, 2 = home, 3 = bottom.
    pub row: usize,
    pub col: usize,
    pub finger: Finger,
    pub home: bool,
}

#[derive(Debug, Clone)]
pub struct Layout {
    pub id: String,
    pub name: String,
    pub rows: Vec<Vec<Key>>,
}

pub const NUMBER_ROW: usize = 0;
pub const HOME_ROW: usize = 2;
/// Home key columns on the home row.
const HOME_COLS: [usize; 8] = [0, 1, 2, 3, 6, 7, 8, 9];
/// English letter frequency order, used to pick the next key to unlock.
const FREQ_ORDER: &str = "etaoinshrdlcumwfgypbvkjxqz";

fn finger_for(row: usize, col: usize) -> Finger {
    use Finger::*;
    if row == NUMBER_ROW {
        // The number row is offset by one column (backtick/tilde sits left of 1).
        return match col {
            0 | 1 => LPinky,
            2 => LRing,
            3 => LMiddle,
            4 | 5 => LIndex,
            6 | 7 => RIndex,
            8 => RMiddle,
            9 => RRing,
            _ => RPinky,
        };
    }
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

/// A layout definition as written in the config file.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LayoutDef {
    pub id: String,
    pub name: String,
    /// Four rows: number, top, home, bottom.
    pub rows: Vec<String>,
    /// Shifted characters for each row; defaults to uppercase of `rows`.
    #[serde(default)]
    pub shift_rows: Vec<String>,
}

impl LayoutDef {
    pub fn build(&self) -> Result<Layout, String> {
        if self.rows.len() != 4 {
            return Err(format!("layout '{}' needs exactly 4 rows", self.id));
        }
        let rows = self
            .rows
            .iter()
            .enumerate()
            .map(|(r, s)| {
                let shifted: Vec<char> = self
                    .shift_rows
                    .get(r)
                    .map(|s| s.chars().collect())
                    .unwrap_or_default();
                s.chars()
                    .enumerate()
                    .map(|(c, ch)| Key {
                        ch,
                        shift: shifted.get(c).copied().unwrap_or_else(|| upper(ch)),
                        row: r,
                        col: c,
                        finger: finger_for(r, c),
                        home: r == HOME_ROW && HOME_COLS.contains(&c),
                    })
                    .collect()
            })
            .collect();
        Ok(Layout {
            id: self.id.clone(),
            name: self.name.clone(),
            rows,
        })
    }
}

fn upper(ch: char) -> char {
    let mut up = ch.to_uppercase();
    match (up.next(), up.next()) {
        (Some(u), None) => u,
        _ => ch,
    }
}

fn def(id: &str, name: &str, rows: [&str; 4], shift: [&str; 4]) -> LayoutDef {
    LayoutDef {
        id: id.into(),
        name: name.into(),
        rows: rows.iter().map(|s| s.to_string()).collect(),
        shift_rows: shift.iter().map(|s| s.to_string()).collect(),
    }
}

const US_NUM: &str = "`1234567890-=";
const US_NUM_SHIFT: &str = "~!@#$%^&*()_+";

pub fn builtin_defs() -> Vec<LayoutDef> {
    vec![
        def(
            "qwerty",
            "QWERTY",
            [US_NUM, "qwertyuiop[]\\", "asdfghjkl;'", "zxcvbnm,./"],
            [US_NUM_SHIFT, "QWERTYUIOP{}|", "ASDFGHJKL:\"", "ZXCVBNM<>?"],
        ),
        def(
            "colemak",
            "Colemak",
            [US_NUM, "qwfpgjluy;[]\\", "arstdhneio'", "zxcvbkm,./"],
            [US_NUM_SHIFT, "QWFPGJLUY:{}|", "ARSTDHNEIO\"", "ZXCVBKM<>?"],
        ),
        def(
            "colemak-dh",
            "Colemak-DH",
            [US_NUM, "qwfpbjluy;[]\\", "arstgmneio'", "zxcdvkh,./"],
            [US_NUM_SHIFT, "QWFPBJLUY:{}|", "ARSTGMNEIO\"", "ZXCDVKH<>?"],
        ),
        def(
            "dvorak",
            "Dvorak",
            [
                "`1234567890[]",
                "',.pyfgcrl/=\\",
                "aoeuidhtns-",
                ";qjkxbmwvz",
            ],
            [
                "~!@#$%^&*(){}",
                "\"<>PYFGCRL?+|",
                "AOEUIDHTNS_",
                ":QJKXBMWVZ",
            ],
        ),
        def(
            "workman",
            "Workman",
            [US_NUM, "qdrwbjfup;[]\\", "ashtgyneoi'", "zxmcvkl,./"],
            [US_NUM_SHIFT, "QDRWBJFUP:{}|", "ASHTGYNEOI\"", "ZXMCVKL<>?"],
        ),
        def(
            "qwertz",
            "QWERTZ (German)",
            [
                "^1234567890ß´",
                "qwertzuiopü+#",
                "asdfghjklöä",
                "yxcvbnm,.-",
            ],
            [
                "°!\"§$%&/()=?`",
                "QWERTZUIOPÜ*'",
                "ASDFGHJKLÖÄ",
                "YXCVBNM;:_",
            ],
        ),
        def(
            "azerty",
            "AZERTY (French)",
            [
                "²&é\"'(-è_çà)=",
                "azertyuiop^$",
                "qsdfghjklmù*",
                "wxcvbn,;:!",
            ],
            [
                "²1234567890°+",
                "AZERTYUIOP¨£",
                "QSDFGHJKLM%µ",
                "WXCVBN?./§",
            ],
        ),
    ]
}

pub fn builtin() -> Vec<Layout> {
    builtin_defs()
        .iter()
        .filter_map(|d| d.build().ok())
        .collect()
}

impl Layout {
    pub fn keys(&self) -> impl Iterator<Item = &Key> {
        self.rows.iter().flatten()
    }

    /// Finds the key producing `ch`, and whether shift is needed.
    pub fn locate(&self, ch: char) -> Option<(&Key, bool)> {
        self.keys()
            .find(|k| k.ch == ch)
            .map(|k| (k, false))
            .or_else(|| self.keys().find(|k| k.shift == ch).map(|k| (k, true)))
    }

    /// Finger used for a character (space → thumb, newline → right pinky).
    pub fn finger_of(&self, ch: char) -> Option<Finger> {
        match ch {
            ' ' => Some(Finger::Thumb),
            '\n' => Some(Finger::RPinky),
            _ => self.locate(ch).map(|(k, _)| k.finger),
        }
    }

    /// Lowercase ASCII letters on the home row.
    pub fn home_letters(&self) -> Vec<char> {
        self.rows[HOME_ROW]
            .iter()
            .map(|k| k.ch)
            .filter(char::is_ascii_lowercase)
            .collect()
    }

    /// Letters ordered for progressive unlocking: home-position keys from the
    /// strongest finger outwards, the rest of the home row, then everything
    /// else by English letter frequency (so useful words appear early).
    pub fn learning_order(&self) -> Vec<char> {
        let finger_rank = |f: Finger| match f {
            Finger::LIndex | Finger::RIndex => 0,
            Finger::LMiddle | Finger::RMiddle => 1,
            Finger::LRing | Finger::RRing => 2,
            _ => 3,
        };
        let mut home: Vec<&Key> = self.rows[HOME_ROW]
            .iter()
            .filter(|k| k.ch.is_ascii_lowercase())
            .collect();
        home.sort_by_key(|k| (!k.home, finger_rank(k.finger), k.col));
        let mut order: Vec<char> = home.iter().map(|k| k.ch).collect();

        let freq = |c: char| FREQ_ORDER.find(c).unwrap_or(usize::MAX);
        let mut rest: Vec<char> = self
            .rows
            .iter()
            .skip(1)
            .flatten()
            .map(|k| k.ch)
            .filter(|c| c.is_ascii_lowercase() && !order.contains(c))
            .collect();
        rest.sort_by_key(|&c| freq(c));
        order.extend(rest);
        order
    }

    /// How many keys are unlocked on day one.
    pub fn initial_unlocked(&self) -> usize {
        self.home_letters()
            .len()
            .clamp(6, self.learning_order().len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_builtin_layouts_cover_the_alphabet() {
        for l in builtin() {
            let order = l.learning_order();
            assert_eq!(order.len(), 26, "{} has {} letters", l.id, order.len());
            for c in 'a'..='z' {
                assert!(l.locate(c).is_some(), "{} missing {c}", l.id);
                assert!(
                    l.locate(c.to_ascii_uppercase()).is_some(),
                    "{} missing {}",
                    l.id,
                    c.to_ascii_uppercase()
                );
            }
        }
    }

    #[test]
    fn qwerty_fingers_and_order() {
        let l = Layout::builtin_by_id("qwerty");
        assert_eq!(l.finger_of('f'), Some(Finger::LIndex));
        assert_eq!(l.finger_of('g'), Some(Finger::LIndex));
        assert_eq!(l.finger_of('h'), Some(Finger::RIndex));
        assert_eq!(l.finger_of('5'), Some(Finger::LIndex));
        assert_eq!(l.finger_of('6'), Some(Finger::RIndex));
        assert_eq!(l.finger_of('0'), Some(Finger::RPinky));
        assert_eq!(l.finger_of('P'), Some(Finger::RPinky));
        assert_eq!(l.finger_of(' '), Some(Finger::Thumb));
        assert_eq!(l.locate('?').map(|(k, s)| (k.ch, s)), Some(('/', true)));
        let order = l.learning_order();
        assert_eq!(&order[..2], &['f', 'j']);
        assert_eq!(order[9], 'e');
        assert_eq!(l.initial_unlocked(), 9);
    }

    #[test]
    fn custom_layout_defaults_shift_to_uppercase() {
        let d = LayoutDef {
            id: "x".into(),
            name: "X".into(),
            rows: vec![
                US_NUM.into(),
                "qwertyuiop".into(),
                "asdfghjkl;".into(),
                "zxcvbnm,./".into(),
            ],
            shift_rows: vec![],
        };
        let l = d.build().unwrap();
        assert_eq!(l.locate('Q').map(|(k, s)| (k.ch, s)), Some(('q', true)));
    }

    impl Layout {
        pub fn builtin_by_id(id: &str) -> Layout {
            builtin().into_iter().find(|l| l.id == id).unwrap()
        }
    }
}
