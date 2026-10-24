use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::Modifier;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};
use ratatui::Frame;

use crate::books::BookSession;
use crate::game::{Game, Mode, Phase, Side};
use crate::i18n::UiLang;
use crate::menu::MenuPage;
use crate::mistakes::MistakeKind;
use crate::model::{Lang, SpeedUnit};
use crate::stats::{format_duration, speed_line, DualStats};
use crate::text::{chars_match, wrap_indices};
use crate::theme;

const BANNER: [&str; 5] = [
    r" _        _    _   _  ____ _______   ______  _____ ",
    r"| |      / \  | \ | |/ ___|_   _\ \ / /  _ \| ____|",
    r"| |     / _ \ |  \| | |  _  | |  \ V /| |_) |  _|  ",
    r"| |___ / ___ \| |\  | |_| | | |   | | |  __/| |___ ",
    r"|_____/_/   \_\_| \_|\____| |_|   |_| |_|   |_____|",
];

#[derive(Clone, Copy, PartialEq, Eq)]
enum PaneState {
    Active,
    Preview,
    Done,
}

pub fn draw_menu(frame: &mut Frame, page: &MenuPage) {
    let area = frame.area();
    clear(frame, area);
    if area.width == 0 || area.height == 0 {
        return;
    }
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(theme::border())
        .style(theme::bg());
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if inner.width == 0 || inner.height == 0 {
        return;
    }
    let body = Rect {
        x: inner.x,
        y: inner.y,
        width: inner.width,
        height: inner.height.saturating_sub(1),
    };
    draw_menu_body(frame, body, page);
    let ui = page.ui;
    let hint = Line::from(vec![
        Span::styled("↑↓", theme::accent()),
        Span::styled(format!(" {}   ", ui.say("move", "выбор")), theme::dim()),
        Span::styled("Enter", theme::ok()),
        Span::styled(format!(" {}   ", ui.say("open", "открыть")), theme::dim()),
        Span::styled("1-9", theme::accent()),
        Span::styled(format!(" {}   ", ui.say("jump", "пункт")), theme::dim()),
        Span::styled("Esc", theme::warn()),
        Span::styled(format!(" {}", page.back_hint), theme::dim()),
    ]);
    frame.render_widget(
        Paragraph::new(hint).alignment(Alignment::Center),
        Rect {
            x: inner.x,
            y: inner.y + inner.height.saturating_sub(1),
            width: inner.width,
            height: 1,
        },
    );
}

