use std::time::{Duration, Instant};

use rand::seq::SliceRandom;

use crate::dict::Pair;
use crate::mistakes::MistakeBank;
use crate::model::{Lang, SpeedUnit};
use crate::stats::{Counters, DualStats};
use crate::text::{chars_match, count_correct, prepare_line, SKIP_PUNCT};

/// The speed clock keeps running this long after the last key, then holds.
const SPEED_IDLE: Duration = Duration::from_secs(2);

/// One line the user is typing.
#[derive(Clone, Debug)]
pub struct Side {
    target: String,
    typed: String,
    keys: u64,
    pace: Pace,
    /// Parallel to `typed` chars. These were filled in, not typed, and do not score.
    ignored: Vec<bool>,
    word_mistake_at: Option<usize>,
    committed: bool,
}

impl Side {
    pub fn new(target: String) -> Self {
        Self {
            target,
            typed: String::new(),
            keys: 0,
            pace: Pace::default(),
            ignored: Vec::new(),
            word_mistake_at: None,
            committed: false,
        }
    }

    pub fn target(&self) -> &str {
        &self.target
    }

    pub fn typed(&self) -> &str {
        &self.typed
    }

    pub fn done(&self) -> bool {
        self.typed_len() >= self.target_len() && self.target_len() > 0
    }

    pub fn backspace(&mut self) {
        let removed = self.typed.pop().is_some();
        if removed {
            self.ignored.pop();
        }
        if removed
            && self
                .word_mistake_at
                .is_some_and(|at| self.typed_len() <= at)
        {
            self.word_mistake_at = None;
        }
        if removed && self.pace.started() {
            self.pace.on_input(Instant::now());
        }
    }

    /// Appends `ch` when the line is not finished. Returns the index just typed.
    pub fn type_char(&mut self, ch: char) -> Option<usize> {
        if self.typed_len() >= self.target_len() {
            return None;
        }
        self.pace.on_input(Instant::now());
        self.typed.push(ch);
        self.ignored.push(false);
        self.keys += 1;
        Some(self.typed_len() - 1)
    }

    /// Fills the next target character without a keystroke and without scoring it.
    pub(crate) fn ignore_expected(&mut self) -> bool {
        if self.typed_len() >= self.target_len() {
            return false;
        }
        let Some(ch) = self.target.chars().nth(self.typed_len()) else {
            return false;
        };
        self.typed.push(ch);
        self.ignored.push(true);
        true
    }

    pub(crate) fn ends_with_ignored(&self) -> bool {
        self.ignored.last().copied().unwrap_or(false)
    }

    /// Keeps the speed clock alive when a key is swallowed and nothing is typed.
    pub(crate) fn note_key(&mut self) {
        if self.pace.started() {
            self.pace.on_input(Instant::now());
        }
    }

    fn typed_len(&self) -> usize {
        self.typed.chars().count()
    }

    fn target_len(&self) -> usize {
        self.target.chars().count()
    }

    fn snapshot(&self) -> Counters {
        Counters {
            keystrokes: self.keys,
            correct: count_correct(&self.target, &self.typed, &self.ignored),
            seconds: self.pace.seconds(Instant::now()),
            sides: 0,
        }
    }

    pub(crate) fn pending(&self) -> Counters {
        self.snapshot()
    }

    pub(crate) fn word_flag(&mut self) -> &mut Option<usize> {
        &mut self.word_mistake_at
    }

    pub(crate) fn has_progress(&self) -> bool {
        !self.typed.is_empty() && !self.committed
    }

    fn is_committed(&self) -> bool {
        self.committed
    }

    pub(crate) fn take_commit(&mut self) -> Option<Counters> {
        if self.committed {
            return None;
        }
        let mut piece = self.snapshot();
        piece.sides = 1;
        self.committed = true;
        Some(piece)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Typing(Lang),
    Done,
}

impl Phase {
    pub fn lang(self) -> Option<Lang> {
        match self {
            Phase::Typing(lang) => Some(lang),
            Phase::Done => None,
        }
    }
}

#[derive(Clone, Debug)]
pub struct PairRow {
    pub en: Side,
    pub ru: Side,
    pub phase: Phase,
}

impl PairRow {
    pub fn side(&self, lang: Lang) -> &Side {
        match lang {
            Lang::En => &self.en,
            Lang::Ru => &self.ru,
        }
    }

