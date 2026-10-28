# kbtrain

A psychologically grounded touch-typing trainer for the terminal. It learns
where you're slow, schedules practice before you forget, and looks like a
neon sunset while doing it.

```
cargo install --path .
kbtrain
```

## What it does

**Learns you.** Every keystroke updates a model of your speed and accuracy on
each key, letter pair (bigram) and triple (trigram), weighted toward recent
performance. Pauses longer than two seconds and impossibly fast repeats are
ignored, so the numbers reflect real typing.

**Teaches progressively.** *Learn* starts you on the home row and unlocks one
new key at a time, only once every unlocked key is both fast enough (your
target speed, 35 wpm by default) and accurate. Each lesson focuses on the
newest or weakest key, mixing real words with pronounceable pseudowords
generated from English letter patterns, so there's always enough material even
with six keys.

**Remembers what you'll forget.** Keys, transitions and words you stumble on
get a forgetting-curve memory. *Review* brings them back when recall is
predicted to drop below 90%; successful reviews push the next one further out,
and late successes push it furthest (the spacing effect).

**Drills transitions.** *Transitions* targets your slowest bigrams and
trigrams. With no data yet, it starts with common pairs that are awkward on
your layout, such as same-finger bigrams.

**Keeps you going.** You get personal-best tracking per mode with sparklines,
a flow combo meter, a daily goal, and a streak calendar. There are no
leaderboards: you only compete with yourself.

**Reacts to how you type.** Letters glow white-hot as you hit them and cool
into place, a clean word gets a sweep of light, and the room warms as your
combo climbs through *flow*, *surge*, *overdrive* and *transcendent*. A
mistake does the opposite: the letter glitches, the line shakes its head, red
bleeds in from the screen edges, and a broken combo crumbles and falls. A run
of misses earns a "slow down". On the results screen your speed gathers out
of scattered dust; a personal best lights an aurora, while a run under 90%
accuracy drains the colour from everything except your mistakes and leaves
the number sputtering like failing neon.

## Modes

| Mode | What it's for |
|---|---|
| Learn | Adaptive lessons with progressive key unlocking |
| Review | Spaced repetition of due keys, transitions and trouble words |
| Transitions | Your slowest letter pairs and triples |
| Words | Common words on every key; punctuation, numbers, capitals optional |
| Timed test | 15 / 30 / 60 / 120 second speed test (←/→ on the menu) |
| Code | Real snippets in Rust, Python, JavaScript, Go, C and shell (←/→ picks) |
| My text | Your own file, resuming where you stopped |

Practise your own text with `kbtrain --file notes.md`. Prose is reflowed;
source files (`.rs`, `.py`, `.js`, …) keep their lines and indentation.
Typographic quotes and dashes are converted to keyboard characters.

## The screen

- **On-screen keyboard** in four styles, cycled with `Ctrl+K`:
  - *large:* solid keycaps tinted by finger zone, labels showing your weak
    spots, the next key lit, and presses that sink the cap and send a ripple
    of light across the key rims.
  - *adaptive:* fades away while you type fluently and returns when you
    hesitate or make a mistake.
  - *compact:* a one-line-per-row heat strip.
  - *hidden.*
- **Unlock bar** showing every letter's confidence, with the lesson's focus key.
- **Errors** are shown in red and corrected ones in amber. Choose how mistakes
  behave: *continue*, *stop on error*, or *must backspace*.
- **Results** show speed, accuracy, raw speed, consistency, max combo, a
  verdict in words, a per-second speed chart with error markers, missed keys, and the words queued
  for review.
- **Stats** has five tabs: progress charts, key heatmap, bigram heat matrix,
  finger and hand analysis (load, speed, alternation, same-finger rate), and
  the streak calendar.

## Keys

| Where | Key | Action |
|---|---|---|
| Menu | `↑↓` / `jk`, `enter`, `1`–`9` | choose and start |
| Menu | `←→` / `hl` | change test length or code language |
| Menu | `s`, `,`, `?`, `q` | stats, settings, help, quit |
| Drill | `tab` (`ctrl+n` in code) | new drill |
| Drill | `ctrl+r` | restart the same text |
| Drill | `ctrl+w` / `ctrl+backspace` | delete word |
| Drill | `ctrl+k` / `F2`, `ctrl+t` / `F3` | cycle keyboard style, theme |
| Drill | `esc` | back to menu (time still counts toward today) |
| Results | `enter`, `r`, `s`, `esc` | next, retry, stats, menu |

## Command line

```
kbtrain [OPTIONS]

  -l, --layout <ID>     qwerty, colemak, colemak-dh, dvorak, workman, qwertz, azerty
  -t, --theme <NAME>    synthwave, midnight, sakura, phosphor, glacier, paper, mono
  -f, --file <PATH>     practise your own text (remembered for "My text")
  -m, --mode <MODE>     learn, review, transitions, words, test, code, file
      --stats           print a progress summary and exit
      --profile <PATH>  use a different progress file
      --config <PATH>   use a different config file
      --list-layouts, --list-themes
```

Each layout keeps its own statistics, so you can learn Colemak without
disturbing your QWERTY history.

## Configuration

Everything is editable from the in-app Settings screen, which writes
`config.toml` in your config directory (`~/.config/kbtrain/` on Linux). You
can also edit it directly:

```toml
layout = "colemak-dh"
theme = "synthwave"
keyboard = "adaptive"        # large | adaptive | compact | hidden
error_mode = "continue"      # continue | stop | backspace
target_wpm = 40
daily_goal_minutes = 15
drill_length = 25
progressive_unlock = true
animations = "full"          # full | reduced | off
color = "auto"               # auto | truecolor | ansi256
bell_on_error = false

# Your own layouts: four rows (number, top, home, bottom). Shifted
# characters default to uppercase if shift_rows is omitted.
[[layouts]]
id = "my-layout"
name = "My Layout"
rows = ["`1234567890-=", "qwfpbjluy;[]\\", "arstgmneio'", "zxcdvkh,./"]
```

Progress lives in `profile.json` in your data directory
(`~/.local/share/kbtrain/` on Linux). It is written atomically and never
leaves your machine.

## Terminal notes

kbtrain uses 24-bit colour when the terminal advertises it (`COLORTERM`) and
otherwise falls back to the nearest 256-colour palette. Set `color =
"truecolor"` if your terminal supports it without saying so. Use a font with
box-drawing and block characters (most modern monospace fonts). Over slow SSH
links, set `animations = "reduced"`.

## Development

```
cargo test                    # unit tests plus render tests at many sizes
cargo test dump_screens -- --ignored --nocapture   # print every screen
KBTRAIN_DUMP_HTML=out cargo test dump_screens -- --ignored  # screens and animation moments as HTML
cargo test --release redraw_cost -- --ignored --nocapture  # bytes sent to the terminal
```