fn draw_menu_body(frame: &mut Frame, inner: Rect, page: &MenuPage) {
    if inner.width == 0 || inner.height == 0 {
        return;
    }
    let width = inner.width as usize;
    let banner_ok = page.show_banner && inner.width >= 52 && inner.height >= 12;
    let heading = if banner_ok {
        5
    } else if page.show_banner || !page.title.is_empty() {
        1
    } else {
        0
    };
    let subtitle = usize::from(!page.subtitle.is_empty());
    let between = usize::from(page.show_banner && heading > 0 && subtitle == 1);
    let gap = usize::from(heading + subtitle > 0);
    let fixed = heading + between + subtitle + gap;
    let list_room = (inner.height as usize).saturating_sub(fixed).max(1);
    let (start, shown) = if page.items.len() <= list_room {
        (0, page.items.len())
    } else {
        let start = page
            .selected
            .saturating_sub(list_room / 2)
            .min(page.items.len() - list_room);
        (start, list_room)
    };
    let labels: Vec<String> = (start..page.items.len())
        .take(shown)
        .map(|index| menu_row_label(index, &page.items[index], index == page.selected, ""))
        .collect();
    let menu_w = labels
        .iter()
        .map(|label| label.chars().count() + 3)
        .max()
        .unwrap_or(8)
        .max(8)
        .min(width) as u16;
    let content_h = (fixed + shown) as u16;
    let mut y = inner.y + content_offset(inner.height, content_h);
    let bottom = inner.y.saturating_add(inner.height);
    if banner_ok {
        let lines: Vec<Line> = BANNER
            .iter()
            .map(|line| Line::from(Span::styled(*line, theme::accent())))
            .collect();
        let banner_h = 5.min(bottom.saturating_sub(y));
        frame.render_widget(
            Paragraph::new(lines).alignment(Alignment::Center),
            Rect {
                x: inner.x,
                y,
                width: inner.width,
                height: banner_h,
            },
        );
        y = y.saturating_add(banner_h).min(bottom);
    } else if heading == 1 && y < bottom {
        let head = if page.title.is_empty() {
            "langtype"
        } else {
            page.title
        };
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(head, theme::title())))
                .alignment(Alignment::Center),
            Rect {
                x: inner.x,
                y,
                width: inner.width,
                height: 1,
            },
        );
        y = y.saturating_add(1);
    }
    if between == 1 && y < bottom {
        y = y.saturating_add(1);
    }
    if !page.subtitle.is_empty() && y < bottom {
        let style = if page.show_banner {
            theme::ok()
        } else {
            theme::dim()
        };
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                fit_width(&page.subtitle, width),
                style,
            )))
            .alignment(Alignment::Center),
            Rect {
                x: inner.x,
                y,
                width: inner.width,
                height: 1,
            },
        );
        y = y.saturating_add(1);
    }
    if gap == 1 && y < bottom {
        y = y.saturating_add(1);
    }
    let menu_x = inner.x + inner.width.saturating_sub(menu_w) / 2;
    for (draw_i, index) in (start..page.items.len()).take(shown).enumerate() {
        if y + draw_i as u16 >= bottom {
            break;
        }
        let arrow = if index == start && start > 0 {
            "  ↑"
        } else if index + 1 == start + shown && start + shown < page.items.len() {
            "  ↓"
        } else {
            ""
        };
        let prefix_room = format!(" ▶  {}.  ", index + 1).chars().count() + arrow.chars().count();
        let room = (menu_w as usize).saturating_sub(prefix_room);
        let label = menu_row_label(
            index,
            &fit_item(&page.items[index], room),
            index == page.selected,
            arrow,
        );
        let style = if index == page.selected {
            theme::selected()
        } else {
            theme::text()
        };
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(label, style))).style(style),
            Rect {
                x: menu_x,
                y: y + draw_i as u16,
                width: menu_w,
                height: 1,
            },
        );
    }
}

fn menu_row_label(index: usize, item: &str, selected: bool, arrow: &str) -> String {
    let marker = if selected { "▶" } else { " " };
    format!(" {marker}  {}.  {item}{arrow}", index + 1)
}

fn content_offset(area_h: u16, content_h: u16) -> u16 {
    if content_h >= area_h {
        0
    } else {
        (area_h - content_h) / 2
    }
}

/// Keep a trailing `  ·  …` note when the row is narrower than the label.
fn fit_item(item: &str, width: usize) -> String {
    if item.chars().count() <= width {
        return item.to_string();
    }
    if let Some(sep) = item.rfind("  ·  ") {
        let tail = &item[sep..];
        let tail_len = tail.chars().count();
        if tail_len + 2 <= width {
            return format!("{}{tail}", fit_width(&item[..sep], width - tail_len));
        }
    }
    fit_width(item, width)
}

fn fit_width(text: &str, width: usize) -> String {
    if text.chars().count() <= width {
        return text.to_string();
    }
    if width == 0 {
        return String::new();
    }
    let mut out: String = text.chars().take(width - 1).collect();
    out.push('…');
    out
}

pub fn draw_typing(frame: &mut Frame, game: &Game, layout_ok: bool, order: Lang, ui: UiLang) {
    let area = frame.area();
    clear(frame, area);
    let live = game.live();
    let chunks = vertical(
        area,
        &[
            Constraint::Length(5),
            Constraint::Min(5),
            Constraint::Length(1),
        ],
    );
    let header = vec![
        Line::from(Span::styled(format!(" {} ", game.title), theme::title())),
        Line::from(""),
        Line::from(Span::styled(
            speed_line(&live, game.speed_unit, order, ui),
            theme::accent(),
        )),
    ];
    frame.render_widget(
        Paragraph::new(header).block(panel(
            format!(" {} ", ui.say("session", "сессия")),
            theme::title(),
            theme::border(),
        )),
        chunks[0],
    );

    if game.mode == Mode::Solo {
        draw_solo(frame, chunks[1], game, ui);
    } else {
        draw_versus(frame, chunks[1], game, ui);
    }
    draw_footer(
        frame,
        chunks[2],
        ui.say("finish", "конец"),
        layout_hint(game.active_lang, layout_ok, ui),
        &format!(
            "   {}",
            ui.say("errors can be skipped", "ошибки можно пропускать")
        ),
        ui,
    );
}

