use crossterm::event::KeyCode;

use crate::books::{self, BookEntry, BookProgress};
use crate::i18n::UiLang;
use crate::mistakes::MistakeKind;
use crate::model::{Lang, SpeedUnit};
use crate::settings::Settings;
use crate::stats::{speed_line, DualStats};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MenuId {
    Welcome,
    Start,
    Books,
    Versus,
    Settings,
    Speed,
    Order,
    Rows,
    WordsPerLine,
    Ui,
    Reset,
    PracticeLang,
    PracticeKind(Lang),
    PracticeHow(Lang, MistakeKind),
}

#[derive(Clone, Copy, Debug)]
pub enum Command {
    Solo(Lang),
    VersusWords,
    VersusSentences,
    Drill {
        lang: Lang,
        kind: MistakeKind,
        in_line: bool,
    },
    Book(usize),
    TogglePunct,
    ToggleCaps,
    ToggleRepeat,
    SetUnit(SpeedUnit),
    SetOrder(Lang),
    SetRows(u32),
    SetWords(u32),
    SetUi(UiLang),
    ResetStats,
}

pub enum Effect {
    None,
    Quit,
    Command(Command),
}

pub struct MenuCtx<'a> {
    pub settings: &'a Settings,
    pub lifetime: &'a DualStats,
    pub books: &'a [BookEntry],
    pub book_progress: &'a BookProgress,
}

pub struct MenuPage {
    pub title: &'static str,
    pub subtitle: String,
    pub items: Vec<String>,
    pub selected: usize,
    pub back_hint: &'static str,
    pub show_banner: bool,
    pub ui: UiLang,
}

#[derive(Clone, Copy)]
enum Action {
    Quit,
    Back,
    Open(MenuId),
    TogglePunct,
    ToggleCaps,
    ToggleRepeat,
    SetUnit(SpeedUnit),
    SetOrder(Lang),
    SetRows(u32),
    SetWords(u32),
    SetUi(UiLang),
    ResetStats,
    Solo(Lang),
    VersusWords,
    VersusSentences,
    Drill {
        lang: Lang,
        kind: MistakeKind,
        in_line: bool,
    },
    Book(usize),
}

pub struct Menu {
    id: MenuId,
    selected: usize,
    stack: Vec<(MenuId, usize)>,
}

impl Menu {
    pub fn new() -> Self {
        Self {
            id: MenuId::Welcome,
            selected: 0,
            stack: Vec::new(),
        }
    }

    pub fn current(&self) -> MenuId {
        self.id
    }

    pub fn reset(&mut self) {
        self.stack.clear();
        self.id = MenuId::Welcome;
        self.selected = 0;
    }

    /// Show `id` without touching the back stack. Used when a flash returns.
    pub fn jump(&mut self, id: MenuId) {
        self.id = id;
        self.selected = 0;
    }

    pub fn page(&self, ctx: &MenuCtx<'_>) -> MenuPage {
        MenuPage {
            title: title(self.id, ctx.settings.ui_lang),
            subtitle: subtitle(self.id, ctx),
            items: items(self.id, ctx)
                .into_iter()
                .map(|(label, _)| label)
                .collect(),
            selected: self.selected,
            back_hint: ctx.settings.ui_lang.say(
                if self.id == MenuId::Welcome {
                    "quit"
                } else {
                    "back"
                },
                if self.id == MenuId::Welcome {
                    "выход"
                } else {
                    "назад"
                },
            ),
            show_banner: self.id == MenuId::Welcome,
            ui: ctx.settings.ui_lang,
        }
    }

    pub fn on_key(&mut self, code: KeyCode, ctx: &MenuCtx<'_>) -> Effect {
        let actions: Vec<Action> = items(self.id, ctx)
            .into_iter()
            .map(|(_, action)| action)
            .collect();
        let len = actions.len();
        if len > 0 && self.selected >= len {
            self.selected = len - 1;
        }
        match code {
            KeyCode::Up | KeyCode::Char('k') => {
                self.step(false, len);
                Effect::None
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.step(true, len);
                Effect::None
            }
            KeyCode::Enter => {
                let Some(action) = actions.get(self.selected).copied() else {
                    return Effect::None;
                };
                self.activate(action)
            }
            KeyCode::Esc => {
                if self.id == MenuId::Welcome && self.stack.is_empty() {
                    Effect::Quit
                } else {
                    self.back();
                    Effect::None
                }
            }
            KeyCode::Char(c @ '1'..='9') => {
                let index = (c as u8 - b'1') as usize;
                if index < len.min(9) {
                    self.activate(actions[index])
                } else {
                    Effect::None
                }
            }
            _ => Effect::None,
        }
    }

