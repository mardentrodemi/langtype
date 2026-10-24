//! FB2 books from the data folder, typed one paragraph at a time.
//! The current paragraph of each book is stored in `books.json`.

use std::collections::{BTreeMap, HashMap};
use std::fs::{self, File};
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

use quick_xml::events::Event;
use quick_xml::name::QName;
use quick_xml::Reader;
use serde::{Deserialize, Serialize};
use zip::ZipArchive;

use crate::game::{Commit, Side};
use crate::i18n::UiLang;
use crate::mistakes::MistakeBank;
use crate::model::{Lang, SpeedUnit};
use crate::paths::{self, user_dir};
use crate::stats::{Counters, DualStats};
use crate::text::{chars_match, prepare_line, SKIP_PUNCT};

const READ_ERROR: &str = "Could not read this book";

pub fn empty_hint(ui: UiLang) -> &'static str {
    ui.say(
        "Put your book in fb2 format in the data folder",
        "Положите книгу в формате fb2 в папку data",
    )
}
const HISTORY_LIMIT: usize = 4;
const PREVIEW_LINES: usize = 2;
/// A typing line. Longer FB2 paragraphs are split on spaces.
const LINE_CHARS: usize = 100;

#[derive(Clone, Debug)]
pub struct BookEntry {
    pub key: String,
    pub title: String,
    pub path: PathBuf,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BookPlace {
    pub line: u32,
    pub total: u32,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct BookProgress {
    #[serde(default)]
    books: BTreeMap<String, BookPlace>,
}

pub struct TypeOpts {
    pub hide_punctuation: bool,
    pub no_capitals: bool,
    pub speed_unit: SpeedUnit,
    pub ui: UiLang,
}

pub struct BookSession {
    key: String,
    title: String,
    lines: Vec<String>,
    index: usize,
    history: Vec<Side>,
    current: Option<Side>,
    lang: Lang,
    session: DualStats,
    speed_unit: SpeedUnit,
    hide_punctuation: bool,
    no_capitals: bool,
    /// Opened a finished book. Leaving without a keystroke keeps "finished".
    started_finished: bool,
    typed_any: bool,
    pub layout_ok: bool,
}

impl BookProgress {
    pub fn path() -> PathBuf {
        user_dir().join("books.json")
    }

    pub fn load(path: &Path) -> Self {
        let Ok(text) = fs::read_to_string(path) else {
            return Self::default();
        };
        serde_json::from_str(&text).unwrap_or_default()
    }

    pub fn place(&self, key: &str) -> Option<BookPlace> {
        self.books.get(key).copied()
    }

    pub fn remember(&mut self, key: impl Into<String>, line: u32, total: u32) {
        self.books.insert(key.into(), BookPlace { line, total });
    }

    pub fn set(&mut self, path: &Path, key: &str, line: u32, total: u32) {
        self.remember(key, line, total);
        paths::write_json(path, self);
    }

    pub fn menu_label(&self, entry: &BookEntry, ui: UiLang) -> String {
        let title = entry.title.as_str();
        let line = ui.say("line", "строка");
        let finished = ui.say("finished", "закончена");
        match self.place(&entry.key) {
            Some(place) if place.total > 0 && place.line >= place.total => {
                format!("{title}  ·  {finished}")
            }
            Some(place) if place.total > 0 => {
                format!(
                    "{title}  ·  {line} {} / {}",
                    place.line.saturating_add(1),
                    place.total
                )
            }
            Some(place) if place.line > 0 => {
                format!("{title}  ·  {line} {}", place.line.saturating_add(1))
            }
            _ => title.to_string(),
        }
    }
}

impl BookSession {
    pub fn start(
        entry: &BookEntry,
        lines: Vec<String>,
        place: Option<BookPlace>,
        opts: TypeOpts,
    ) -> Result<Self, &'static str> {
        if lines.is_empty()
            || lines
                .iter()
                .all(|line| prepare_line(line, opts.hide_punctuation, opts.no_capitals).is_empty())
        {
            return Err(opts.ui.say("This book has no text", "В книге нет текста"));
        }
        let (index, started_finished) = opening_index(place, lines.len());
        let mut session = Self {
            key: entry.key.clone(),
            title: entry.title.clone(),
            lines,
            index,
            history: Vec::new(),
            current: None,
            lang: Lang::Ru,
            session: DualStats::default(),
            speed_unit: opts.speed_unit,
            hide_punctuation: opts.hide_punctuation,
            no_capitals: opts.no_capitals,
            started_finished,
            typed_any: false,
            layout_ok: false,
        };
        session.lang = guess_lang(&session.lines);
        session.sync();
        Ok(session)
    }

    pub fn key(&self) -> &str {
        &self.key
    }

    pub fn title(&self) -> &str {
        &self.title
    }

    pub fn lang(&self) -> Lang {
        self.lang
    }

    pub fn speed_unit(&self) -> SpeedUnit {
        self.speed_unit
    }

    pub fn history(&self) -> &[Side] {
        &self.history
    }

    pub fn current(&self) -> Option<&Side> {
        self.current.as_ref()
    }

    pub fn live(&self) -> DualStats {
        match &self.current {
            Some(side) if side.has_progress() => {
                self.session.plus_side(self.lang, side.pending(), false)
            }
            _ => self.session.clone(),
        }
    }

    pub fn progress_label(&self, ui: UiLang) -> String {
        let total = self.lines.len();
        if self.index >= total {
            ui.say("finished", "закончена").into()
        } else {
            format!("{} {} / {total}", ui.say("line", "строка"), self.index + 1)
        }
    }

    pub fn preview_texts(&self) -> Vec<String> {
        let mut out = Vec::new();
        for line in self.lines.iter().skip(self.index + 1) {
            let text = prepare_line(line, self.hide_punctuation, self.no_capitals);
            if text.is_empty() {
                continue;
            }
            out.push(text);
            if out.len() == PREVIEW_LINES {
                break;
            }
        }
        out
    }

    /// Paragraph to resume, and how many paragraphs the book has.
    pub fn checkpoint(&self) -> (u32, u32) {
        let total = u32::try_from(self.lines.len()).unwrap_or(u32::MAX);
        if self.started_finished && !self.typed_any {
            return (total, total);
        }
        let line = u32::try_from(self.index).unwrap_or(u32::MAX).min(total);
        (line, total)
    }

    pub fn backspace(&mut self) {
        if let Some(side) = self.current.as_mut() {
            side.backspace();
        }
    }

    pub fn on_char(&mut self, ch: char, bank: &mut MistakeBank) -> Option<Commit> {
        if self.hide_punctuation && SKIP_PUNCT.contains(ch) {
            return None;
        }
        self.sync();
        let lang = self.lang;
        let Some(side) = self.current.as_mut() else {
            return None;
        };
        let Some(pos) = side.type_char(ch) else {
            return None;
        };
        self.typed_any = true;
        let mismatch = side
            .target()
            .chars()
            .nth(pos)
            .is_some_and(|expected| !chars_match(ch, expected));
        if mismatch {
            let target = side.target().to_string();
            bank.note_typo(lang, &target, pos, ch, side.word_flag());
        }
        let finished_line = self.current.as_ref().is_some_and(Side::done);
        if !finished_line {
            return None;
        }
        let commit = self.commit_current();
        self.index += 1;
        self.sync();
        commit
    }

    /// Keeps the current paragraph for the next visit and records what was typed.
    pub fn finish(&mut self) -> Option<Commit> {
        let side = self.current.as_mut()?;
        if !side.has_progress() {
            return None;
        }
        let counters = side.take_commit()?;
        self.fold(counters)
    }

    fn commit_current(&mut self) -> Option<Commit> {
        let mut side = self.current.take()?;
        let counters = side.take_commit()?;
        self.push_history(side);
        self.fold(counters)
    }

    fn fold(&mut self, counters: Counters) -> Option<Commit> {
        self.session = self.session.plus_side(self.lang, counters.clone(), false);
        Some(Commit {
            lang: self.lang,
            counters,
            complete_pair: false,
        })
    }

    fn push_history(&mut self, side: Side) {
        self.history.push(side);
        if self.history.len() > HISTORY_LIMIT {
            self.history.remove(0);
        }
    }

    fn sync(&mut self) {
        if self.current.is_some() {
            return;
        }
        while self.index < self.lines.len() {
            let text = prepare_line(
                &self.lines[self.index],
                self.hide_punctuation,
                self.no_capitals,
            );
            if text.is_empty() {
                self.index += 1;
                continue;
            }
            self.current = Some(Side::new(text));
            return;
        }
    }
}

pub fn list(dir: &Path) -> Vec<BookEntry> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut books = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() || !is_book_file(&path) {
            continue;
        }
        let Some(key) = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
        else {
            continue;
        };
        if key.is_empty() || key.starts_with('.') {
            continue;
        }
        let title = parse_path(&path, true)
            .ok()
            .map(|parsed| parsed.title)
            .filter(|title| !title.is_empty())
            .unwrap_or_else(|| stem_title(&path));
        books.push(BookEntry { key, title, path });
    }
    disambiguate(&mut books);
    books.sort_by(|a, b| fold_title(&a.title).cmp(&fold_title(&b.title)));
    books
}