pub fn draw_book(frame: &mut Frame, book: &BookSession, layout_ok: bool, order: Lang, ui: UiLang) {
    let area = frame.area();
    clear(frame, area);
    let live = book.live();
    let chunks = vertical(
        area,
        &[
            Constraint::Length(5),
            Constraint::Min(5),
            Constraint::Length(1),
        ],
    );
    let header = vec![
        Line::from(vec![
            Span::styled(format!(" {} ", clip_header(book.title())), theme::title()),
            Span::styled(book.progress_label(ui), theme::accent()),
        ]),
        Line::from(""),
        Line::from(Span::styled(
            speed_line(&live, book.speed_unit(), order, ui),
            theme::accent(),
        )),
    ];
    frame.render_widget(
        Paragraph::new(header).block(panel(
            format!(" {} ", ui.say("book", "книга")),
            theme::title(),
            theme::border(),
        )),
        chunks[0],
    );

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(theme::border())
        .title(Span::styled(
            format!(
                " {}  ·  {} ",
                book.lang().label(),
                ui.say("type here", "печатайте")
            ),
            theme::accent(),
        ));
    let inner = block.inner(chunks[1]);
    frame.render_widget(block, chunks[1]);
    draw_book_lines(frame, inner, book, ui);
    draw_footer(
        frame,
        chunks[2],
        ui.say("books", "книги"),
        layout_hint(book.lang(), layout_ok, ui),
        &format!(
            "   {}",
            ui.say("errors can be skipped", "ошибки можно пропускать")
        ),
        ui,
    );
}

fn clip_header(text: &str) -> String {
    const MAX: usize = 28;
    if text.chars().count() <= MAX {
        return text.to_string();
    }
    let mut out: String = text.chars().take(MAX - 1).collect();
    out.push('…');
    out
}

fn draw_book_lines(frame: &mut Frame, inner: Rect, book: &BookSession, ui: UiLang) {
    if inner.width < 2 || inner.height == 0 {
        return;
    }
    let width = inner.width as usize;
    let mut lines = Vec::new();
    for (index, side) in book.history().iter().enumerate() {
        if index > 0 {
            lines.push(Line::from(""));
        }
        let (painted, _) = paint_side(side, PaneState::Done, width);
        lines.extend(painted);
    }
    let cursor_line = if let Some(current) = book.current() {
        if !lines.is_empty() {
            lines.push(Line::from(""));
        }
        let (painted, cursor) = paint_side(current, PaneState::Active, width);
        let cursor_line = lines.len() + cursor;
        lines.extend(painted);
        for preview in book.preview_texts() {
            lines.push(Line::from(""));
            let side = Side::new(preview);
            let (painted, _) = paint_side(&side, PaneState::Preview, width);
            lines.extend(painted);
        }
        cursor_line
    } else {
        if !lines.is_empty() {
            lines.push(Line::from(""));
        }
        lines.push(Line::from(Span::styled(
            format!(" {}", ui.say("Book finished", "Книга закончена")),
            theme::ok(),
        )));
        lines.len().saturating_sub(1)
    };
    render_lines(frame, inner, &lines, 0, lines.len(), Some(cursor_line));
}

pub fn draw_drill(
    frame: &mut Frame,
    lang: Lang,
    kind_label: &str,
    top: &str,
    rows: &[Side],
    layout_ok: bool,
    ui: UiLang,
) {
    let area = frame.area();
    clear(frame, area);
    let chunks = vertical(
        area,
        &[
            Constraint::Length(3),
            Constraint::Min(5),
            Constraint::Length(2),
        ],
    );
    frame.render_widget(
        Paragraph::new(vec![
            Line::from(Span::styled(
                format!(
                    " {}  ·  {}  ·  {kind_label}",
                    ui.say("Practice mistakes", "Тренировка ошибок"),
                    lang.label()
                ),
                theme::title(),
            )),
            Line::from(Span::styled(format!(" {top}"), theme::ok())),
        ]),
        chunks[0],
    );
    let block = panel(
        format!(" {} ", ui.say("drill", "тренировка")),
        theme::accent(),
        theme::border(),
    );
    let inner = block.inner(chunks[1]);
    frame.render_widget(block, chunks[1]);
    draw_column(frame, inner, rows);
    draw_footer(
        frame,
        chunks[2],
        ui.say("back", "назад"),
        layout_hint(lang, layout_ok, ui),
        &format!(
            "   {}",
            ui.say(
                "time only, mistakes are not recorded",
                "только время, ошибки не записываются"
            )
        ),
        ui,
    );
}

