use crate::model::Lang;

/// Language of the program interface. Practice languages stay EN and RU.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UiLang {
    En,
    Ru,
}

impl Default for UiLang {
    fn default() -> Self {
        Self::En
    }
}

impl UiLang {
    pub fn as_str(self) -> &'static str {
        match self {
            UiLang::En => "en",
            UiLang::Ru => "ru",
        }
    }

    pub fn parse(raw: &str) -> Self {
        if raw.eq_ignore_ascii_case("ru") {
            UiLang::Ru
        } else {
            UiLang::En
        }
    }

    /// This language's own name, so the switch stays readable after it changes.
    pub fn own_name(self) -> &'static str {
        match self {
            UiLang::En => "English",
            UiLang::Ru => "Русский",
        }
    }

    pub fn say(self, en: &'static str, ru: &'static str) -> &'static str {
        match self {
            UiLang::En => en,
            UiLang::Ru => ru,
        }
    }

    pub fn practice(self, lang: Lang) -> &'static str {
        match (self, lang) {
            (UiLang::En, Lang::En) => "English",
            (UiLang::En, Lang::Ru) => "Russian",
            (UiLang::Ru, Lang::En) => "Английский",
            (UiLang::Ru, Lang::Ru) => "Русский",
        }
    }

    pub fn on_off(self, on: bool) -> &'static str {
        match (self, on) {
            (UiLang::En, true) => "On",
            (UiLang::En, false) => "Off",
            (UiLang::Ru, true) => "Вкл",
            (UiLang::Ru, false) => "Выкл",
        }
    }
}