    fn side_mut(&mut self, lang: Lang) -> &mut Side {
        match lang {
            Lang::En => &mut self.en,
            Lang::Ru => &mut self.ru,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Solo,
    Versus,
}

pub struct Config {
    pub deck: Vec<Pair>,
    pub speed_unit: SpeedUnit,
    pub first_lang: Lang,
    pub hide_punctuation: bool,
    pub no_capitals: bool,
    pub mode: Mode,
    pub words_per_line: usize,
    pub repeat_word: bool,
    pub title: String,
}

/// A finished side, ready to fold into lifetime stats.
pub struct Commit {
    pub lang: Lang,
    pub counters: Counters,
    pub complete_pair: bool,
}

pub struct KeyResult {
    pub commit: Option<Commit>,
    pub switched_to: Option<Lang>,
}

impl KeyResult {
    fn none() -> Self {
        Self {
            commit: None,
            switched_to: None,
        }
    }
}

pub struct Game {
    deck: Vec<Pair>,
    queue: Vec<Pair>,
    pub rows: Vec<PairRow>,
    pub solo_rows: Vec<Side>,
    session: DualStats,
    pub active_lang: Lang,
    pub first_lang: Lang,
    hide_punctuation: bool,
    no_capitals: bool,
    pub speed_unit: SpeedUnit,
    pub mode: Mode,
    words_per_line: usize,
    repeat_word: bool,
    pub title: String,
    pub layout_ok: bool,
    /// One space may be ignored after a side switch, in either direction.
    bridge_space: bool,
}

impl Game {
    pub fn new(config: Config) -> Self {
        Self {
            deck: config.deck,
            queue: Vec::new(),
            rows: Vec::new(),
            solo_rows: Vec::new(),
            session: DualStats::default(),
            active_lang: config.first_lang,
            first_lang: config.first_lang,
            hide_punctuation: config.hide_punctuation,
            no_capitals: config.no_capitals,
            speed_unit: config.speed_unit,
            mode: config.mode,
            words_per_line: config.words_per_line.max(1),
            repeat_word: config.repeat_word,
            title: config.title,
            layout_ok: false,
            bridge_space: false,
        }
    }

    pub fn session(&self) -> &DualStats {
        &self.session
    }

    pub fn second_lang(&self) -> Lang {
        self.first_lang.other()
    }

    pub fn ensure_started(&mut self) {
        match self.mode {
            Mode::Solo if self.solo_rows.is_empty() => {
                let side = self.new_solo();
                self.solo_rows.push(side);
                self.active_lang = self.first_lang;
            }
            Mode::Versus if self.needs_row() => {
                let row = self.new_row();
                self.rows.push(row);
                self.active_lang = self.first_lang;
            }
            _ => {}
        }
    }

    pub fn live(&self) -> DualStats {
        let Some(lang) = self.open_lang() else {
            return self.session.clone();
        };
        match self.side_ref(lang) {
            Some(side) if side.has_progress() => {
                self.session.plus_side(lang, side.snapshot(), false)
            }
            _ => self.session.clone(),
        }
    }

    pub fn backspace(&mut self) {
        if let Some(side) = self.active_side_mut() {
            side.backspace();
        }
    }

    pub fn on_char(&mut self, ch: char, bank: &mut MistakeBank) -> KeyResult {
        if self.hide_punctuation && SKIP_PUNCT.contains(ch) {
            return KeyResult::none();
        }
        if self.reject_bridge_space(ch) {
            return KeyResult::none();
        }
        let before = self.active_lang;
        if !self.input_char(ch, bank) {
            return KeyResult::none();
        }
        let commit = self.advance();
        let switched_to = (self.active_lang != before).then_some(self.active_lang);
        KeyResult {
            commit,
            switched_to,
        }
    }

    /// Commits the side in progress, if the user has typed anything on it.
    pub fn finish(&mut self) -> Option<Commit> {
        let lang = self.open_lang()?;
        if self.side_ref(lang).is_some_and(Side::has_progress) {
            self.commit_lang(lang, false)
        } else {
            None
        }
    }

    fn needs_row(&self) -> bool {
        self.rows.is_empty() || self.rows.last().is_some_and(|row| row.phase == Phase::Done)
    }

    fn prepare(&self, text: &str) -> String {
        prepare_line(text, self.hide_punctuation, self.no_capitals)
    }

    fn draw_pair(&mut self) -> Pair {
        if self.queue.is_empty() {
            self.queue = self.deck.clone();
            self.queue.shuffle(&mut rand::thread_rng());
        }
        self.queue.pop().unwrap_or_default()
    }

    fn take_bundle(&mut self) -> Pair {
        let count = self.words_per_line.max(1);
        let first = self.draw_pair();
        if self.repeat_word {
            return Pair::new(
                vec![first.en.as_str(); count].join(" "),
                vec![first.ru.as_str(); count].join(" "),
            );
        }
        let mut en = vec![first.en];
        let mut ru = vec![first.ru];
        for _ in 1..count {
            let extra = self.draw_pair();
            en.push(extra.en);
            ru.push(extra.ru);
        }
        Pair::new(en.join(" "), ru.join(" "))
    }

    fn new_row(&mut self) -> PairRow {
        let pair = self.take_bundle();
        PairRow {
            en: Side::new(self.prepare(&pair.en)),
            ru: Side::new(self.prepare(&pair.ru)),
            phase: Phase::Typing(self.first_lang),
        }
    }

    fn new_solo(&mut self) -> Side {
        let pair = self.take_bundle();
        Side::new(self.prepare(pair.text(self.first_lang)))
    }

    fn open_lang(&self) -> Option<Lang> {
        match self.mode {
            Mode::Solo => self.solo_rows.last().map(|_| self.first_lang),
            Mode::Versus => self.rows.last().and_then(|row| row.phase.lang()),
        }
    }

    fn side_ref(&self, lang: Lang) -> Option<&Side> {
        match self.mode {
            Mode::Solo => self.solo_rows.last(),
            Mode::Versus => self.rows.last().map(|row| row.side(lang)),
        }
    }

    fn side_mut(&mut self, lang: Lang) -> Option<&mut Side> {
        match self.mode {
            Mode::Solo => self.solo_rows.last_mut(),
            Mode::Versus => self.rows.last_mut().map(|row| row.side_mut(lang)),
        }
    }

    fn active_side_mut(&mut self) -> Option<&mut Side> {
        self.ensure_started();
        match self.mode {
            Mode::Solo => self.solo_rows.last_mut(),
            Mode::Versus => {
                let lang = self.rows.last().and_then(|row| row.phase.lang())?;
                self.rows.last_mut().map(|row| row.side_mut(lang))
            }
        }
    }

    fn commit_lang(&mut self, lang: Lang, complete_pair: bool) -> Option<Commit> {
        let piece = self.side_mut(lang)?.take_commit()?;
        self.session = self.session.plus_side(lang, piece.clone(), complete_pair);
        Some(Commit {
            lang,
            counters: piece,
            complete_pair,
        })
    }

    fn advance(&mut self) -> Option<Commit> {
        match self.mode {
            Mode::Solo => self.advance_solo(),
            Mode::Versus => self.advance_versus(),
        }
    }

    fn advance_solo(&mut self) -> Option<Commit> {
        let ready = self
            .solo_rows
            .last()
            .is_some_and(|side| side.done() && !side.is_committed());
        if !ready {
            return None;
        }
        let commit = self.commit_lang(self.first_lang, false)?;
        let next = self.new_solo();
        self.solo_rows.push(next);
        Some(commit)
    }

    fn advance_versus(&mut self) -> Option<Commit> {
        let first = self.first_lang;
        let second = first.other();
        let step = {
            let row = self.rows.last()?;
            if row.phase == Phase::Typing(first) && row.side(first).done() {
                VersusStep::First
            } else if row.phase == Phase::Typing(second) && row.side(second).done() {
                VersusStep::Second
            } else {
                return None;
            }
        };
        match step {
            VersusStep::First => {
                let commit = self.commit_lang(first, false);
                if let Some(row) = self.rows.last_mut() {
                    row.phase = Phase::Typing(second);
                }
                self.active_lang = second;
                self.bridge_space = true;
                commit
            }
            VersusStep::Second => {
                let commit = self.commit_lang(second, true);
                if let Some(row) = self.rows.last_mut() {
                    row.phase = Phase::Done;
                }
                let next = self.new_row();
                self.rows.push(next);
                self.active_lang = first;
                self.bridge_space = true;
                commit
            }
        }
    }

    /// Returns true when this space is the free one after a language switch.
    fn reject_bridge_space(&mut self, ch: char) -> bool {
        if self.mode == Mode::Versus && ch == ' ' && self.bridge_space {
            self.bridge_space = false;
            return self.active_side_is_fresh();
        }
        if ch != ' ' {
            self.bridge_space = false;
        }
        false
    }

    fn active_side_is_fresh(&self) -> bool {
        let Some(row) = self.rows.last() else {
            return false;
        };
        let Some(lang) = row.phase.lang() else {
            return false;
        };
        let side = row.side(lang);
        side.typed().is_empty() && !side.target().starts_with(' ')
    }

    fn input_char(&mut self, ch: char, bank: &mut MistakeBank) -> bool {
        let lang = self.active_lang;
        let Some(side) = self.active_side_mut() else {
            return false;
        };
        let Some(pos) = side.type_char(ch) else {
            return false;
        };
        let mismatch = side
            .target()
            .chars()
            .nth(pos)
            .is_some_and(|expected| !chars_match(ch, expected));
        if mismatch {
            let target = side.target().to_string();
            bank.note_typo(lang, &target, pos, ch, side.word_flag());
        }
        true
    }
}

enum VersusStep {
    First,
    Second,
}

/// Typing time for one line. Idle longer than [`SPEED_IDLE`] is left out,
/// so the live speed holds instead of falling while the user is stopped.
#[derive(Clone, Debug, Default)]
struct Pace {
    closed: f64,
    running_since: Option<Instant>,
    last_input: Option<Instant>,
}

impl Pace {
    fn started(&self) -> bool {
        self.running_since.is_some()
    }

    fn on_input(&mut self, now: Instant) {
        if let Some(since) = self.running_since {
            if let Some(cap) = self.idle_cap() {
                if now > cap {
                    self.closed += cap.saturating_duration_since(since).as_secs_f64();
                    self.running_since = Some(now);
                }
            }
        } else {
            self.running_since = Some(now);
        }
        self.last_input = Some(now);
    }

    fn seconds(&self, now: Instant) -> f64 {
        let Some(since) = self.running_since else {
            return self.closed;
        };
        let end = match self.idle_cap() {
            Some(cap) if now > cap => cap,
            _ => now,
        };
        self.closed + end.saturating_duration_since(since).as_secs_f64()
    }

    fn idle_cap(&self) -> Option<Instant> {
        self.last_input
            .and_then(|last| last.checked_add(SPEED_IDLE))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn game(en: &str, ru: &str) -> Game {
        Game::new(Config {
            deck: vec![Pair::new(en, ru)],
            speed_unit: SpeedUnit::Wpm,
            first_lang: Lang::En,
            hide_punctuation: false,
            no_capitals: false,
            mode: Mode::Versus,
            words_per_line: 1,
            repeat_word: false,
            title: "test".into(),
        })
    }

    #[test]
    fn bridging_space_is_not_a_typo() {
        let mut g = game("hi", "да");
        let mut bank = MistakeBank::default();
        g.on_char('h', &mut bank);
        g.on_char('i', &mut bank);
        assert_eq!(g.active_lang, Lang::Ru);
        let spaced = g.on_char(' ', &mut bank);
        assert!(spaced.switched_to.is_none());
        assert!(spaced.commit.is_none());
        assert!(g.rows.last().unwrap().ru.typed().is_empty());
        g.on_char('д', &mut bank);
        assert_eq!(g.rows.last().unwrap().ru.typed(), "д");
    }

    #[test]
    fn only_one_bridging_space_each_way() {
        let mut g = game("hi", "да");
        let mut bank = MistakeBank::default();
        g.on_char('h', &mut bank);
        g.on_char('i', &mut bank);
        assert!(
            g.on_char(' ', &mut bank).commit.is_none()
                && g.rows.last().unwrap().ru.typed().is_empty()
        );
        g.on_char(' ', &mut bank);
        assert_eq!(g.rows.last().unwrap().ru.typed(), " ");

        let mut g = game("hi", "да");
        g.on_char('h', &mut bank);
        g.on_char('i', &mut bank);
        g.on_char('д', &mut bank);
        g.on_char('а', &mut bank);
        assert_eq!(g.active_lang, Lang::En);
        assert!(g.on_char(' ', &mut bank).commit.is_none());
        assert!(g.rows.last().unwrap().en.typed().is_empty());
        g.on_char(' ', &mut bank);
        assert_eq!(g.rows.last().unwrap().en.typed(), " ");
    }

    #[test]
    fn speed_clock_holds_after_a_pause_and_drops_the_gap() {
        let t0 = Instant::now();
        let mut pace = Pace::default();
        pace.on_input(t0);
        pace.on_input(t0 + Duration::from_millis(400));
        let typing = pace.seconds(t0 + Duration::from_millis(400));
        assert!((typing - 0.4).abs() < 0.001);

        let paused = t0 + Duration::from_secs(10);
        let held = pace.seconds(paused);
        assert!((held - 2.4).abs() < 0.001);

        pace.on_input(paused);
        let resumed = pace.seconds(paused + Duration::from_millis(500));
        assert!((resumed - 2.9).abs() < 0.001);
    }

    #[test]
    fn short_gaps_keep_the_speed_clock_running() {
        let t0 = Instant::now();
        let mut pace = Pace::default();
        pace.on_input(t0);
        let mid = t0 + Duration::from_millis(1500);
        pace.on_input(mid);
        let later = mid + Duration::from_millis(1500);
        assert!((pace.seconds(later) - 3.0).abs() < 0.001);
    }

    #[test]
    fn repeat_one_word() {
        let mut g = game("house", "дом");
        g.words_per_line = 3;
        g.repeat_word = true;
        let pair = g.take_bundle();
        assert_eq!(pair.en, "house house house");
        assert_eq!(pair.ru, "дом дом дом");
    }
}
