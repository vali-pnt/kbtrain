//! Practice material: code snippets and the user's own text files.

use std::{fs, path::Path};

use anyhow::{Context, Result};

pub struct Snippet {
    pub lang: &'static str,
    pub code: &'static str,
}

pub const LANGUAGES: [&str; 6] = ["rust", "python", "javascript", "go", "c", "shell"];

pub const SNIPPETS: &[Snippet] = &[
    Snippet {
        lang: "rust",
        code: r#"fn mean(values: &[f64]) -> Option<f64> {
    if values.is_empty() {
        return None;
    }
    Some(values.iter().sum::<f64>() / values.len() as f64)
}"#,
    },
    Snippet {
        lang: "rust",
        code: r#"#[derive(Debug, Clone, PartialEq)]
enum Token {
    Number(f64),
    Plus,
    Minus,
}

fn lex(src: &str) -> Vec<Token> {
    src.split_whitespace()
        .filter_map(|t| match t {
            "+" => Some(Token::Plus),
            "-" => Some(Token::Minus),
            n => n.parse().ok().map(Token::Number),
        })
        .collect()
}"#,
    },
    Snippet {
        lang: "rust",
        code: r#"impl Iterator for Counter {
    type Item = u32;

    fn next(&mut self) -> Option<u32> {
        if self.count < self.limit {
            self.count += 1;
            Some(self.count)
        } else {
            None
        }
    }
}"#,
    },
    Snippet {
        lang: "python",
        code: r#"def word_counts(path):
    counts = {}
    with open(path) as f:
        for line in f:
            for word in line.lower().split():
                counts[word] = counts.get(word, 0) + 1
    return sorted(counts.items(), key=lambda kv: -kv[1])"#,
    },
    Snippet {
        lang: "python",
        code: r#"class Stack:
    def __init__(self):
        self._items = []

    def push(self, item):
        self._items.append(item)

    def pop(self):
        if not self._items:
            raise IndexError("pop from empty stack")
        return self._items.pop()"#,
    },
    Snippet {
        lang: "python",
        code: r#"def binary_search(items, target):
    lo, hi = 0, len(items) - 1
    while lo <= hi:
        mid = (lo + hi) // 2
        if items[mid] == target:
            return mid
        if items[mid] < target:
            lo = mid + 1
        else:
            hi = mid - 1
    return -1"#,
    },
    Snippet {
        lang: "javascript",
        code: r#"function debounce(fn, ms) {
    let timer = null;
    return (...args) => {
        clearTimeout(timer);
        timer = setTimeout(() => fn(...args), ms);
    };
}"#,
    },
    Snippet {
        lang: "javascript",
        code: r#"const groupBy = (items, key) =>
    items.reduce((acc, item) => {
        const k = item[key];
        (acc[k] ||= []).push(item);
        return acc;
    }, {});"#,
    },
    Snippet {
        lang: "go",
        code: r#"func reverse(s string) string {
    runes := []rune(s)
    for i, j := 0, len(runes)-1; i < j; i, j = i+1, j-1 {
        runes[i], runes[j] = runes[j], runes[i]
    }
    return string(runes)
}"#,
    },
    Snippet {
        lang: "go",
        code: r#"func worker(id int, jobs <-chan int, results chan<- int) {
    for j := range jobs {
        fmt.Printf("worker %d processing job %d\n", id, j)
        results <- j * 2
    }
}"#,
    },
    Snippet {
        lang: "c",
        code: r#"size_t str_len(const char *s) {
    const char *p = s;
    while (*p != '\0') {
        p++;
    }
    return (size_t)(p - s);
}"#,
    },
    Snippet {
        lang: "shell",
        code: r#"for f in *.log; do
    if [ -s "$f" ]; then
        gzip -9 "$f" && echo "compressed $f"
    fi
done"#,
    },
];