pub fn draw_summary(
    frame: &mut Frame,
    stats: &DualStats,
    unit: SpeedUnit,
    first: Lang,
    seconds_left: f64,
    ui: UiLang,
) {
    let area = frame.area();
    clear(frame, area);
    let height = 12u16.min(area.height);
    let width = 54u16.min(area.width);
    let rect = centered(area, width, height);
    let block = panel(
        format!(" {} ", ui.say("session", "сессия")),
        theme::title(),
        theme::border(),
    );
    let inner = block.inner(rect);
    frame.render_widget(block, rect);

    let mut lines = vec![
        Line::from(""),
        Line::from(Span::styled(
            format!("  {}", ui.say("SESSION", "СЕССИЯ")),
            theme::title(),
        )),
        Line::from(""),
    ];
    for (index, line) in summary_lines(stats, unit, first, ui).iter().enumerate() {
        let style = if index == 0 {
            theme::accent()
        } else {
            theme::text()
        };
        lines.push(Line::from(Span::styled(format!("  {line}"), style)));
    }
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        format!(
            "  {} {}    {} {:.0}{}",
            ui.say("unit", "единица"),
            unit.tag(),
            ui.say("menu in", "меню через"),
            seconds_left.ceil(),
            ui.say("s", " с")
        ),
        theme::dim(),
    )));
    lines.push(Line::from(vec![
        Span::styled("  Enter", theme::ok()),
        Span::styled(format!(" {}    ", ui.say("skip", "пропуск")), theme::dim()),
        Span::styled("Esc", theme::warn()),
        Span::styled(format!(" {}", ui.say("menu", "меню")), theme::dim()),
    ]));
    frame.render_widget(Paragraph::new(lines), inner);
}

pub fn draw_flash(frame: &mut Frame, text: &str) {
    let area = frame.area();
    clear(frame, area);
    let width = (text.chars().count() as u16 + 6).min(area.width).max(10);
    let rect = centered(area, width, 3.min(area.height));
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            text,
            theme::warn().add_modifier(Modifier::BOLD),
        )))
        .alignment(Alignment::Center)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(theme::border()),
        ),
        rect,
    );
}

pub fn kind_label(kind: MistakeKind, in_line: bool, ui: UiLang) -> String {
    let base = match (ui, kind) {
        (UiLang::En, MistakeKind::Letters) => "letters",
        (UiLang::En, MistakeKind::Words) => "words",
        (UiLang::En, MistakeKind::Syllables) => "syllables",
        (UiLang::Ru, MistakeKind::Letters) => "буквы",
        (UiLang::Ru, MistakeKind::Words) => "слова",
        (UiLang::Ru, MistakeKind::Syllables) => "слоги",
    };
    if in_line {
        format!("{base} {}", ui.say("in a line", "в строку"))
    } else {
        base.to_string()
    }
}

fn draw_versus(frame: &mut Frame, area: Rect, game: &Game, ui: UiLang) {
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(area);
    paint_lang_pane(frame, cols[0], game, game.first_lang, ui);
    paint_lang_pane(frame, cols[1], game, game.second_lang(), ui);
}

