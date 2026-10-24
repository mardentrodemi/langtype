use ratatui::style::{Color, Modifier, Style};

const BG: Color = Color::Rgb(16, 18, 22);
const BORDER: Color = Color::Rgb(78, 140, 176);
const BORDER_DIM: Color = Color::Rgb(48, 64, 78);
const TITLE: Color = Color::Rgb(232, 238, 242);
const TEXT: Color = Color::Rgb(214, 220, 226);
const DIM: Color = Color::Rgb(110, 122, 136);
const ACCENT: Color = Color::Rgb(120, 206, 228);
const OK: Color = Color::Rgb(126, 214, 154);
const WARN: Color = Color::Rgb(232, 196, 96);
const BAD: Color = Color::Rgb(255, 214, 214);
const BAD_BG: Color = Color::Rgb(132, 36, 48);
const CURSOR_FG: Color = Color::Rgb(12, 16, 20);
const CURSOR_BG: Color = Color::Rgb(120, 206, 228);

pub fn text() -> Style {
    Style::default().fg(TEXT)
}

pub fn dim() -> Style {
    Style::default().fg(DIM)
}

pub fn title() -> Style {
    Style::default().fg(TITLE).add_modifier(Modifier::BOLD)
}

pub fn accent() -> Style {
    Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)
}

pub fn ok() -> Style {
    Style::default().fg(OK).add_modifier(Modifier::BOLD)
}

pub fn warn() -> Style {
    Style::default().fg(WARN)
}

pub fn border() -> Style {
    Style::default().fg(BORDER)
}

pub fn border_dim() -> Style {
    Style::default().fg(BORDER_DIM)
}

pub fn selected() -> Style {
    Style::default()
        .fg(CURSOR_FG)
        .bg(ACCENT)
        .add_modifier(Modifier::BOLD)
}

pub fn typed_ok() -> Style {
    Style::default().fg(OK).add_modifier(Modifier::BOLD)
}

pub fn typed_bad() -> Style {
    Style::default()
        .fg(BAD)
        .bg(BAD_BG)
        .add_modifier(Modifier::BOLD)
}

pub fn cursor() -> Style {
    Style::default()
        .fg(CURSOR_FG)
        .bg(CURSOR_BG)
        .add_modifier(Modifier::BOLD)
}

pub fn untyped() -> Style {
    Style::default().fg(DIM)
}

pub fn bg() -> Style {
    Style::default().bg(BG)
}