/// Extensions treated as code: newlines and indentation are kept.
const CODE_EXTS: [&str; 22] = [
    "rs", "py", "js", "ts", "jsx", "tsx", "go", "c", "h", "cpp", "hpp", "java", "kt", "rb", "sh", "lua", "zig",
    "hs", "toml", "json", "yaml", "yml",
];

pub fn is_code_path(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| CODE_EXTS.contains(&e.to_ascii_lowercase().as_str()))
}

/// Replaces typographic characters with their keyboard equivalents.
pub fn normalize(text: &str) -> String {
    text.chars()
        .flat_map(|c| -> Vec<char> {
            match c {
                '\u{2018}' | '\u{2019}' | '\u{201B}' | '\u{2032}' => vec!['\''],
                '\u{201C}' | '\u{201D}' | '\u{201F}' | '\u{2033}' => vec!['"'],
                '\u{2013}' | '\u{2014}' | '\u{2212}' => vec!['-'],
                '\u{2026}' => vec!['.', '.', '.'],
                '\u{00A0}' => vec![' '],
                '\t' => vec![' '; 4],
                '\r' => vec![],
                c => vec![c],
            }
        })
        .collect()
}

/// Loads a practice file. Prose is collapsed to single-spaced text; code
/// keeps its lines (trailing whitespace and blank runs removed).
pub fn load_text(path: &Path) -> Result<(String, bool)> {
    let raw = fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    let text = normalize(&raw);
    let code = is_code_path(path);
    let out = if code {
        let mut lines: Vec<&str> = Vec::new();
        for line in text.lines().map(str::trim_end) {
            if !(line.is_empty() && lines.last().is_some_and(|l| l.is_empty())) {
                lines.push(line);
            }
        }
        lines.join("\n").trim_matches('\n').to_string()
    } else {
        text.split_whitespace().collect::<Vec<_>>().join(" ")
    };
    Ok((out, code))
}

/// Picks a passage of roughly `approx` chars starting at char offset
/// `start`, ending at a sentence (prose) or line (code) boundary.
/// Returns the char range.
pub fn chunk(text: &str, start: usize, approx: usize, code: bool) -> (usize, usize) {
    let chars: Vec<char> = text.chars().collect();
    let start = if start >= chars.len() { 0 } else { start };
    let hard_end = (start + approx * 2).min(chars.len());
    let soft = (start + approx).min(chars.len());
    let is_break = |i: usize| -> bool {
        if code {
            chars[i] == '\n'
        } else {
            matches!(chars[i], '.' | '!' | '?') && chars.get(i + 1).is_none_or(|c| *c == ' ')
        }
    };
    let mut end = (soft..hard_end).find(|&i| is_break(i)).map_or(hard_end, |i| if code { i } else { i + 1 });
    if end < chars.len() && !code {
        // Never cut a word in half.
        while end > start + 1 && chars[end - 1] != ' ' && chars.get(end).is_some_and(|c| *c != ' ') {
            end -= 1;
        }
    }
    (start, end.max(start + 1).min(chars.len()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snippets_are_typeable() {
        for s in SNIPPETS {
            assert!(LANGUAGES.contains(&s.lang));
            assert!(s.code.is_ascii(), "{} snippet has non-ascii text", s.lang);
            assert!(!s.code.contains('\t'));
        }
    }

    #[test]
    fn normalizes_typography() {
        assert_eq!(normalize("\u{201C}hi\u{201D} \u{2014} it\u{2019}s\u{2026}"), "\"hi\" - it's...");
    }

    #[test]
    fn chunks_end_on_sentences() {
        let text = "One two three. Four five six. Seven eight nine.";
        let (s, e) = chunk(text, 0, 10, false);
        let piece: String = text.chars().skip(s).take(e - s).collect();
        assert_eq!(piece, "One two three.");
        let (s2, _) = chunk(text, e + 1, 10, false);
        assert_eq!(s2, e + 1);
        assert_eq!(chunk(text, 999, 10, false).0, 0, "wraps around at the end");
    }
}