    fn step(&mut self, forward: bool, len: usize) {
        if len == 0 {
            return;
        }
        self.selected = if forward {
            (self.selected + 1) % len
        } else if self.selected == 0 {
            len - 1
        } else {
            self.selected - 1
        };
    }

    fn activate(&mut self, action: Action) -> Effect {
        match action {
            Action::Quit => Effect::Quit,
            Action::Back => {
                self.back();
                Effect::None
            }
            Action::Open(id) => {
                self.open(id);
                Effect::None
            }
            Action::SetUnit(unit) => self.confirm(Command::SetUnit(unit)),
            Action::SetOrder(lang) => self.confirm(Command::SetOrder(lang)),
            Action::SetRows(rows) => self.confirm(Command::SetRows(rows)),
            Action::SetWords(words) => self.confirm(Command::SetWords(words)),
            Action::SetUi(lang) => self.confirm(Command::SetUi(lang)),
            Action::ResetStats => self.confirm(Command::ResetStats),
            Action::TogglePunct => Effect::Command(Command::TogglePunct),
            Action::ToggleCaps => Effect::Command(Command::ToggleCaps),
            Action::ToggleRepeat => Effect::Command(Command::ToggleRepeat),
            Action::Solo(lang) => Effect::Command(Command::Solo(lang)),
            Action::VersusWords => Effect::Command(Command::VersusWords),
            Action::VersusSentences => Effect::Command(Command::VersusSentences),
            Action::Drill {
                lang,
                kind,
                in_line,
            } => Effect::Command(Command::Drill {
                lang,
                kind,
                in_line,
            }),
            Action::Book(index) => Effect::Command(Command::Book(index)),
        }
    }

    fn confirm(&mut self, command: Command) -> Effect {
        self.back();
        Effect::Command(command)
    }

    fn open(&mut self, id: MenuId) {
        self.stack.push((self.id, self.selected));
        self.id = id;
        self.selected = 0;
    }

    fn back(&mut self) {
        if let Some((id, selected)) = self.stack.pop() {
            self.id = id;
            self.selected = selected;
        } else {
            self.id = MenuId::Welcome;
            self.selected = 0;
        }
    }
}

impl Default for Menu {
    fn default() -> Self {
        Self::new()
    }
}

fn title(id: MenuId, ui: UiLang) -> &'static str {
    match id {
        MenuId::Welcome => "",
        MenuId::Start => ui.say("Start", "Старт"),
        MenuId::Books => ui.say("Type a book", "Печатать книгу"),
        MenuId::Versus => ui.say("Language vs language", "Язык против языка"),
        MenuId::Settings => ui.say("Settings", "Настройки"),
        MenuId::Speed => ui.say("Speed unit", "Единица скорости"),
        MenuId::Order => ui.say("Language order", "Порядок языков"),
        MenuId::Rows => ui.say("In-a-line rows", "Строк подряд"),
        MenuId::WordsPerLine => ui.say("Words per line", "Слов в строке"),
        MenuId::Ui => ui.say("Program language", "Язык программы"),
        MenuId::Reset => ui.say("Reset all statistics?", "Сбросить всю статистику?"),
        MenuId::PracticeLang | MenuId::PracticeKind(_) | MenuId::PracticeHow(_, _) => {
            ui.say("Practice mistakes", "Тренировка ошибок")
        }
    }
}

fn subtitle(id: MenuId, ctx: &MenuCtx<'_>) -> String {
    let ui = ctx.settings.ui_lang;
    match id {
        MenuId::Welcome => {
            if ctx.lifetime.total_seconds() <= 0.0 && ctx.lifetime.pairs == 0 {
                format!(
                    "{}  ·  {}",
                    ui.say("No stats yet", "Статистики пока нет"),
                    ctx.settings.speed_unit.tag()
                )
            } else {
                speed_line(
                    ctx.lifetime,
                    ctx.settings.speed_unit,
                    ctx.settings.first_lang,
                    ui,
                )
            }
        }
        MenuId::Speed => ui
            .say("How typing speed is shown", "Как показывается скорость")
            .into(),
        MenuId::Order => ui
            .say("First language is on the left", "Первый язык слева")
            .into(),
        MenuId::Rows => ui
            .say(
                "How many lines of the same item before the next one",
                "Сколько одинаковых строк до следующего задания",
            )
            .into(),
        MenuId::WordsPerLine => ui
            .say(
                "Language vs language  ·  Words",
                "Язык против языка  ·  Слова",
            )
            .into(),
        MenuId::Ui => ui
            .say("English or Russian", "Английский или русский")
            .into(),
        MenuId::Reset => ui
            .say(
                "Enter to confirm   ·   Esc to cancel",
                "Enter подтвердить   ·   Esc отмена",
            )
            .into(),
        MenuId::PracticeLang => ui.say("Choose language", "Выберите язык").into(),
        MenuId::PracticeKind(lang) => ui.practice(lang).into(),
        MenuId::PracticeHow(lang, kind) => format!("{}  ·  {}", ui.practice(lang), kind.label(ui)),
        MenuId::Books if ctx.books.is_empty() => books::empty_hint(ui).into(),
        _ => String::new(),
    }
}

