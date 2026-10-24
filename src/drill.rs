use std::collections::HashMap;
use std::time::Instant;

use crate::game::Side;
use crate::i18n::UiLang;
use crate::mistakes::{
    build_repeat_line, build_weighted_passage, pick_weighted, MistakeBank, MistakeKind,
};
use crate::model::Lang;
use crate::text::prepare_line;

/// What a drill line is built from. Borrowed from the running app for one keystroke.
pub struct LineSource<'a> {
    pub bank: &'a MistakeBank,
    pub no_capitals: bool,
    pub line_rows: u32,
    pub width: usize,
    pub ui: UiLang,
}

pub struct Drill {
    pub lang: Lang,
    pub kind: MistakeKind,
    pub in_line: bool,
    pub rows: Vec<Side>,
    pub layout_ok: bool,
    started: Option<Instant>,
    last_repeat: String,
    repeats_left: i32,
}

impl Drill {
    pub fn begin(
        lang: Lang,
        kind: MistakeKind,
        in_line: bool,
        layout_ok: bool,
        source: &LineSource<'_>,
    ) -> Result<Self, String> {
        if source.bank.items(lang, kind).is_empty() {
            return Err(empty_message(lang, kind, source.ui));
        }
        let mut drill = Self {
            lang,
            kind,
            in_line,
            rows: Vec::new(),
            layout_ok,
            started: None,
            last_repeat: String::new(),
            repeats_left: 0,
        };
        let target = drill.next_target(source);
        drill.rows.push(Side::new(target));
        Ok(drill)
    }

    pub fn elapsed(&self) -> f64 {
        self.started
            .map(|t| t.elapsed().as_secs_f64())
            .unwrap_or(0.0)
    }

    pub fn backspace(&mut self) {
        if let Some(side) = self.rows.last_mut() {
            side.backspace();
        }
    }

    pub fn type_char(&mut self, ch: char, source: &LineSource<'_>) {
        let accepted = self
            .rows
            .last_mut()
            .and_then(|side| side.type_char(ch))
            .is_some();
        if !accepted {
            return;
        }
        if self.started.is_none() {
            self.started = Some(Instant::now());
        }
        if self.rows.last().is_some_and(Side::done) {
            let target = self.next_target(source);
            if !target.is_empty() {
                self.rows.push(Side::new(target));
            }
        }
    }

    fn next_target(&mut self, source: &LineSource<'_>) -> String {
        let ranked = pool(source.bank, self.lang, self.kind, source.no_capitals);
        let text = if self.in_line {
            if self.repeats_left <= 0 || self.last_repeat.is_empty() {
                let avoid = if self.last_repeat.is_empty() {
                    &[][..]
                } else {
                    std::slice::from_ref(&self.last_repeat)
                };
                self.last_repeat = pick_weighted(&ranked, avoid, &HashMap::new());
                self.repeats_left = source.line_rows.max(1) as i32;
            }
            self.repeats_left -= 1;
            build_repeat_line(&self.last_repeat, source.width)
        } else {
            build_weighted_passage(&ranked, source.width)
        };
        prepare_line(&text, false, source.no_capitals)
    }
}

fn pool(
    bank: &MistakeBank,
    lang: Lang,
    kind: MistakeKind,
    no_capitals: bool,
) -> Vec<(String, u64)> {
    let ranked = bank.ranked(lang, kind);
    if !no_capitals {
        return ranked;
    }
    let mut merged: HashMap<String, u64> = HashMap::new();
    for (item, count) in ranked {
        *merged.entry(item.to_lowercase()).or_default() += count;
    }
    let mut out: Vec<_> = merged.into_iter().collect();
    out.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    out
}

fn empty_message(lang: Lang, kind: MistakeKind, ui: UiLang) -> String {
    let what = match (ui, kind) {
        (UiLang::En, MistakeKind::Letters) => "letter",
        (UiLang::En, MistakeKind::Words) => "word",
        (UiLang::En, MistakeKind::Syllables) => "syllable",
        (UiLang::Ru, MistakeKind::Letters) => "буквах",
        (UiLang::Ru, MistakeKind::Words) => "словах",
        (UiLang::Ru, MistakeKind::Syllables) => "слогах",
    };
    match ui {
        UiLang::En => format!("No {what} mistakes for {} yet", ui.practice(lang)),
        UiLang::Ru => format!("Пока нет ошибок в {what}: {}", ui.practice(lang)),
    }
}
