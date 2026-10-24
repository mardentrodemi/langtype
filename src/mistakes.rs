use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use rand::seq::SliceRandom;
use rand::Rng;
use regex::Regex;
use serde_json::{Map, Value};

use crate::model::{ByLang, Lang};
use crate::paths::{self, user_dir};
use crate::text::chars_match;

static WORD_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"[A-Za-zА-Яа-яЁё]+(?:['’-][A-Za-zА-Яа-яЁё]+)*").expect("word regex")
});

const MAX_TRACKED: usize = 1000;
const PASSAGE_ATTEMPTS: usize = 400;
const KIND_FILES: [&str; 3] = ["letters", "words", "syllables"];

const REPEAT_LAST: f64 = 0.16;
const REPEAT_OLDER: f64 = 0.48;
const SHARE_HIGH: f64 = 0.36;
const SHARE_HIGH_WEIGHT: f64 = 0.08;
const SHARE_MID: f64 = 0.24;
const SHARE_MID_WEIGHT: f64 = 0.4;
const MIN_WEIGHT: f64 = 1e-9;

type Counts = HashMap<String, u64>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MistakeKind {
    Letters,
    Words,
    Syllables,
}

impl MistakeKind {
    pub fn as_str(self) -> &'static str {
        match self {
            MistakeKind::Letters => "letters",
            MistakeKind::Words => "words",
            MistakeKind::Syllables => "syllables",
        }
    }

    pub fn label(self, ui: crate::i18n::UiLang) -> &'static str {
        match (self, ui) {
            (MistakeKind::Letters, crate::i18n::UiLang::En) => "Letters",
            (MistakeKind::Words, crate::i18n::UiLang::En) => "Words",
            (MistakeKind::Syllables, crate::i18n::UiLang::En) => "Syllables",
            (MistakeKind::Letters, crate::i18n::UiLang::Ru) => "Буквы",
            (MistakeKind::Words, crate::i18n::UiLang::Ru) => "Слова",
            (MistakeKind::Syllables, crate::i18n::UiLang::Ru) => "Слоги",
        }
    }

    fn all() -> [Self; 3] {
        [Self::Letters, Self::Words, Self::Syllables]
    }
}

#[derive(Clone, Debug, Default)]
struct SessionKeys {
    words: ByLang<HashSet<String>>,
    syllables: ByLang<HashSet<String>>,
}

#[derive(Clone, Debug)]
pub struct MistakeBank {
    letters: ByLang<Counts>,
    words: ByLang<Counts>,
    syllables: ByLang<Counts>,
    directory: PathBuf,
    dirty: bool,
    session_start: SessionKeys,
}

impl Default for MistakeBank {
    fn default() -> Self {
        Self {
            letters: ByLang::default(),
            words: ByLang::default(),
            syllables: ByLang::default(),
            directory: PathBuf::new(),
            dirty: false,
            session_start: SessionKeys::default(),
        }
    }
}

impl MistakeBank {
    #[cfg(test)]
    pub(crate) fn in_directory(directory: impl Into<PathBuf>) -> Self {
        Self {
            directory: directory.into(),
            ..Self::default()
        }
    }

    pub fn load() -> Self {
        let directory = user_dir();
        let mut bank = Self {
            directory: directory.clone(),
            ..Self::default()
        };
        if split_files_exist(&directory) {
            for kind in MistakeKind::all() {
                *bank.bucket_mut(kind) =
                    read_kind_file(&directory.join(format!("mistakes-{}.json", kind.as_str())));
            }
            bank.capture_old();
            return bank;
        }
        if let Some(data) = read_json(&directory.join("mistakes.json")) {
            bank.import_legacy(&data);
            bank.capture_old();
            bank.dirty = true;
            bank.flush();
        } else {
            bank.capture_old();
        }
        bank
    }

    pub fn begin_session(&mut self) {
        self.capture_old();
    }

