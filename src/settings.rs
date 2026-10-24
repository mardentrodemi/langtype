use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::i18n::UiLang;
use crate::model::{Lang, SpeedUnit};
use crate::paths::{self, user_dir};

#[derive(Clone, Debug)]
pub struct Settings {
    pub speed_unit: SpeedUnit,
    pub first_lang: Lang,
    pub hide_punctuation: bool,
    pub no_capitals: bool,
    pub in_line_rows: u32,
    pub versus_words: u32,
    pub versus_repeat: bool,
    pub ui_lang: UiLang,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            speed_unit: SpeedUnit::Wpm,
            first_lang: Lang::En,
            hide_punctuation: false,
            no_capitals: false,
            in_line_rows: 1,
            versus_words: 1,
            versus_repeat: false,
            ui_lang: UiLang::En,
        }
    }
}

impl Settings {
    pub const LINE_ROWS: [u32; 4] = [1, 3, 6, 9];
    pub const WORDS_PER_LINE: [u32; 3] = [1, 3, 5];

    pub fn load(path: &Path) -> Self {
        let Ok(raw) = std::fs::read_to_string(path) else {
            return Self::default();
        };
        let Ok(file) = serde_json::from_str::<FileSettings>(&raw) else {
            return Self::default();
        };
        Self {
            speed_unit: SpeedUnit::parse(&file.speed_unit),
            first_lang: Lang::parse(&file.first_lang).unwrap_or(Lang::En),
            hide_punctuation: file.skip_dot_comma,
            no_capitals: file.no_capitals,
            in_line_rows: allowed(file.in_line_rows, &Self::LINE_ROWS, 1),
            versus_words: allowed(file.versus_words, &Self::WORDS_PER_LINE, 1),
            versus_repeat: file.versus_repeat,
            ui_lang: UiLang::parse(&file.ui_lang),
        }
    }

    pub fn save(&self, path: &Path) {
        paths::write_json(
            path,
            &FileSettings {
                speed_unit: self.speed_unit.as_str().to_string(),
                first_lang: self.first_lang.as_str().to_string(),
                skip_dot_comma: self.hide_punctuation,
                no_capitals: self.no_capitals,
                in_line_rows: self.in_line_rows,
                versus_words: self.versus_words,
                versus_repeat: self.versus_repeat,
                ui_lang: self.ui_lang.as_str().to_string(),
            },
        );
    }

    pub fn unit_label(&self) -> &'static str {
        let ui = self.ui_lang;
        match self.speed_unit {
            SpeedUnit::Cpm => ui.say("Characters per minute (CPM)", "Знаков в минуту (CPM)"),
            SpeedUnit::Wpm => ui.say("Words per minute (WPM)", "Слов в минуту (WPM)"),
        }
    }

    pub fn order_label(&self) -> &'static str {
        let ui = self.ui_lang;
        match self.first_lang {
            Lang::Ru => ui.say("Left RU, right EN", "Слева RU, справа EN"),
            Lang::En => ui.say("Left EN, right RU", "Слева EN, справа RU"),
        }
    }

    pub fn language_label(&self) -> String {
        format!(
            "{}: {}",
            self.ui_lang.say("Language", "Язык"),
            self.ui_lang.own_name()
        )
    }

    pub fn punct_label(&self) -> String {
        switch_label(
            self.ui_lang,
            self.ui_lang.say("Hide punctuation", "Скрыть пунктуацию"),
            self.hide_punctuation,
        )
    }

    pub fn capitals_label(&self) -> String {
        switch_label(
            self.ui_lang,
            self.ui_lang.say("No capitals", "Без заглавных"),
            self.no_capitals,
        )
    }

    pub fn line_rows_label(&self) -> String {
        format!(
            "{}: {}",
            self.ui_lang.say("In-a-line rows", "Строк подряд"),
            self.in_line_rows
        )
    }

    pub fn versus_words_label(&self) -> String {
        format!(
            "{}: {}",
            self.ui_lang.say("Words per line", "Слов в строке"),
            self.versus_words
        )
    }

    pub fn versus_repeat_label(&self) -> String {
        switch_label(
            self.ui_lang,
            self.ui_lang.say("Repeat one word", "Повторять одно слово"),
            self.versus_repeat,
        )
    }
}

pub fn settings_path() -> PathBuf {
    user_dir().join("settings.json")
}

fn switch_label(ui: UiLang, name: &str, on: bool) -> String {
    format!("{name} : {}", ui.on_off(on))
}

fn allowed(value: u32, options: &[u32], fallback: u32) -> u32 {
    if options.contains(&value) {
        value
    } else {
        fallback
    }
}

/// On-disk shape. `skip_dot_comma` is the historical name of "hide punctuation".
#[derive(Serialize, Deserialize)]
struct FileSettings {
    #[serde(default = "default_unit")]
    speed_unit: String,
    #[serde(default = "default_lang")]
    first_lang: String,
    #[serde(default)]
    skip_dot_comma: bool,
    #[serde(default)]
    no_capitals: bool,
    #[serde(default = "default_rows")]
    in_line_rows: u32,
    #[serde(default = "default_words")]
    versus_words: u32,
    #[serde(default)]
    versus_repeat: bool,
    #[serde(default = "default_ui")]
    ui_lang: String,
}

fn default_unit() -> String {
    "wpm".into()
}

fn default_lang() -> String {
    "en".into()
}

fn default_rows() -> u32 {
    1
}

fn default_words() -> u32 {
    1
}

fn default_ui() -> String {
    "en".into()
}
