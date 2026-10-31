use std::io;
use std::time::{Duration, Instant};

use crossterm::cursor::{Hide, Show};
use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;

use crate::books::{self, BookProgress, BookSession, TypeOpts};
use crate::dict::{self, Pair};
use crate::drill::{Drill, LineSource};
use crate::game::{Commit, Config, Game, Mode};
use crate::layout::LayoutSwitcher;
use crate::menu::{Command, Effect, Menu, MenuCtx, MenuId};
use crate::mistakes::{MistakeBank, MistakeKind};
use crate::model::Lang;
use crate::settings::{self, Settings};
use crate::stats::{self, DualStats};
use crate::ui;

const TICK: Duration = Duration::from_millis(50);
const SUMMARY: Duration = Duration::from_secs(7);
const FLASH: Duration = Duration::from_millis(1400);

enum Screen {
    Menu,
    Typing,
    Drill(Drill),
    Book,
    Summary {
        stats: DualStats,
        deadline: Instant,
    },
    Flash {
        text: String,
        until: Instant,
        back: MenuId,
    },
}

struct App {
    settings: Settings,
    settings_path: std::path::PathBuf,
    lifetime_path: std::path::PathBuf,
    lifetime: DualStats,
    sentences: Vec<Pair>,
    words: Vec<Pair>,
    bank: MistakeBank,
    layouts: LayoutSwitcher,
    menu: Menu,
    screen: Screen,
    game: Option<Game>,
    books: Vec<books::BookEntry>,
    book_progress: BookProgress,
    book_progress_path: std::path::PathBuf,
    book: Option<BookSession>,
    cols: u16,
}

impl App {
    fn boot() -> Result<Self, dict::LoadError> {
        let dir = dict::dictionary_dir();
        let sentences = dict::load_sentences(&dir)?;
        let words = dict::load_words(&dir)?;
        let settings_path = settings::settings_path();
        let lifetime_path = stats::lifetime_path();
        Ok(Self {
            settings: Settings::load(&settings_path),
            settings_path,
            lifetime: DualStats::load(&lifetime_path),
            lifetime_path,
            sentences,
            words,
            bank: MistakeBank::load(),
            layouts: LayoutSwitcher::detect(),
            menu: Menu::new(),
            screen: Screen::Menu,
            game: None,
            books: books::list(&dir),
            book_progress: BookProgress::load(&BookProgress::path()),
            book_progress_path: BookProgress::path(),
            book: None,
            cols: 80,
        })
    }

