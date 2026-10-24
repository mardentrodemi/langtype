use std::process::Command;

use serde_json::Value;

use crate::model::Lang;

/// Switches the keyboard layout to the language being typed, then restores it.
#[derive(Clone, Debug)]
pub struct LayoutSwitcher {
    backend: Option<Backend>,
    previous: Option<String>,
}

#[derive(Clone, Debug)]
enum Backend {
    Niri { en: String, ru: String },
    Xkb { en: String, ru: String },
}

impl LayoutSwitcher {
    pub fn detect() -> Self {
        detect_niri().or_else(detect_xkb).unwrap_or(Self {
            backend: None,
            previous: None,
        })
    }

    pub fn available(&self) -> bool {
        self.backend.is_some()
    }

    pub fn set_lang(&self, lang: Lang) -> bool {
        let Some(target) = self.target(lang) else {
            return false;
        };
        match self.backend {
            Some(Backend::Niri { .. }) => {
                command("niri", &["msg", "action", "switch-layout", target]).is_some()
            }
            Some(Backend::Xkb { .. }) => command("setxkbmap", &[target]).is_some(),
            None => false,
        }
    }

    pub fn restore(&self) {
        let Some(previous) = self.previous.as_deref() else {
            return;
        };
        match self.backend {
            Some(Backend::Niri { .. }) => {
                let _ = command("niri", &["msg", "action", "switch-layout", previous]);
            }
            Some(Backend::Xkb { .. }) => {
                let layout = previous.split(',').next().unwrap_or(previous).trim();
                let _ = command("setxkbmap", &[layout]);
            }
            None => {}
        }
    }

    fn target(&self, lang: Lang) -> Option<&str> {
        let (en, ru) = match &self.backend {
            Some(Backend::Niri { en, ru } | Backend::Xkb { en, ru }) => (en, ru),
            None => return None,
        };
        Some(match lang {
            Lang::En => en,
            Lang::Ru => ru,
        })
    }
}

fn detect_niri() -> Option<LayoutSwitcher> {
    if !on_path("niri") {
        return None;
    }
    let stdout = command("niri", &["msg", "-j", "keyboard-layouts"])?;
    let data: Value = serde_json::from_str(&stdout).ok()?;
    let names = data.get("names").and_then(Value::as_array)?;
    let mut en = None;
    let mut ru = None;
    for (index, name) in names.iter().enumerate() {
        let Some(text) = name.as_str() else { continue };
        match (classify(text), en.is_none(), ru.is_none()) {
            (Some(Lang::En), true, _) => en = Some(index.to_string()),
            (Some(Lang::Ru), _, true) => ru = Some(index.to_string()),
            _ => {}
        }
    }
    Some(LayoutSwitcher {
        backend: Some(Backend::Niri { en: en?, ru: ru? }),
        previous: data
            .get("current_idx")
            .and_then(Value::as_u64)
            .map(|n| n.to_string()),
    })
}

fn detect_xkb() -> Option<LayoutSwitcher> {
    if !on_path("setxkbmap") {
        return None;
    }
    let previous = command("setxkbmap", &["-query"]).and_then(|stdout| {
        stdout.lines().find_map(|line| {
            line.strip_prefix("layout:")
                .map(|rest| rest.trim().to_string())
        })
    });
    Some(LayoutSwitcher {
        backend: Some(Backend::Xkb {
            en: "us".into(),
            ru: "ru".into(),
        }),
        previous,
    })
}

fn classify(name: &str) -> Option<Lang> {
    let name = name.to_lowercase();
    if name.contains("ru") || name.contains("рус") || name.contains("russian") {
        Some(Lang::Ru)
    } else if name.contains("us")
        || name.contains("en")
        || name.contains("english")
        || name.contains("amer")
    {
        Some(Lang::En)
    } else {
        None
    }
}

fn on_path(program: &str) -> bool {
    std::env::var_os("PATH")
        .is_some_and(|path| std::env::split_paths(&path).any(|dir| dir.join(program).is_file()))
}

fn command(program: &str, args: &[&str]) -> Option<String> {
    let out = Command::new(program).args(args).output().ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}