fn items(id: MenuId, ctx: &MenuCtx<'_>) -> Vec<(String, Action)> {
    let settings = ctx.settings;
    let ui = settings.ui_lang;
    match id {
        MenuId::Welcome => vec![
            (ui.say("Start", "Старт").into(), Action::Open(MenuId::Start)),
            (
                ui.say("Settings", "Настройки").into(),
                Action::Open(MenuId::Settings),
            ),
            (ui.say("Quit", "Выход").into(), Action::Quit),
        ],
        MenuId::Start => {
            let left = settings.first_lang;
            let right = left.other();
            vec![
                (
                    format!("{} ({})", ui.say("Typing practice", "Печать"), left.label()),
                    Action::Solo(left),
                ),
                (
                    format!(
                        "{} ({})",
                        ui.say("Typing practice", "Печать"),
                        right.label()
                    ),
                    Action::Solo(right),
                ),
                (
                    ui.say("Type a book", "Печатать книгу").into(),
                    Action::Open(MenuId::Books),
                ),
                (
                    ui.say("Language vs language", "Язык против языка").into(),
                    Action::Open(MenuId::Versus),
                ),
                (
                    ui.say("Practice mistakes", "Тренировка ошибок").into(),
                    Action::Open(MenuId::PracticeLang),
                ),
                (ui.say("Back", "Назад").into(), Action::Back),
            ]
        }
        MenuId::Books => {
            if ctx.books.is_empty() {
                return vec![(ui.say("Back", "Назад").into(), Action::Back)];
            }
            let mut rows: Vec<(String, Action)> = ctx
                .books
                .iter()
                .enumerate()
                .map(|(index, entry)| {
                    (ctx.book_progress.menu_label(entry, ui), Action::Book(index))
                })
                .collect();
            rows.push((ui.say("Back", "Назад").into(), Action::Back));
            rows
        }
        MenuId::Versus => vec![
            (ui.say("Words", "Слова").into(), Action::VersusWords),
            (
                ui.say("Sentences", "Предложения").into(),
                Action::VersusSentences,
            ),
            (ui.say("Back", "Назад").into(), Action::Back),
        ],
        MenuId::Settings => vec![
            (settings.language_label(), Action::Open(MenuId::Ui)),
            (
                ui.say("Reset statistics", "Сбросить статистику").into(),
                Action::Open(MenuId::Reset),
            ),
            (
                format!(
                    "{}: {}",
                    ui.say("Speed unit", "Скорость"),
                    settings.unit_label()
                ),
                Action::Open(MenuId::Speed),
            ),
            (
                format!("{}: {}", ui.say("Order", "Порядок"), settings.order_label()),
                Action::Open(MenuId::Order),
            ),
            (settings.punct_label(), Action::TogglePunct),
            (settings.capitals_label(), Action::ToggleCaps),
            (settings.line_rows_label(), Action::Open(MenuId::Rows)),
            (
                settings.versus_words_label(),
                Action::Open(MenuId::WordsPerLine),
            ),
            (settings.versus_repeat_label(), Action::ToggleRepeat),
            (ui.say("Back", "Назад").into(), Action::Back),
        ],
        MenuId::Speed => vec![
            (
                ui.say("Words per minute (WPM)", "Слов в минуту (WPM)")
                    .into(),
                Action::SetUnit(SpeedUnit::Wpm),
            ),
            (
                ui.say("Characters per minute (CPM)", "Знаков в минуту (CPM)")
                    .into(),
                Action::SetUnit(SpeedUnit::Cpm),
            ),
        ],
        MenuId::Order => vec![
            (
                ui.say("Left EN, right RU", "Слева EN, справа RU").into(),
                Action::SetOrder(Lang::En),
            ),
            (
                ui.say("Left RU, right EN", "Слева RU, справа EN").into(),
                Action::SetOrder(Lang::Ru),
            ),
        ],
        MenuId::Ui => vec![
            ("English".into(), Action::SetUi(UiLang::En)),
            ("Русский".into(), Action::SetUi(UiLang::Ru)),
        ],
        MenuId::Rows => Settings::LINE_ROWS
            .into_iter()
            .map(|rows| (rows.to_string(), Action::SetRows(rows)))
            .collect(),
        MenuId::WordsPerLine => Settings::WORDS_PER_LINE
            .into_iter()
            .map(|words| (words.to_string(), Action::SetWords(words)))
            .collect(),
        MenuId::Reset => vec![
            (
                ui.say("Yes, reset", "Да, сбросить").into(),
                Action::ResetStats,
            ),
            (ui.say("Cancel", "Отмена").into(), Action::Back),
        ],
        MenuId::PracticeLang => vec![
            (
                ui.practice(Lang::Ru).into(),
                Action::Open(MenuId::PracticeKind(Lang::Ru)),
            ),
            (
                ui.practice(Lang::En).into(),
                Action::Open(MenuId::PracticeKind(Lang::En)),
            ),
            (ui.say("Back", "Назад").into(), Action::Back),
        ],
        MenuId::PracticeKind(lang) => vec![
            (
                MistakeKind::Letters.label(ui).into(),
                Action::Open(MenuId::PracticeHow(lang, MistakeKind::Letters)),
            ),
            (
                MistakeKind::Words.label(ui).into(),
                Action::Open(MenuId::PracticeHow(lang, MistakeKind::Words)),
            ),
            (
                MistakeKind::Syllables.label(ui).into(),
                Action::Open(MenuId::PracticeHow(lang, MistakeKind::Syllables)),
            ),
            (ui.say("Back", "Назад").into(), Action::Back),
        ],
        MenuId::PracticeHow(lang, kind) => vec![
            (
                ui.say("One after another", "По очереди").into(),
                Action::Drill {
                    lang,
                    kind,
                    in_line: false,
                },
            ),
            (
                ui.say("In a line", "В строку").into(),
                Action::Drill {
                    lang,
                    kind,
                    in_line: true,
                },
            ),
            (ui.say("Back", "Назад").into(), Action::Back),
        ],
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::books::BookEntry;
    use crate::stats::DualStats;

    #[test]
    fn book_menu_sits_after_ru_practice() {
        let settings = Settings::default();
        let lifetime = DualStats::default();
        let progress = BookProgress::default();
        let books: Vec<BookEntry> = Vec::new();
        let ctx = MenuCtx {
            settings: &settings,
            lifetime: &lifetime,
            books: &books,
            book_progress: &progress,
        };
        let mut menu = Menu::new();
        assert_eq!(menu.page(&ctx).items[0], "Start");
        let _ = menu.on_key(KeyCode::Enter, &ctx);
        assert_eq!(
            menu.page(&ctx).items,
            vec![
                "Typing practice (EN)".to_string(),
                "Typing practice (RU)".to_string(),
                "Type a book".to_string(),
                "Language vs language".to_string(),
                "Practice mistakes".to_string(),
                "Back".to_string(),
            ]
        );
        let _ = menu.on_key(KeyCode::Down, &ctx);
        let _ = menu.on_key(KeyCode::Down, &ctx);
        let _ = menu.on_key(KeyCode::Enter, &ctx);
        let page = menu.page(&ctx);
        assert_eq!(page.title, "Type a book");
        assert_eq!(page.subtitle, books::empty_hint(UiLang::En));
        assert_eq!(page.items, vec!["Back".to_string()]);

        let books = vec![BookEntry {
            key: "a.fb2".into(),
            title: "Тест".into(),
            path: PathBuf::from("a.fb2"),
        }];
        let mut progress = BookProgress::default();
        progress.remember("a.fb2", 4, 20);
        let ctx = MenuCtx {
            settings: &settings,
            lifetime: &lifetime,
            books: &books,
            book_progress: &progress,
        };
        let page = menu.page(&ctx);
        assert_eq!(page.items[0], "Тест  ·  line 5 / 20");
        assert_eq!(page.items[1], "Back");
    }

    #[test]
    fn russian_interface_translates_the_menu() {
        let mut settings = Settings::default();
        settings.ui_lang = UiLang::Ru;
        let lifetime = DualStats::default();
        let progress = BookProgress::default();
        let books: Vec<BookEntry> = Vec::new();
        let ctx = MenuCtx {
            settings: &settings,
            lifetime: &lifetime,
            books: &books,
            book_progress: &progress,
        };
        let menu = Menu::new();
        assert_eq!(
            menu.page(&ctx).items,
            vec![
                "Старт".to_string(),
                "Настройки".to_string(),
                "Выход".to_string(),
            ]
        );
    }
}