    pub fn note_typo(
        &mut self,
        lang: Lang,
        target: &str,
        pos: usize,
        typed_ch: char,
        word_flag: &mut Option<usize>,
    ) {
        let Some(expected) = target.chars().nth(pos) else {
            return;
        };
        if chars_match(typed_ch, expected) {
            return;
        }
        if expected.is_alphabetic() {
            self.add(MistakeKind::Letters, lang, &expected.to_string());
        }
        if let Some(syllable) = syllable_at(target, pos) {
            self.add(MistakeKind::Syllables, lang, &syllable);
        }
        if let Some((word, start)) = word_span_at(target, pos) {
            if *word_flag != Some(start) {
                self.add(MistakeKind::Words, lang, &word);
                *word_flag = Some(start);
            }
        }
    }

    pub fn items(&self, lang: Lang, kind: MistakeKind) -> Vec<String> {
        self.bucket(kind)[lang].keys().cloned().collect()
    }

    pub fn ranked(&self, lang: Lang, kind: MistakeKind) -> Vec<(String, u64)> {
        sorted(&self.bucket(kind)[lang])
    }

    pub fn top_line(&self, lang: Lang, kind: MistakeKind, ui: crate::i18n::UiLang) -> String {
        let ranked = self.ranked(lang, kind);
        let head = ui.say("Most frequent", "Чаще всего");
        if ranked.is_empty() {
            return format!("{head}: —");
        }
        let bits: Vec<String> = ranked
            .into_iter()
            .take(12)
            .map(|(item, count)| format!("{item}×{count}"))
            .collect();
        format!("{head}:  {}", bits.join("  "))
    }

    pub fn flush(&mut self) {
        if !self.dirty {
            return;
        }
        self.trim_capped();
        for kind in MistakeKind::all() {
            self.write_kind(kind);
        }
        self.dirty = false;
        self.capture_old();
    }

    fn add(&mut self, kind: MistakeKind, lang: Lang, key: &str) {
        if key.is_empty() {
            return;
        }
        *self.bucket_mut(kind)[lang]
            .entry(key.to_string())
            .or_insert(0) += 1;
        self.dirty = true;
    }

    fn bucket(&self, kind: MistakeKind) -> &ByLang<Counts> {
        match kind {
            MistakeKind::Letters => &self.letters,
            MistakeKind::Words => &self.words,
            MistakeKind::Syllables => &self.syllables,
        }
    }

    fn bucket_mut(&mut self, kind: MistakeKind) -> &mut ByLang<Counts> {
        match kind {
            MistakeKind::Letters => &mut self.letters,
            MistakeKind::Words => &mut self.words,
            MistakeKind::Syllables => &mut self.syllables,
        }
    }

    fn capture_old(&mut self) {
        self.session_start = SessionKeys {
            words: key_sets(&self.words),
            syllables: key_sets(&self.syllables),
        };
    }

    /// Prefer dropping keys that already existed when the session started, so new mistakes survive.
    fn trim_capped(&mut self) {
        for lang in [Lang::Ru, Lang::En] {
            trim_to_cap(&mut self.words[lang], &self.session_start.words[lang]);
            trim_to_cap(
                &mut self.syllables[lang],
                &self.session_start.syllables[lang],
            );
        }
    }

    fn write_kind(&self, kind: MistakeKind) {
        let mut root = Map::new();
        for lang in [Lang::Ru, Lang::En] {
            let mut map = Map::new();
            for (key, count) in sorted(&self.bucket(kind)[lang]) {
                map.insert(key, Value::from(count));
            }
            root.insert(lang.as_str().to_string(), Value::Object(map));
        }
        paths::write_json(
            &self
                .directory
                .join(format!("mistakes-{}.json", kind.as_str())),
            &Value::Object(root),
        );
    }