    fn menu_ctx(&self) -> MenuCtx<'_> {
        MenuCtx {
            settings: &self.settings,
            lifetime: &self.lifetime,
            books: &self.books,
            book_progress: &self.book_progress,
        }
    }

    fn refresh_books(&mut self) {
        self.books = books::list(&dict::dictionary_dir());
    }

    fn welcome(&mut self) {
        self.menu.reset();
        self.game = None;
        self.layouts.restore();
        self.screen = Screen::Menu;
    }

    fn save_settings(&self) {
        self.settings.save(&self.settings_path);
    }

    fn absorb(&mut self, commit: Commit) {
        self.lifetime = self
            .lifetime
            .plus_side(commit.lang, commit.counters, commit.complete_pair);
        self.lifetime.save(&self.lifetime_path);
    }

    fn start_game(
        &mut self,
        mode: Mode,
        lang: Lang,
        deck: Vec<Pair>,
        words_per_line: usize,
        repeat_word: bool,
        title: String,
    ) {
        self.lifetime.sessions += 1;
        self.lifetime.save(&self.lifetime_path);
        self.bank.begin_session();
        let mut game = Game::new(Config {
            deck,
            speed_unit: self.settings.speed_unit,
            first_lang: lang,
            hide_punctuation: self.settings.hide_punctuation,
            no_capitals: self.settings.no_capitals,
            mode,
            words_per_line,
            repeat_word,
            title,
        });
        game.ensure_started();
        game.layout_ok = self.layouts.set_lang(game.active_lang);
        self.game = Some(game);
        self.screen = Screen::Typing;
    }

    fn end_game(&mut self) {
        let outcome = self
            .game
            .as_mut()
            .map(|game| (game.finish(), game.session().clone()));
        let Some((commit, stats)) = outcome else {
            self.welcome();
            return;
        };
        if let Some(commit) = commit {
            self.absorb(commit);
        }
        self.bank.flush();
        self.layouts.restore();
        self.screen = Screen::Summary {
            stats,
            deadline: Instant::now() + SUMMARY,
        };
    }

    fn drill_width(&self) -> usize {
        (self.cols as usize).saturating_sub(4).max(8)
    }

    fn line_source(&self) -> LineSource<'_> {
        LineSource {
            bank: &self.bank,
            no_capitals: self.settings.no_capitals,
            line_rows: self.settings.in_line_rows,
            width: self.drill_width(),
            ui: self.settings.ui_lang,
        }
    }

    fn start_drill(&mut self, lang: Lang, kind: MistakeKind, in_line: bool) {
        let source = self.line_source();
        match Drill::begin(lang, kind, in_line, false, &source) {
            Ok(mut drill) => {
                drill.layout_ok = self.layouts.set_lang(lang);
                self.screen = Screen::Drill(drill);
            }
            Err(text) => {
                self.screen = Screen::Flash {
                    text,
                    until: Instant::now() + FLASH,
                    back: MenuId::PracticeKind(lang),
                };
            }
        }
    }

    fn apply(&mut self, command: Command) {
        match command {
            Command::TogglePunct => self
                .update_settings(|settings| settings.hide_punctuation = !settings.hide_punctuation),
            Command::ToggleCaps => {
                self.update_settings(|settings| settings.no_capitals = !settings.no_capitals)
            }
            Command::ToggleRepeat => {
                self.update_settings(|settings| settings.versus_repeat = !settings.versus_repeat)
            }
            Command::SetUnit(unit) => self.update_settings(|settings| settings.speed_unit = unit),
            Command::SetOrder(lang) => self.update_settings(|settings| settings.first_lang = lang),
            Command::SetRows(rows) => self.update_settings(|settings| settings.in_line_rows = rows),
            Command::SetWords(words) => {
                self.update_settings(|settings| settings.versus_words = words)
            }
            Command::SetUi(lang) => self.update_settings(|settings| settings.ui_lang = lang),
            Command::ResetStats => self.lifetime = DualStats::reset(&self.lifetime_path),
            Command::Solo(lang) => {
                let deck = self.sentences.clone();
                let title = format!(
                    "{} {}",
                    self.settings.ui_lang.say("Practice", "Печать"),
                    lang.label()
                );
                self.start_game(Mode::Solo, lang, deck, 1, false, title);
            }
            Command::VersusWords => {
                let deck = self.words.clone();
                let count = self.settings.versus_words as usize;
                let repeat = self.settings.versus_repeat;
                let lang = self.settings.first_lang;
                let title = self.settings.ui_lang.say("Words", "Слова").to_string();
                self.start_game(Mode::Versus, lang, deck, count, repeat, title);
            }
            Command::VersusSentences => {
                let deck = self.sentences.clone();
                let lang = self.settings.first_lang;
                let title = self
                    .settings
                    .ui_lang
                    .say("Sentences", "Предложения")
                    .to_string();
                self.start_game(Mode::Versus, lang, deck, 1, false, title);
            }
            Command::Drill {
                lang,
                kind,
                in_line,
            } => self.start_drill(lang, kind, in_line),
            Command::Book(index) => self.start_book(index),
        }
    }

    fn start_book(&mut self, index: usize) {
        let Some(entry) = self.books.get(index).cloned() else {
            return;
        };
        let lines = match books::load_lines(&entry.path, self.settings.ui_lang) {
            Ok(lines) => lines,
            Err(text) => {
                self.flash(text, MenuId::Books);
                return;
            }
        };
        let place = self.book_progress.place(&entry.key);
        match BookSession::start(
            &entry,
            lines,
            place,
            TypeOpts {
                hide_punctuation: self.settings.hide_punctuation,
                no_capitals: self.settings.no_capitals,
                speed_unit: self.settings.speed_unit,
                ui: self.settings.ui_lang,
            },
        ) {
            Ok(mut session) => {
                self.lifetime.sessions += 1;
                self.lifetime.save(&self.lifetime_path);
                self.bank.begin_session();
                session.layout_ok = self.layouts.set_lang(session.lang());
                self.book = Some(session);
                self.screen = Screen::Book;
            }
            Err(text) => self.flash(text, MenuId::Books),
        }
    }

    fn flash(&mut self, text: impl Into<String>, back: MenuId) {
        self.screen = Screen::Flash {
            text: text.into(),
            until: Instant::now() + FLASH,
            back,
        };
    }

    fn end_book(&mut self) {
        let Some(mut session) = self.book.take() else {
            return;
        };
        if let Some(commit) = session.finish() {
            self.absorb(commit);
        }
        let (line, total) = session.checkpoint();
        self.book_progress
            .set(&self.book_progress_path, session.key(), line, total);
        self.bank.flush();
        self.layouts.restore();
        self.refresh_books();
        self.screen = Screen::Menu;
    }

    fn save_book_place(&mut self) {
        let Some(session) = &self.book else {
            return;
        };
        let key = session.key().to_string();
        let (line, total) = session.checkpoint();
        self.book_progress
            .set(&self.book_progress_path, &key, line, total);
    }

    fn update_settings(&mut self, change: impl FnOnce(&mut Settings)) {
        change(&mut self.settings);
        self.save_settings();
    }

    fn on_menu_key(&mut self, code: KeyCode) -> bool {
        let before = self.menu.current();
        let effect = self.menu.on_key(
            code,
            &MenuCtx {
                settings: &self.settings,
                lifetime: &self.lifetime,
                books: &self.books,
                book_progress: &self.book_progress,
            },
        );
        let quit = match effect {
            Effect::Quit => true,
            Effect::None => false,
            Effect::Command(command) => {
                self.apply(command);
                false
            }
        };
        if matches!(self.screen, Screen::Menu)
            && self.menu.current() == MenuId::Books
            && before != MenuId::Books
        {
            self.refresh_books();
        }
        quit
    }

    fn on_typing_key(&mut self, code: KeyCode) {
        match code {
            KeyCode::Esc => self.end_game(),
            KeyCode::Backspace => {
                if let Some(game) = self.game.as_mut() {
                    game.backspace();
                }
            }
            KeyCode::Char(ch) => self.type_in_game(ch),
            _ => {}
        }
    }

    fn on_book_key(&mut self, code: KeyCode) {
        match code {
            KeyCode::Esc => self.end_book(),
            KeyCode::Backspace => {
                if let Some(book) = self.book.as_mut() {
                    book.backspace();
                }
            }
            KeyCode::Char(ch) => self.type_in_book(ch),
            _ => {}
        }
    }

    fn type_in_book(&mut self, ch: char) {
        let commit = {
            let Some(session) = self.book.as_mut() else {
                return;
            };
            session.set_wrap_width(book_text_width(self.cols));
            session.on_char(ch, &mut self.bank)
        };
        let Some(commit) = commit else {
            return;
        };
        self.absorb(commit);
        self.save_book_place();
    }

    fn type_in_game(&mut self, ch: char) {
        let Some(result) = self
            .game
            .as_mut()
            .map(|game| game.on_char(ch, &mut self.bank))
        else {
            return;
        };
        if let Some(commit) = result.commit {
            self.absorb(commit);
        }
        if let Some(lang) = result.switched_to {
            let ok = self.layouts.set_lang(lang);
            if let Some(game) = self.game.as_mut() {
                game.layout_ok = ok;
            }
        }
    }

    fn on_drill_key(&mut self, code: KeyCode) {
        match code {
            KeyCode::Esc => {
                let elapsed = match &self.screen {
                    Screen::Drill(drill) => drill.elapsed(),
                    _ => 0.0,
                };
                if elapsed > 0.0 {
                    self.lifetime.practice_seconds += elapsed;
                    self.lifetime.save(&self.lifetime_path);
                }
                self.welcome();
            }
            KeyCode::Backspace => {
                if let Screen::Drill(drill) = &mut self.screen {
                    drill.backspace();
                }
            }
            KeyCode::Char(ch) => self.type_in_drill(ch),
            _ => {}
        }
    }

    fn type_in_drill(&mut self, ch: char) {
        let no_capitals = self.settings.no_capitals;
        let line_rows = self.settings.in_line_rows;
        let width = self.drill_width();
        let ui = self.settings.ui_lang;
        let Screen::Drill(drill) = &mut self.screen else {
            return;
        };
        drill.type_char(
            ch,
            &LineSource {
                bank: &self.bank,
                no_capitals,
                line_rows,
                width,
                ui,
            },
        );
    }

    fn tick(&mut self) {
        let now = Instant::now();
        let expired = match &self.screen {
            Screen::Summary { deadline, .. } => now >= *deadline,
            Screen::Flash { until, .. } => now >= *until,
            _ => false,
        };
        if !expired {
            return;
        }
        let back = match &self.screen {
            Screen::Flash { back, .. } => Some(*back),
            _ => None,
        };
        if let Some(id) = back {
            self.menu.jump(id);
            self.screen = Screen::Menu;
        } else {
            self.welcome();
        }
    }

    fn handle(&mut self, event: Event) -> bool {
        let Event::Key(key) = event else {
            return false;
        };
        if key.kind == KeyEventKind::Release {
            return false;
        }
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            if matches!(self.screen, Screen::Book) {
                self.end_book();
            }
            return true;
        }
        match &self.screen {
            Screen::Menu => self.on_menu_key(key.code),
            Screen::Typing => {
                self.on_typing_key(key.code);
                false
            }
            Screen::Book => {
                self.on_book_key(key.code);
                false
            }
            Screen::Drill(_) => {
                self.on_drill_key(key.code);
                false
            }
            Screen::Summary { .. } => {
                if matches!(key.code, KeyCode::Enter | KeyCode::Esc) {
                    self.welcome();
                }
                false
            }
            Screen::Flash { .. } => false,
        }
    }

    fn draw(&self, frame: &mut ratatui::Frame) {
        match &self.screen {
            Screen::Menu => ui::draw_menu(frame, &self.menu.page(&self.menu_ctx())),
            Screen::Typing => {
                if let Some(game) = &self.game {
                    ui::draw_typing(
                        frame,
                        game,
                        game.layout_ok && self.layouts.available(),
                        self.settings.first_lang,
                        self.settings.ui_lang,
                    );
                }
            }
            Screen::Book => {
                if let Some(book) = &self.book {
                    ui::draw_book(
                        frame,
                        book,
                        book.layout_ok && self.layouts.available(),
                        self.settings.first_lang,
                        self.settings.ui_lang,
                    );
                }
            }
            Screen::Drill(drill) => {
                let mut top = self
                    .bank
                    .top_line(drill.lang, drill.kind, self.settings.ui_lang);
                if self.settings.no_capitals {
                    top = top.to_lowercase();
                }
                ui::draw_drill(
                    frame,
                    drill.lang,
                    &ui::kind_label(drill.kind, drill.in_line, self.settings.ui_lang),
                    &top,
                    &drill.rows,
                    drill.layout_ok && self.layouts.available(),
                    self.settings.ui_lang,
                );
            }
            Screen::Summary { stats, deadline } => {
                let left = deadline
                    .saturating_duration_since(Instant::now())
                    .as_secs_f64();
                ui::draw_summary(
                    frame,
                    stats,
                    self.settings.speed_unit,
                    self.settings.first_lang,
                    left,
                    self.settings.ui_lang,
                );
            }
            Screen::Flash { text, .. } => ui::draw_flash(frame, text),
        }
    }
}

/// Book text is inside a one-cell border, matching `draw_book`.
fn book_text_width(cols: u16) -> usize {
    (cols as usize).saturating_sub(2).max(1)
}

struct RawGuard;

impl Drop for RawGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), LeaveAlternateScreen, Show);
    }
}

pub fn run() -> io::Result<()> {
    let mut app = App::boot().map_err(io::Error::other)?;
    enable_raw_mode()?;
    execute!(io::stdout(), EnterAlternateScreen, Hide)?;
    let _guard = RawGuard;
    let mut terminal = Terminal::new(CrosstermBackend::new(io::stdout()))?;
    terminal.clear()?;

    loop {
        if let Ok((width, _)) = crossterm::terminal::size() {
            app.cols = width;
        }
        terminal.draw(|frame| app.draw(frame))?;
        app.tick();
        if event::poll(TICK)? && app.handle(event::read()?) {
            break;
        }
    }
    app.layouts.restore();
    Ok(())
}