pub fn load_lines(path: &Path, ui: UiLang) -> Result<Vec<String>, String> {
    parse_path(path, false)
        .map(|parsed| parsed.lines)
        .map_err(|err| translate_read_error(err, ui))
}

fn translate_read_error(err: String, ui: UiLang) -> String {
    match ui {
        UiLang::En => err,
        UiLang::Ru => err.replacen(READ_ERROR, "Не удалось прочитать книгу", 1),
    }
}

fn opening_index(place: Option<BookPlace>, len: usize) -> (usize, bool) {
    let Some(place) = place else {
        return (0, false);
    };
    let line = place.line as usize;
    if len > 0 && line >= len {
        (0, true)
    } else {
        (line.min(len), false)
    }
}

fn guess_lang(lines: &[String]) -> Lang {
    let mut cyr = 0usize;
    let mut lat = 0usize;
    for line in lines.iter().take(80) {
        for ch in line.chars() {
            if ('\u{0400}'..='\u{04FF}').contains(&ch) {
                cyr += 1;
            } else if ch.is_ascii_alphabetic() {
                lat += 1;
            }
        }
    }
    if lat > cyr {
        Lang::En
    } else {
        Lang::Ru
    }
}

fn fold_title(title: &str) -> String {
    title.to_lowercase()
}