    fn import_legacy(&mut self, data: &Value) {
        for kind in MistakeKind::all() {
            let Some(raw_kind) = data.get(kind.as_str()).and_then(Value::as_object) else {
                continue;
            };
            for lang in [Lang::Ru, Lang::En] {
                let Some(obj) = raw_kind.get(lang.as_str()).and_then(Value::as_object) else {
                    continue;
                };
                let dest = &mut self.bucket_mut(kind)[lang];
                for (key, value) in obj {
                    if let Some(count) = value.as_u64() {
                        if !key.is_empty() {
                            dest.insert(key.clone(), count);
                        }
                    }
                }
            }
        }
    }
}

pub fn pick_weighted(
    ranked: &[(String, u64)],
    recent: &[String],
    used: &HashMap<String, u32>,
) -> String {
    let pool: Vec<(&str, f64)> = ranked
        .iter()
        .filter(|(item, _)| !item.is_empty())
        .map(|(item, count)| (item.as_str(), error_weight(*count)))
        .collect();
    if pool.is_empty() {
        return String::new();
    }
    if pool.len() == 1 {
        return pool[0].0.to_string();
    }
    let picked: u32 = used.values().sum();
    let mut choices = Vec::with_capacity(pool.len());
    let mut weights = Vec::with_capacity(pool.len());
    for (item, base) in pool {
        let weight = (base * recency_factor(item, recent) * diversity_factor(item, used, picked))
            .max(MIN_WEIGHT);
        choices.push(item.to_string());
        weights.push(weight);
    }
    let total: f64 = weights.iter().sum();
    let mut mark = rand::thread_rng().gen::<f64>() * total;
    for (item, weight) in choices.iter().zip(weights.iter()) {
        mark -= weight;
        if mark <= 0.0 {
            return item.clone();
        }
    }
    choices.last().cloned().unwrap_or_default()
}

pub fn build_weighted_passage(ranked: &[(String, u64)], max_chars: usize) -> String {
    if !ranked.iter().any(|(item, _)| !item.is_empty()) {
        return String::new();
    }
    let cap = max_chars.max(1);
    let mut parts = Vec::new();
    let mut recent = Vec::new();
    let mut used = HashMap::new();
    let mut length = 0usize;
    for _ in 0..PASSAGE_ATTEMPTS {
        if length >= cap {
            break;
        }
        let item = pick_weighted(ranked, &recent, &used);
        if item.is_empty() {
            break;
        }
        let extra = if parts.is_empty() {
            item.clone()
        } else {
            format!(" {item}")
        };
        if !parts.is_empty() && length + extra.chars().count() > cap {
            break;
        }
        length += extra.chars().count();
        *used.entry(item.clone()).or_default() += 1;
        recent.push(item);
        if recent.len() > 2 {
            recent.remove(0);
        }
        parts.push(extra);
    }
    parts.concat()
}

pub fn build_repeat_line(item: &str, width: usize) -> String {
    let item = item.trim();
    if item.is_empty() {
        return String::new();
    }
    let width = width.max(8);
    let mut copies = ((width + 1) / (item.chars().count() + 1)).max(4);
    let mut line = vec![item; copies].join(" ");
    while copies > 4 && line.chars().count() > width {
        copies -= 1;
        line = vec![item; copies].join(" ");
    }
    line
}

fn error_weight(count: u64) -> f64 {
    (count.max(1) as f64).sqrt()
}

fn recency_factor(item: &str, recent: &[String]) -> f64 {
    if recent.last().map(String::as_str) == Some(item) {
        REPEAT_LAST
    } else if recent.len() >= 2 && recent[recent.len() - 2] == item {
        REPEAT_OLDER
    } else {
        1.0
    }
}

fn diversity_factor(item: &str, used: &HashMap<String, u32>, picked: u32) -> f64 {
    if picked < 3 {
        return 1.0;
    }
    let share = f64::from(*used.get(item).unwrap_or(&0)) / f64::from(picked);
    if share >= SHARE_HIGH {
        SHARE_HIGH_WEIGHT
    } else if share >= SHARE_MID {
        SHARE_MID_WEIGHT
    } else {
        1.0
    }
}