fn paint_lang_pane(frame: &mut Frame, area: Rect, game: &Game, lang: Lang, ui: UiLang) {
    let active = game.rows.last().and_then(|row| row.phase.lang()) == Some(lang);
    let title = if active {
        format!(
            " {}  ·  {} ",
            lang.label(),
            ui.say("type here", "печатайте")
        )
    } else {
        format!(" {} ", lang.label())
    };
    let border = if active {
        theme::border()
    } else {
        theme::border_dim()
    };
    let title_style = if active {
        theme::accent()
    } else {
        theme::dim()
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(border)
        .title(Span::styled(title, title_style));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if inner.width < 2 || inner.height == 0 {
        return;
    }

    let width = inner.width as usize;
    let mut lines = Vec::new();
    let mut row_start = 0usize;
    let mut cursor_line = None;
    for (index, row) in game.rows.iter().enumerate() {
        let state = pane_state(row.phase, lang, game.first_lang);
        let at = lines.len();
        let (painted, cursor) = paint_side(row.side(lang), state, width);
        if index + 1 == game.rows.len() {
            row_start = at;
            if state == PaneState::Active {
                cursor_line = Some(at + cursor);
            }
        }
        lines.extend(painted);
        if index + 1 != game.rows.len() {
            lines.push(Line::from(""));
        }
    }
    render_lines(frame, inner, &lines, row_start, lines.len(), cursor_line);
}

fn draw_solo(frame: &mut Frame, area: Rect, game: &Game, ui: UiLang) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(theme::border())
        .title(Span::styled(
            format!(
                " {}  ·  {} ",
                game.first_lang.label(),
                ui.say("type here", "печатайте")
            ),
            theme::accent(),
        ));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    draw_column(frame, inner, &game.solo_rows);
}

fn draw_column(frame: &mut Frame, inner: Rect, rows: &[Side]) {
    if inner.width < 2 || inner.height == 0 {
        return;
    }
    let width = inner.width as usize;
    let mut lines = Vec::new();
    let mut cursor_line = 0usize;
    for (index, side) in rows.iter().enumerate() {
        let state = if side.done() {
            PaneState::Done
        } else {
            PaneState::Active
        };
        let (painted, cursor) = paint_side(side, state, width);
        if index + 1 == rows.len() {
            cursor_line = lines.len() + cursor;
        }
        lines.extend(painted);
        if index + 1 != rows.len() {
            lines.push(Line::from(""));
        }
    }
    render_lines(frame, inner, &lines, 0, lines.len(), Some(cursor_line));
}

fn pane_state(phase: Phase, lang: Lang, first: Lang) -> PaneState {
    match phase {
        Phase::Done => PaneState::Done,
        Phase::Typing(active) if active == lang => PaneState::Active,
        Phase::Typing(active) if active == first => PaneState::Preview,
        Phase::Typing(_) => PaneState::Done,
    }
}

fn paint_side(side: &Side, state: PaneState, width: usize) -> (Vec<Line<'static>>, usize) {
    let target: Vec<char> = side.target().chars().collect();
    let typed: Vec<char> = side.typed().chars().collect();
    let wrapped = wrap_indices(side.target(), width.max(1));
    let mut lines = Vec::new();
    let mut cursor_line = 0usize;
    for (line_index, indexes) in wrapped.iter().enumerate() {
        let mut spans = Vec::new();
        for &index in indexes {
            let ch = target.get(index).copied().unwrap_or(' ');
            let (text, style) = match state {
                PaneState::Preview => (ch, theme::untyped()),
                PaneState::Done | PaneState::Active => {
                    if index < typed.len() {
                        let got = typed[index];
                        if chars_match(got, ch) {
                            (got, theme::typed_ok())
                        } else {
                            (if got == ' ' { '_' } else { got }, theme::typed_bad())
                        }
                    } else if state == PaneState::Active && index == typed.len() {
                        cursor_line = line_index;
                        (if ch == ' ' { '_' } else { ch }, theme::cursor())
                    } else {
                        (ch, theme::untyped())
                    }
                }
            };
            spans.push(Span::styled(text.to_string(), style));
        }
        if spans.is_empty() && state == PaneState::Active {
            spans.push(Span::styled(" ", theme::cursor()));
        }
        lines.push(Line::from(spans));
    }
    if lines.is_empty() {
        lines.push(Line::from(Span::styled(" ", theme::cursor())));
    }
    (lines, cursor_line)
}