fn clip_chars(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    let mut out: String = text.chars().take(max.saturating_sub(1)).collect();
    out.push('…');
    out
}

fn disambiguate(books: &mut [BookEntry]) {
    let mut counts: HashMap<String, usize> = HashMap::new();
    for book in books.iter() {
        *counts.entry(book.title.clone()).or_insert(0) += 1;
    }
    for book in books.iter_mut() {
        if counts.get(&book.title).copied().unwrap_or(0) > 1 {
            let key = book.key.clone();
            book.title = format!("{} ({key})", book.title);
        }
    }
}

fn is_book_file(path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
        return false;
    };
    let name = name.to_ascii_lowercase();
    name.ends_with(".fb2") || name.ends_with(".fb2.zip")
}

fn stem_title(path: &Path) -> String {
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("book");
    let lower = name.to_ascii_lowercase();
    let stem = if let Some(stripped) = lower.strip_suffix(".fb2.zip") {
        &name[..stripped.len()]
    } else if let Some(stripped) = lower.strip_suffix(".fb2") {
        &name[..stripped.len()]
    } else {
        name
    };
    if stem.is_empty() {
        "book".into()
    } else {
        stem.to_string()
    }
}

struct Parsed {
    title: String,
    lines: Vec<String>,
}

fn parse_path(path: &Path, titles_only: bool) -> Result<Parsed, String> {
    if is_zip(path) {
        let file = File::open(path).map_err(|err| read_error(err))?;
        let mut archive = ZipArchive::new(file).map_err(|err| read_error(err))?;
        let index = find_fb2(&mut archive)?;
        let entry = archive.by_index(index).map_err(|err| read_error(err))?;
        parse_fb2(BufReader::new(entry), titles_only)
    } else {
        let file = File::open(path).map_err(|err| read_error(err))?;
        parse_fb2(BufReader::new(file), titles_only)
    }
}

