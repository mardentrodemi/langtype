mod app;
mod books;
mod dict;
mod drill;
mod game;
mod i18n;
mod layout;
mod menu;
mod mistakes;
mod model;
mod paths;
mod settings;
mod stats;
mod text;
mod theme;
mod ui;

fn main() {
    if let Err(err) = app::run() {
        eprintln!("langtype: {err}");
        std::process::exit(1);
    }
}
