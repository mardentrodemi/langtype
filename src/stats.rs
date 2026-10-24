use std::path::{Path, PathBuf};

use serde::Serialize;
use serde_json::Value;

use crate::i18n::UiLang;
use crate::model::{ByLang, Lang, SpeedUnit};
use crate::paths::{self, user_dir};

#[derive(Clone, Debug, Default)]
pub struct Counters {
    pub keystrokes: u64,
    pub correct: u64,
    pub seconds: f64,
    pub sides: u64,
}

impl Counters {
    fn merged(&self, other: &Counters) -> Self {
        Self {
            keystrokes: self.keystrokes + other.keystrokes,
            correct: self.correct + other.correct,
            seconds: self.seconds + other.seconds,
            sides: self.sides + other.sides,
        }
    }

    pub fn net(&self, unit: SpeedUnit) -> f64 {
        if self.seconds <= 0.0 {
            return 0.0;
        }
        let minutes = self.seconds.max(1e-6) / 60.0;
        let chars = match unit {
            SpeedUnit::Cpm => self.correct as f64,
            SpeedUnit::Wpm => self.correct as f64 / 5.0,
        };
        chars / minutes
    }

    pub fn accuracy(&self) -> f64 {
        if self.keystrokes == 0 {
            100.0
        } else {
            100.0 * self.correct as f64 / self.keystrokes as f64
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct DualStats {
    langs: ByLang<Counters>,
    pub pairs: u64,
    pub sessions: u64,
    pub practice_seconds: f64,
}

impl DualStats {
    pub fn for_lang(&self, lang: Lang) -> &Counters {
        &self.langs[lang]
    }

    pub fn plus_side(&self, lang: Lang, piece: Counters, complete_pair: bool) -> Self {
        let mut out = self.clone();
        out.langs[lang] = self.langs[lang].merged(&piece);
        if complete_pair {
            out.pairs += 1;
        }
        out
    }

    pub fn accuracy(&self) -> f64 {
        let keys = self.langs.en.keystrokes + self.langs.ru.keystrokes;
        let correct = self.langs.en.correct + self.langs.ru.correct;
        if keys == 0 {
            100.0
        } else {
            100.0 * correct as f64 / keys as f64
        }
    }

    pub fn total_seconds(&self) -> f64 {
        self.langs.en.seconds + self.langs.ru.seconds + self.practice_seconds
    }

    pub fn load(path: &Path) -> Self {
        let Ok(raw) = std::fs::read_to_string(path) else {
            return Self::default();
        };
        let Ok(data) = serde_json::from_str::<Value>(&raw) else {
            return Self::default();
        };
        Self {
            langs: ByLang::new(
                counters_from(data.get("en").unwrap_or(&Value::Null)),
                counters_from(data.get("ru").unwrap_or(&Value::Null)),
            ),
            pairs: u64_field(&data, "pairs"),
            sessions: u64_field(&data, "sessions"),
            practice_seconds: f64_field(&data, "practice_seconds"),
        }
    }

    pub fn save(&self, path: &Path) {
        paths::write_json(
            path,
            &LifetimeFile {
                pairs: self.pairs,
                sessions: self.sessions,
                practice_seconds: round_millis(self.practice_seconds),
                en: CountersFile::from(&self.langs.en),
                ru: CountersFile::from(&self.langs.ru),
            },
        );
    }

    pub fn reset(path: &Path) -> Self {
        let empty = Self::default();
        empty.save(path);
        empty
    }
}

pub fn lifetime_path() -> PathBuf {
    user_dir().join("lifetime.json")
}

pub fn format_duration(seconds: f64) -> String {
    let total = seconds.max(0.0) as u64;
    let hours = total / 3600;
    let minutes = (total % 3600) / 60;
    let secs = total % 60;
    if hours > 0 {
        format!("{hours}:{minutes:02}:{secs:02}")
    } else {
        format!("{minutes}:{secs:02}")
    }
}

pub fn speed_line(stats: &DualStats, unit: SpeedUnit, first: Lang, ui: UiLang) -> String {
    let second = first.other();
    format!(
        "{} {}  ·  {} {} {:.0}  {} {:.0}  ·  {} {:.0}%  ·  {} {}",
        ui.say("Time", "Время"),
        format_duration(stats.total_seconds()),
        unit.tag(),
        first.label(),
        stats.for_lang(first).net(unit),
        second.label(),
        stats.for_lang(second).net(unit),
        ui.say("Accuracy", "Точность"),
        stats.accuracy(),
        ui.say("Pairs", "Пары"),
        stats.pairs
    )
}

fn round_millis(seconds: f64) -> f64 {
    (seconds * 1000.0).round() / 1000.0
}

fn u64_field(value: &Value, key: &str) -> u64 {
    value.get(key).and_then(Value::as_u64).unwrap_or(0)
}

fn f64_field(value: &Value, key: &str) -> f64 {
    value.get(key).and_then(Value::as_f64).unwrap_or(0.0)
}

fn counters_from(value: &Value) -> Counters {
    Counters {
        keystrokes: u64_field(value, "keystrokes"),
        correct: u64_field(value, "correct"),
        seconds: f64_field(value, "seconds"),
        sides: u64_field(value, "sides"),
    }
}

#[derive(Serialize)]
struct LifetimeFile {
    pairs: u64,
    sessions: u64,
    practice_seconds: f64,
    en: CountersFile,
    ru: CountersFile,
}

#[derive(Serialize)]
struct CountersFile {
    keystrokes: u64,
    correct: u64,
    seconds: f64,
    sides: u64,
}

impl From<&Counters> for CountersFile {
    fn from(counters: &Counters) -> Self {
        Self {
            keystrokes: counters.keystrokes,
            correct: counters.correct,
            seconds: round_millis(counters.seconds),
            sides: counters.sides,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn speed_line_names_both_languages_in_settings_order() {
        let stats = DualStats::default()
            .plus_side(
                Lang::En,
                Counters {
                    keystrokes: 100,
                    correct: 90,
                    seconds: 22.2,
                    sides: 1,
                },
                false,
            )
            .plus_side(
                Lang::Ru,
                Counters {
                    keystrokes: 50,
                    correct: 40,
                    sides: 1,
                    seconds: 9.5,
                },
                true,
            );
        assert_eq!(
            speed_line(&stats, SpeedUnit::Cpm, Lang::En, UiLang::En),
            "Time 0:31  ·  CPM EN 243  RU 253  ·  Accuracy 87%  ·  Pairs 1"
        );
        assert_eq!(
            speed_line(&stats, SpeedUnit::Wpm, Lang::Ru, UiLang::En),
            "Time 0:31  ·  WPM RU 51  EN 49  ·  Accuracy 87%  ·  Pairs 1"
        );
    }
}