fn is_zip(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.to_ascii_lowercase().ends_with(".fb2.zip"))
}

fn find_fb2<R: std::io::Read + std::io::Seek>(
    archive: &mut ZipArchive<R>,
) -> Result<usize, String> {
    let mut found = None;
    for index in 0..archive.len() {
        let file = archive.by_index(index).map_err(|err| read_error(err))?;
        let name = file.name().to_ascii_lowercase();
        let is_fb2 = name.ends_with(".fb2") && !name.ends_with('/');
        drop(file);
        if is_fb2 {
            found = Some(index);
            break;
        }
    }
    found.ok_or_else(|| READ_ERROR.to_string())
}

fn read_error(err: impl std::fmt::Display) -> String {
    let detail = clip_chars(&err.to_string(), 48);
    format!("{READ_ERROR}: {detail}")
}

fn parse_fb2(reader: impl BufRead, titles_only: bool) -> Result<Parsed, String> {
    let mut reader = Reader::from_reader(reader);
    reader.config_mut().trim_text(false);
    let mut state = ParseState::default();
    let mut buf = Vec::new();
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Eof) => break,
            Err(err) => return Err(read_error(err)),
            Ok(event) => {
                let stop = state.push(event, titles_only);
                buf.clear();
                if stop {
                    break;
                }
            }
        }
    }
    Ok(Parsed {
        title: state.title,
        lines: state.lines,
    })
}

#[derive(Default)]
struct ParseState {
    title: String,
    lines: Vec<String>,
    /// Innermost body is notes/comments when the flag is true.
    bodies: Vec<bool>,
    capture: Option<Capture>,
}

struct Capture {
    tag: String,
    title: bool,
    depth: u32,
    buf: String,
}

impl ParseState {
    fn in_main_body(&self) -> bool {
        self.bodies.last().copied() == Some(false)
    }

    /// Returns true when a title-only scan can stop.
    fn push(&mut self, event: Event<'_>, titles_only: bool) -> bool {
        match event {
            Event::Start(start) => self.on_start(&start, false, titles_only),
            Event::Empty(start) => self.on_start(&start, true, titles_only),
            Event::End(end) => {
                let name = tag_name(end.name());
                if name == "body" {
                    self.bodies.pop();
                    return false;
                }
                self.close_tag(&name, titles_only)
            }
            Event::Text(text) => {
                if let Some(capture) = &mut self.capture {
                    if let Ok(decoded) = text.unescape() {
                        capture.buf.push_str(&decoded);
                    }
                }
                false
            }
            Event::CData(text) => {
                if let Some(capture) = &mut self.capture {
                    capture
                        .buf
                        .push_str(&String::from_utf8_lossy(text.as_ref()));
                }
                false
            }
            _ => false,
        }
    }

    fn on_start(
        &mut self,
        start: &quick_xml::events::BytesStart<'_>,
        empty: bool,
        titles_only: bool,
    ) -> bool {
        let name = tag_name(start.name());
        if let Some(capture) = &mut self.capture {
            if capture.tag == name {
                capture.depth += 1;
            }
            return false;
        }
        if name == "body" {
            if !empty {
                self.bodies.push(body_is_notes(start));
            }
            return false;
        }
        if name == "book-title" && self.title.is_empty() && !empty {
            self.capture = Some(Capture {
                tag: name,
                title: true,
                depth: 1,
                buf: String::new(),
            });
            return false;
        }
        if self.in_main_body() && is_line_tag(&name) && !empty {
            self.capture = Some(Capture {
                tag: name,
                title: false,
                depth: 1,
                buf: String::new(),
            });
        }
        let _ = titles_only;
        false
    }