fn render_lines(
    frame: &mut Frame,
    area: Rect,
    lines: &[Line<'static>],
    row_start: usize,
    row_end: usize,
    cursor_line: Option<usize>,
) {
    let height = area.height as usize;
    if height == 0 || lines.is_empty() {
        return;
    }
    let start = if lines.len() <= height {
        0
    } else {
        view_start(lines.len(), height, row_start, row_end, cursor_line)
    };
    let view: Vec<Line> = lines.iter().skip(start).take(height).cloned().collect();
    frame.render_widget(Paragraph::new(view).wrap(Wrap { trim: false }), area);
}

/// Keep the current pair on screen. The side being typed follows the cursor;
/// the other side stays on that same pair instead of sticking to the top.
fn view_start(
    total: usize,
    height: usize,
    row_start: usize,
    row_end: usize,
    cursor_line: Option<usize>,
) -> usize {
    let row_start = row_start.min(total.saturating_sub(1));
    let row_end = row_end.max(row_start + 1).min(total);
    let max_start = total - height;
    if let Some(cursor) = cursor_line {
        let cursor = cursor.min(total - 1);
        let mut start = cursor.saturating_add(1).saturating_sub(height);
        if row_end - row_start <= height {
            start = start.min(row_start);
        }
        return start.min(max_start);
    }
    if row_end - row_start >= height {
        return row_start.min(max_start);
    }
    row_end.saturating_sub(height).min(row_start).min(max_start)
}

fn summary_lines(stats: &DualStats, unit: SpeedUnit, first: Lang, ui: UiLang) -> Vec<String> {
    let second = first.other();
    let line = |lang: Lang| {
        format!(
            "{}   {:.0} {}    {} {:.0}%",
            lang.label(),
            stats.for_lang(lang).net(unit),
            unit.tag(),
            ui.say("Accuracy", "Точность"),
            stats.for_lang(lang).accuracy()
        )
    };
    vec![
        format!(
            "{} {}    {} {}",
            ui.say("Time", "Время"),
            format_duration(stats.total_seconds()),
            ui.say("Pairs", "Пары"),
            stats.pairs
        ),
        line(first),
        line(second),
    ]
}

fn layout_hint(lang: Lang, ok: bool, ui: UiLang) -> String {
    if ok {
        format!("{} {}", ui.say("layout", "раскладка"), lang.label())
    } else {
        format!(
            "{} {}",
            ui.say("switch layout to", "смените раскладку на"),
            lang.label()
        )
    }
}

fn draw_footer(
    frame: &mut Frame,
    area: Rect,
    esc_label: &str,
    layout: String,
    tail: &str,
    ui: UiLang,
) {
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(" Esc ", theme::warn()),
            Span::styled(format!("{esc_label}   "), theme::dim()),
            Span::styled(" ⌫ ", theme::accent()),
            Span::styled(format!("{}   ", ui.say("undo", "стереть")), theme::dim()),
            Span::styled(layout, theme::dim()),
            Span::styled(tail, theme::dim()),
        ])),
        area,
    );
}

fn panel(
    title: impl Into<String>,
    title_style: ratatui::style::Style,
    border: ratatui::style::Style,
) -> Block<'static> {
    Block::default()
        .borders(Borders::ALL)
        .border_style(border)
        .title(Span::styled(title.into(), title_style))
}

fn vertical(area: Rect, constraints: &[Constraint]) -> std::rc::Rc<[Rect]> {
    Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(area)
}

fn centered(area: Rect, width: u16, height: u16) -> Rect {
    Rect {
        x: area.x + area.width.saturating_sub(width) / 2,
        y: area.y + area.height.saturating_sub(height) / 2,
        width,
        height,
    }
}

fn clear(frame: &mut Frame, area: Rect) {
    frame.render_widget(Block::default().style(theme::bg()), area);
}

#[cfg(test)]
mod tests {
    use super::{content_offset, fit_item, view_start};

    #[test]
    fn other_pane_follows_current_pair() {
        let start = view_start(30, 8, 24, 28, None);
        assert!(start <= 24);
        assert!(start + 8 >= 28);
        assert!(start > 0);
    }

    #[test]
    fn narrow_menu_keeps_the_progress_note() {
        let item = "A very long book title that will not fit  ·  line 12 / 400";
        let fitted = fit_item(item, 36);
        assert!(fitted.chars().count() <= 36);
        assert!(fitted.ends_with("  ·  line 12 / 400"));
        assert!(fitted.contains('…'));
    }

    #[test]
    fn menu_block_sits_in_the_middle() {
        assert_eq!(content_offset(24, 12), 6);
        assert_eq!(content_offset(10, 10), 0);
        assert_eq!(content_offset(8, 12), 0);
    }
}
