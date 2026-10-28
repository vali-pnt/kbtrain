# kbtrain design decisions

## Visuals
- **Default theme: neon synthwave.** Deep indigo background, magenta→cyan gradients, glow, rounded box-drawn keys. Themes are swappable.
- **Motion: rich but calm while typing.** Particles, gradients and glow on the home menu and results. During a drill only subtle cues: cursor pulse, key-press ripple, combo meter.
- **Launch: animated home menu.** Synthwave title, mode select, stats summary, today's goal.

## On-screen keyboard
Toggleable modes (cycle with a key):
1. Large, finger-zoned keys with a heatmap overlay and press ripple
2. Adaptive/fading: hidden while typing fluently, fades in on hesitation or errors
3. Compact heatmap strip
4. Hidden

## Pedagogy
- **Progressive key unlock:** start on the home row and unlock the next key once the current set is fluent (mastery learning).
- **Spaced repetition:** weak keys, bigrams and words resurface on a forgetting-curve schedule.
- **Bigram/trigram drills:** target slow transitions, not just single keys.
- **Real-text practice:** quotes, code, and the user's own files.
- **Errors:** configurable (stop / continue / must backspace); default continue.

## Motivation
- Personal-best trends only (sparklines, PB markers). No leaderboards.
- Flow combo meter for consecutive correct keys.
- Daily goal + streak calendar.

## Analytics
- Key & bigram heatmaps
- Finger/hand analysis (load, speed, alternation, same-finger bigrams)
- Progress charts (WPM, accuracy, keys mastered over time)
