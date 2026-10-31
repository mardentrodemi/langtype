# langtype

Terminal trainer for typing speed and English. Default: English on the left, Russian on the right.

**Typing practice** uses only the left language: one sentence after another until you press Esc. Speed is saved to that language. **Language vs language** is the two-column EN/RU mode: **Sentences** uses Tatoeba pairs; **Words** uses one dictionary word per side.

Mistakes do not stop you: a wrong character is highlighted red and you can keep going. Backspace is optional.

## Run

```bash
cargo run --release
```

Rust, [ratatui](https://ratatui.rs/), and a color terminal. The screen is a dark panel layout: ASCII title, bordered menus, live speed and accuracy, a progress bar, and a session card when you press Esc.

## Screen

- **Top** — session and all-time speed per language (`EN` / `RU` + `WPM` or `CPM`), in the same order as the columns. Time and pairs stay on the same line.
- **Left / right** — first and second language (default **EN left, RU right**; change in Settings).
- Menu: **Start** → Typing practice (left lang) / Typing practice (right lang) / Language vs language (**Words** or **Sentences**) / Practice mistakes. Then **Settings**, **Quit**.
- Green = correct, red = error, cursor is highlighted. Esc shows session stats, then returns to the menu after 7 seconds (Enter skips).

Mistakes (wrong letters, 4-letter syllables, and whole words) are stored per language with a frequency count, at the moment you type the wrong character in typing practice — even if you then fix it. A **syllable** is the mistyped letter plus the three letters before it, skipping spaces (so a typo on **а** in «привет админ» saves **вета**). **Practice mistakes** does not add new mistakes and does not change speed stats; only all-time duration grows. Pick language, then **Letters** / **Words** / **Syllables**, then **One after another** (mixed on one line, filled to the terminal; items with more errors appear more often, but the same token is unlikely twice in a row) or **In a line** (one item per line, repeated; how many consecutive lines is set in Settings; the next item is also weighted by error count). Esc returns to the menu.

Each side’s timer starts on the first character. If you stop, the speed clock keeps going for 2 seconds, then holds until the next key. That pause is not counted.

## Speed units

In Settings choose speed unit, language order, whether to hide punctuation (`. , ! ? : ; ' "` and similar quotes — skipped so you do not type them when switching layouts), **No capitals** (uppercase letters in sentences and in Practice mistakes become lowercase), **In-a-line rows** (1 / 3 / 6 / 9 consecutive lines of the same letter, word, or syllable before switching), and **Words per line** (1 / 3 / 5 words at a time in Language vs language → Words; **Repeat one word** repeats the same pair instead of different words).

- **WPM** — (characters / 5) / minutes
- **CPM** — characters / minutes

Net uses correct characters; gross uses every typed character.

## Layout

On niri the keyboard layout switches by itself to the language you are typing. If auto-switch is unavailable, the footer tells you to switch manually.

Russian **е** and **ё** (and **Е** / **Ё**) count as the same letter. Any dash (**—**, **–**, **−**, …) counts as hyphen-minus **-**. Straight, curly, and guillemet quotes (`"`, `“”`, `«»`) count as the same character. None of these is treated as a typo.

## Data

User files live in `~/.local/share/langtype/`. An existing `~/.local/share/ruentype` folder is renamed there on the first launch.

- `lifetime.json` — all-time speed (keystrokes, correct chars, time, pairs, sessions, separate EN/RU) plus practice-mistakes duration
- `mistakes-letters.json`, `mistakes-words.json`, `mistakes-syllables.json` — mistakes per language. Words and syllables stay at most 1000 entries each; new ones replace random older ones. Written at the end of a typing session, not on every typo.
- `settings.json` — WPM/CPM, language order, hide punctuation, no capitals, in-a-line rows, words per line

Session totals stay in memory until a finished side is written into `lifetime.json`.

## Dictionary

**13 800** sentence pairs (Russian ↔ English) from Tatoeba via OPUS, license [CC BY 2.0 FR](https://creativecommons.org/licenses/by/2.0/fr/).

**3 767** learner word pairs, selected for learning English: **3 672** entries from the A1–B2 portion of [Oxford 3000 / 5000](https://www.oxfordlearnersdictionaries.com/about/wordlists/oxford3000-5000), plus **95** everyday additions such as `kettle`, `toothbrush`, `wallet`, `turn on`, and `look for`. This replaces the previous Russian-headword-based SMARTool/OpenRussian word list.

Russian equivalents were reviewed and edited to favour familiar meanings: `money → деньги`, `receipt → чек`, `boot → сапог`, `bush → куст`, `bored → скучающий`, and `boring → скучный`. Each English entry has one representative meaning; this is a practice vocabulary, not an exhaustive dictionary of every sense. Short everyday phrases are included. C1-only entries and articles without a direct Russian equivalent are excluded. The spelling is mainly British; English days, months, and proper names retain their capitals.

Edit [data/words.tsv](data/words.tsv) to maintain the vocabulary. Its columns are `en`, `ru`, `level`, and `source`. Oxford levels are the lowest listed level for the source headword; they do not classify every meaning of a word. Everyday additions have no assigned CEFR level. These labels are editorial metadata; the trainer shuffles the combined vocabulary.

```bash
python3 scripts/build_words.py          # export both runtime word files
python3 scripts/build_words.py --check  # validate entries and check exports
```

`data/words_en.txt` and `data/words_ru.txt` contain one aligned pair per line. Every entry is 1–24 characters, as required by the word-mode loader. The exporter checks spelling characters, duplicate English entries, source labels, and alignment. See [data/WORDS-LICENSE.txt](data/WORDS-LICENSE.txt) for attribution and data rights. Sentence data is maintained separately.

## License

The program code is under the [PolyForm Noncommercial License 1.0.0](LICENSE). You may use, change, and share it for personal use, study, hobby projects, and noncommercial organizations such as schools and charities. Selling the program or a modified version is outside the allowed purposes. The license text itself is the terms; this paragraph is only a short description.

Required Notice: Copyright 2026 MARADENTRO (https://github.com/mardentrodemi/langtype)

Sentence pairs and word lists keep their own terms: [data/TATOEBA-LICENSE.txt](data/TATOEBA-LICENSE.txt) and [data/WORDS-LICENSE.txt](data/WORDS-LICENSE.txt).