    fn close_tag(&mut self, name: &str, titles_only: bool) -> bool {
        let closing = self
            .capture
            .as_ref()
            .is_some_and(|capture| capture.tag == name);
        if closing {
            let depth = {
                let capture = self.capture.as_mut().expect("capture");
                capture.depth = capture.depth.saturating_sub(1);
                capture.depth
            };
            if depth == 0 {
                let capture = self.capture.take().expect("capture");
                if capture.title {
                    let title = normalize(&capture.buf);
                    if !title.is_empty() {
                        self.title = title;
                    }
                } else {
                    self.lines
                        .extend(split_line(&normalize(&capture.buf), LINE_CHARS));
                }
            }
        }
        titles_only && name == "title-info"
    }
}

fn body_is_notes(start: &quick_xml::events::BytesStart<'_>) -> bool {
    let Some(name) = attr(start, b"name") else {
        return false;
    };
    let name = name.to_ascii_lowercase();
    name.contains("note") || name.contains("comment")
}

fn attr(start: &quick_xml::events::BytesStart<'_>, key: &[u8]) -> Option<String> {
    for attr in start.attributes().flatten() {
        if local_name(attr.key.as_ref()) == key {
            return Some(String::from_utf8_lossy(&attr.value).into_owned());
        }
    }
    None
}

fn is_line_tag(name: &str) -> bool {
    matches!(name, "p" | "v" | "subtitle" | "text-author")
}

fn tag_name(name: QName<'_>) -> String {
    String::from_utf8_lossy(&local_name(name.as_ref()).to_ascii_lowercase()).into_owned()
}

fn local_name(raw: &[u8]) -> &[u8] {
    match raw.iter().rposition(|byte| *byte == b':') {
        Some(index) => &raw[index + 1..],
        None => raw,
    }
}

