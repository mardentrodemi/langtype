use std::ops::{Index, IndexMut};

/// A practice language. English is stored on the left of a pair, Russian on the right.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Lang {
    En,
    Ru,
}

impl Lang {
    pub fn as_str(self) -> &'static str {
        match self {
            Lang::En => "en",
            Lang::Ru => "ru",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Lang::En => "EN",
            Lang::Ru => "RU",
        }
    }

    pub fn other(self) -> Self {
        match self {
            Lang::En => Lang::Ru,
            Lang::Ru => Lang::En,
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw.to_lowercase().as_str() {
            "en" => Some(Lang::En),
            "ru" => Some(Lang::Ru),
            _ => None,
        }
    }
}

/// One value for English and one for Russian.
#[derive(Clone, Debug)]
pub struct ByLang<T> {
    pub en: T,
    pub ru: T,
}

impl<T> ByLang<T> {
    pub fn new(en: T, ru: T) -> Self {
        Self { en, ru }
    }

    pub fn get(&self, lang: Lang) -> &T {
        match lang {
            Lang::En => &self.en,
            Lang::Ru => &self.ru,
        }
    }

    pub fn get_mut(&mut self, lang: Lang) -> &mut T {
        match lang {
            Lang::En => &mut self.en,
            Lang::Ru => &mut self.ru,
        }
    }
}

impl<T: Default> Default for ByLang<T> {
    fn default() -> Self {
        Self {
            en: T::default(),
            ru: T::default(),
        }
    }
}

impl<T> Index<Lang> for ByLang<T> {
    type Output = T;

    fn index(&self, lang: Lang) -> &T {
        self.get(lang)
    }
}

impl<T> IndexMut<Lang> for ByLang<T> {
    fn index_mut(&mut self, lang: Lang) -> &mut T {
        self.get_mut(lang)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpeedUnit {
    Wpm,
    Cpm,
}

impl SpeedUnit {
    pub fn tag(self) -> &'static str {
        match self {
            SpeedUnit::Wpm => "WPM",
            SpeedUnit::Cpm => "CPM",
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            SpeedUnit::Wpm => "wpm",
            SpeedUnit::Cpm => "cpm",
        }
    }

    pub fn parse(raw: &str) -> Self {
        if raw.eq_ignore_ascii_case("cpm") {
            SpeedUnit::Cpm
        } else {
            SpeedUnit::Wpm
        }
    }
}
