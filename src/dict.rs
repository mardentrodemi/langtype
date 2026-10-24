use std::fs;
use std::path::{Path, PathBuf};

use crate::model::Lang;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Pair {
    pub en: String,
    pub ru: String,
}

impl Pair {
    pub fn new(en: impl Into<String>, ru: impl Into<String>) -> Self {
        Self {
            en: en.into(),
            ru: ru.into(),
        }
    }

    pub fn text(&self, lang: Lang) -> &str {
        match lang {
            Lang::En => &self.en,
            Lang::Ru => &self.ru,
        }
    }
}

#[derive(Debug)]
pub struct LoadError {
    what: &'static str,
    dir: PathBuf,
}

impl std::fmt::Display for LoadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Empty {} dictionary in {}",
            self.what,
            self.dir.display()
        )
    }
}

impl std::error::Error for LoadError {}

pub fn dictionary_dir() -> PathBuf {
    program_data_dir().unwrap_or_else(|| PathBuf::from("data"))
}

fn program_data_dir() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let exe = fs::canonicalize(&exe).unwrap_or(exe);
    let mut dir = exe.parent()?.to_path_buf();
    for _ in 0..6 {
        let data = dir.join("data");
        if data.join("en.txt").is_file() {
            return Some(data);
        }
        dir = dir.parent()?.to_path_buf();
    }
    None
}

fn load_aligned(
    dir: &Path,
    en_file: &str,
    ru_file: &str,
    min_len: usize,
    max_len: usize,
) -> Vec<Pair> {
    let en = fs::read_to_string(dir.join(en_file)).unwrap_or_default();
    let ru = fs::read_to_string(dir.join(ru_file)).unwrap_or_default();
    en.lines()
        .zip(ru.lines())
        .filter_map(|(en_raw, ru_raw)| {
            let en_line = normalize(en_raw);
            let ru_line = normalize(ru_raw);
            let en_len = en_line.chars().count();
            let ru_len = ru_line.chars().count();
            let fits =
                (min_len..=max_len).contains(&en_len) && (min_len..=max_len).contains(&ru_len);
            fits.then(|| Pair::new(en_line, ru_line))
        })
        .collect()
}

fn normalize(raw: &str) -> String {
    raw.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub fn load_sentences(dir: &Path) -> Result<Vec<Pair>, LoadError> {
    let pairs = load_aligned(dir, "en.txt", "ru.txt", 40, 180)
        .into_iter()
        .filter(|pair| !is_tatoeba_meta(&pair.en) && !is_tatoeba_meta(&pair.ru))
        .collect::<Vec<_>>();
    if pairs.is_empty() {
        Err(LoadError {
            what: "sentence",
            dir: dir.to_path_buf(),
        })
    } else {
        Ok(pairs)
    }
}

pub fn load_words(dir: &Path) -> Result<Vec<Pair>, LoadError> {
    let pairs = load_aligned(dir, "words_en.txt", "words_ru.txt", 1, 24);
    if pairs.is_empty() {
        Err(LoadError {
            what: "word",
            dir: dir.to_path_buf(),
        })
    } else {
        Ok(pairs)
    }
}

fn is_tatoeba_meta(text: &str) -> bool {
    let lower = text.to_lowercase();
    lower.contains("tatoeba") || lower.contains("татоэб")
}