fn normalize(text: &str) -> String {
    text.split_whitespace()
        .map(|word| word.replace('\u{00ad}', ""))
        .filter(|word| !word.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

fn split_line(text: &str, limit: usize) -> Vec<String> {
    let limit = limit.max(1);
    let chars: Vec<char> = text.chars().collect();
    if chars.is_empty() {
        return Vec::new();
    }
    if chars.len() <= limit {
        return vec![text.to_string()];
    }
    let mut parts = Vec::new();
    let mut start = 0;
    while start < chars.len() {
        if chars.len() - start <= limit {
            let piece: String = chars[start..].iter().collect::<String>().trim().to_string();
            if !piece.is_empty() {
                parts.push(piece);
            }
            break;
        }
        let end = start + limit;
        let cut = chars[start..end]
            .iter()
            .rposition(|ch| *ch == ' ')
            .filter(|index| *index >= 20)
            .unwrap_or(limit);
        let piece: String = chars[start..start + cut]
            .iter()
            .collect::<String>()
            .trim()
            .to_string();
        if !piece.is_empty() {
            parts.push(piece);
        }
        start += cut;
        while start < chars.len() && chars[start] == ' ' {
            start += 1;
        }
    }
    parts
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use super::*;
    use crate::mistakes::MistakeKind;
    use crate::model::{Lang, SpeedUnit};
    use crate::stats::DualStats;

    const SAMPLE: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<FictionBook xmlns="http://www.gribuser.ru/xml/fictionbook/2.0">
  <description>
    <title-info>
      <book-title>Тест &amp; книга</book-title>
      <annotation><p>Аннотация</p></annotation>
    </title-info>
  </description>
  <body>
    <title><p>Глава</p></title>
    <section>
      <p>Первая <emphasis>строка</emphasis>.</p>
      <empty-line/>
      <p>  Вторая   строка  </p>
      <poem><stanza><v>Стих</v></stanza></poem>
    </section>
  </body>
  <body name="notes">
    <section><p>Сноска</p></section>
  </body>
</FictionBook>"#;

    fn opts() -> TypeOpts {
        TypeOpts {
            hide_punctuation: false,
            no_capitals: false,
            speed_unit: SpeedUnit::Wpm,
            ui: UiLang::En,
        }
    }

    fn entry(key: &str) -> BookEntry {
        BookEntry {
            key: key.into(),
            title: "Книга".into(),
            path: PathBuf::from(key),
        }
    }

    #[test]
    fn long_paragraph_splits_on_spaces() {
        let text = "один два три четыре пять шесть семь восемь девять десять";
        let parts = split_line(text, 24);
        assert!(parts.len() > 1);
        assert!(parts.iter().all(|part| part.chars().count() <= 24));
        assert_eq!(parts.join(" "), text);
    }

    #[test]
    fn fb2_skips_annotation_and_notes() {
        let parsed = parse_fb2(SAMPLE.as_bytes(), false).unwrap();
        assert_eq!(parsed.title, "Тест & книга");
        assert_eq!(
            parsed.lines,
            vec![
                "Глава".to_string(),
                "Первая строка.".to_string(),
                "Вторая строка".to_string(),
                "Стих".to_string(),
            ]
        );
    }

    #[test]
    fn windows1251_title() {
        let mut xml = Vec::from(&b"<?xml version=\"1.0\" encoding=\"windows-1251\"?>\n"[..]);
        xml.extend_from_slice(b"<FictionBook><description><title-info><book-title>");
        xml.extend_from_slice(&[0xCF, 0xF0, 0xE8, 0xE2, 0xE5, 0xF2]);
        xml.extend_from_slice(b"</book-title></title-info></description><body><p>");
        xml.extend_from_slice(&[0xCF, 0xF0, 0xE8, 0xE2, 0xE5, 0xF2]);
        xml.extend_from_slice(b"</p></body></FictionBook>");
        let parsed = parse_fb2(xml.as_slice(), false).unwrap();
        assert_eq!(parsed.title, "Привет");
        assert_eq!(parsed.lines, vec!["Привет".to_string()]);
    }

    #[test]
    fn zip_roundtrip_and_list() {
        let dir = std::env::temp_dir().join(format!("langtype-fb2-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("tale.fb2.zip");
        let file = File::create(&path).unwrap();
        let mut zip = zip::ZipWriter::new(file);
        zip.start_file(
            "tale.fb2",
            zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Stored),
        )
        .unwrap();
        zip.write_all(SAMPLE.as_bytes()).unwrap();
        zip.finish().unwrap();
        fs::write(dir.join("note.txt"), "nope").unwrap();

        let books = list(&dir);
        assert_eq!(books.len(), 1);
        assert_eq!(books[0].title, "Тест & книга");
        assert_eq!(books[0].key, "tale.fb2.zip");
        let lines = load_lines(&books[0].path, UiLang::En).unwrap();
        assert_eq!(lines[0], "Глава");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn each_book_keeps_its_line() {
        let dir = std::env::temp_dir().join(format!("langtype-place-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("books.json");
        let mut progress = BookProgress::load(&path);
        progress.set(&path, "a.fb2", 2, 10);
        progress.set(&path, "b.fb2", 5, 8);
        let loaded = BookProgress::load(&path);
        assert_eq!(loaded.place("a.fb2").unwrap().line, 2);
        assert_eq!(loaded.place("b.fb2").unwrap().line, 5);
        assert_eq!(
            loaded.menu_label(
                &BookEntry {
                    key: "b.fb2".into(),
                    title: "Вторая".into(),
                    path: PathBuf::from("b.fb2"),
                },
                UiLang::En,
            ),
            "Вторая  ·  line 6 / 8"
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn typing_joins_lifetime_stats_and_mistakes() {
        let dir = std::env::temp_dir().join(format!("langtype-stats-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let mut session = BookSession::start(
            &entry("a.fb2"),
            vec!["привет".into(), "ещё".into()],
            None,
            opts(),
        )
        .unwrap();
        let mut bank = MistakeBank::in_directory(&dir);
        bank.begin_session();

        let mut done = None;
        for ch in ['п', 'р', 'и', 'х', 'е', 'т'] {
            done = session.on_char(ch, &mut bank);
        }
        let done = done.expect("finished line");
        assert_eq!(done.lang, Lang::Ru);
        assert_eq!(done.counters.keystrokes, 6);
        assert_eq!(done.counters.correct, 5);
        assert_eq!(done.counters.sides, 1);
        assert!(session.finish().is_none());

        let stats_path = dir.join("lifetime.json");
        let mut lifetime = DualStats::default().plus_side(done.lang, done.counters, false);
        lifetime.sessions = 1;
        lifetime.save(&stats_path);
        let loaded = DualStats::load(&stats_path);
        assert_eq!(loaded.for_lang(Lang::Ru).keystrokes, 6);
        assert_eq!(loaded.for_lang(Lang::Ru).correct, 5);
        assert_eq!(loaded.for_lang(Lang::Ru).sides, 1);
        assert_eq!(loaded.for_lang(Lang::En).keystrokes, 0);
        assert_eq!(loaded.sessions, 1);

        assert_eq!(
            bank.ranked(Lang::Ru, MistakeKind::Letters),
            vec![("в".into(), 1)]
        );
        assert_eq!(
            bank.ranked(Lang::Ru, MistakeKind::Words),
            vec![("привет".into(), 1)]
        );
        assert_eq!(
            bank.ranked(Lang::Ru, MistakeKind::Syllables),
            vec![("прив".into(), 1)]
        );

        assert!(session.on_char('е', &mut bank).is_none());
        let partial = session.finish().expect("open line");
        assert_eq!(partial.counters.keystrokes, 1);
        assert_eq!(partial.counters.correct, 1);
        let lifetime = lifetime.plus_side(partial.lang, partial.counters, false);
        lifetime.save(&stats_path);
        let loaded = DualStats::load(&stats_path);
        assert_eq!(loaded.for_lang(Lang::Ru).keystrokes, 7);
        assert_eq!(loaded.for_lang(Lang::Ru).sides, 2);

        bank.flush();
        let letters = fs::read_to_string(dir.join("mistakes-letters.json")).unwrap();
        let words = fs::read_to_string(dir.join("mistakes-words.json")).unwrap();
        let syllables = fs::read_to_string(dir.join("mistakes-syllables.json")).unwrap();
        assert!(letters.contains("\"в\": 1"));
        assert!(words.contains("\"привет\": 1"));
        assert!(syllables.contains("\"прив\": 1"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn stop_and_resume_on_the_same_line() {
        let lines = vec!["раз".into(), "два".into(), "три".into()];
        let mut session = BookSession::start(&entry("a.fb2"), lines.clone(), None, opts()).unwrap();
        let mut bank = MistakeBank::default();
        assert!(session.on_char('р', &mut bank).is_none());
        assert_eq!(session.checkpoint(), (0, 3));
        session.on_char('а', &mut bank);
        assert!(session.on_char('з', &mut bank).is_some());
        assert_eq!(session.current().unwrap().target(), "два");
        assert_eq!(session.checkpoint(), (1, 3));

        let mut again = BookSession::start(
            &entry("a.fb2"),
            lines,
            Some(BookPlace { line: 1, total: 3 }),
            opts(),
        )
        .unwrap();
        assert_eq!(again.current().unwrap().target(), "два");
        again.on_char('д', &mut bank);
        assert_eq!(again.checkpoint(), (1, 3));
    }

    #[test]
    fn finished_book_stays_finished_until_typed() {
        let lines = vec!["раз".into(), "два".into()];
        let mut session = BookSession::start(
            &entry("a.fb2"),
            lines,
            Some(BookPlace { line: 2, total: 2 }),
            opts(),
        )
        .unwrap();
        assert_eq!(session.current().unwrap().target(), "раз");
        assert_eq!(session.checkpoint(), (2, 2));
        let mut bank = MistakeBank::default();
        session.on_char('р', &mut bank);
        assert_eq!(session.checkpoint(), (0, 2));
    }
}