/// The mistyped letter plus the three letters before it. Spaces and punctuation are skipped.
fn syllable_at(text: &str, index: usize) -> Option<String> {
    let chars: Vec<char> = text.chars().collect();
    if index >= chars.len() || !chars[index].is_alphabetic() {
        return None;
    }
    let mut prev = Vec::new();
    let mut cursor = index as isize - 1;
    while cursor >= 0 && prev.len() < 3 {
        let ch = chars[cursor as usize];
        if ch.is_alphabetic() {
            prev.push(ch);
        }
        cursor -= 1;
    }
    if prev.len() < 3 {
        return None;
    }
    prev.reverse();
    prev.push(chars[index]);
    Some(prev.into_iter().collect())
}

fn word_span_at(text: &str, index: usize) -> Option<(String, usize)> {
    let byte = text.char_indices().nth(index).map(|(byte, _)| byte)?;
    for hit in WORD_RE.find_iter(text) {
        if hit.start() <= byte && byte < hit.end() {
            let start = text[..hit.start()].chars().count();
            return Some((hit.as_str().to_string(), start));
        }
    }
    None
}

fn sorted(map: &Counts) -> Vec<(String, u64)> {
    let mut rows: Vec<_> = map
        .iter()
        .map(|(key, count)| (key.clone(), *count))
        .collect();
    rows.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    rows
}

fn key_sets(bucket: &ByLang<Counts>) -> ByLang<HashSet<String>> {
    ByLang {
        en: bucket.en.keys().cloned().collect(),
        ru: bucket.ru.keys().cloned().collect(),
    }
}

fn trim_to_cap(counts: &mut Counts, old: &HashSet<String>) {
    let mut extra = counts.len().saturating_sub(MAX_TRACKED);
    if extra == 0 {
        return;
    }
    let mut old_present: Vec<String> = counts
        .keys()
        .filter(|key| old.contains(*key))
        .cloned()
        .collect();
    old_present.shuffle(&mut rand::thread_rng());
    for key in old_present.into_iter().take(extra) {
        counts.remove(&key);
    }
    extra = counts.len().saturating_sub(MAX_TRACKED);
    if extra == 0 {
        return;
    }
    let mut rest: Vec<String> = counts.keys().cloned().collect();
    rest.shuffle(&mut rand::thread_rng());
    for key in rest.into_iter().take(extra) {
        counts.remove(&key);
    }
}

fn split_files_exist(directory: &Path) -> bool {
    KIND_FILES
        .iter()
        .any(|kind| directory.join(format!("mistakes-{kind}.json")).exists())
}

fn read_json(path: &Path) -> Option<Value> {
    let raw = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&raw).ok()
}

fn read_kind_file(path: &Path) -> ByLang<Counts> {
    let Some(data) = read_json(path) else {
        return ByLang::default();
    };
    if KIND_FILES.iter().any(|kind| data.get(kind).is_some()) {
        return ByLang::default();
    }
    let mut bucket: ByLang<Counts> = ByLang::default();
    for lang in [Lang::Ru, Lang::En] {
        let Some(obj) = data.get(lang.as_str()).and_then(Value::as_object) else {
            continue;
        };
        for (key, value) in obj {
            if let Some(count) = count_value(value) {
                if !key.is_empty() {
                    bucket[lang].insert(key.clone(), count);
                }
            }
        }
    }
    bucket
}

fn count_value(value: &Value) -> Option<u64> {
    value
        .as_u64()
        .or_else(|| value.as_i64().map(|n| n.max(0) as u64))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_item_is_stable() {
        let ranked = vec![("house".into(), 4)];
        assert_eq!(pick_weighted(&ranked, &[], &HashMap::new()), "house");
    }

    #[test]
    fn syllable_skips_spaces() {
        assert_eq!(syllable_at("привет админ", 7), Some("вета".into()));
    }
}
