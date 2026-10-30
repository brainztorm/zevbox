//! Drawing of `zevbox console` (docs/SPEC_console.md §3): banner on top, menu on
//! the left, description, choice list or command output on the right, valid
//! keys at the bottom, and the boxes (text input, « Êtes-vous sûr ? ») over them.
//! Pure: reads the [`App`], draws into a ratatui `Frame` (tested with
//! `TestBackend`).
//!
//! Colours (feedback 👤 2026-10-30), three, one meaning each, from the terminal's
//! own palette so they follow its theme:
//! - blue: where the keys go (border and title of the active panel, selected
//!   row, focused button);
//! - green: OK (`✓ terminé`, installed versions, « connecté »);
//! - red: attention (`✗ échec …`, « Êtes-vous sûr ? », « non installé »).
//!
//! Everything else uses the default colour, or dim gray for secondary text.

use super::app::{App, Focus, LineKind, Output, RunStatus, View};
use super::menu::{Entry, PanelLine, Tone};
use super::text;
use ratatui::layout::{Constraint, Layout, Position, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, Paragraph};
use ratatui::Frame;

pub const MIN_WIDTH: u16 = 80;
pub const MIN_HEIGHT: u16 = 20;
pub const TOO_SMALL: &str = "Agrandissez la fenêtre (80 × 20 minimum)";
/// Fits « ▸ Vérifier s'il y a des mises à jour » and leaves 40 columns on the
/// right at the 80-column minimum.
const LEFT_WIDTH: u16 = 40;

pub const FOCUS: Color = Color::Blue;
pub const GOOD: Color = Color::Green;
pub const BAD: Color = Color::Red;
pub const DIM: Color = Color::DarkGray;

/// Selected row where the keys go: white bold on blue reads on dark and light
/// themes alike.
fn selected_style() -> Style {
    Style::new()
        .bg(FOCUS)
        .fg(Color::White)
        .add_modifier(Modifier::BOLD)
}

fn tone_style(tone: Tone) -> Style {
    match tone {
        Tone::Body => Style::new(),
        Tone::Dim => Style::new().fg(DIM),
        Tone::Good => Style::new().fg(GOOD),
        Tone::Bad => Style::new().fg(BAD),
    }
}

/// Bordered panel: blue border and bold blue title when it has the keys,
/// dim otherwise.
fn panel(title: &str, focused: bool) -> Block<'static> {
    let (border, title_style) = if focused {
        (
            Style::new().fg(FOCUS),
            Style::new().fg(FOCUS).add_modifier(Modifier::BOLD),
        )
    } else {
        (Style::new().fg(DIM), Style::new().fg(DIM))
    };
    Block::bordered()
        .border_style(border)
        .title(Span::styled(format!(" {title} "), title_style))
}

/// Banner, left panel, right panel, key line.
fn regions(area: Rect) -> [Rect; 4] {
    let [top, main, bottom] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(0),
        Constraint::Length(1),
    ])
    .areas(area);
    let [left, right] =
        Layout::horizontal([Constraint::Length(LEFT_WIDTH), Constraint::Min(0)]).areas(main);
    [top, left, right, bottom]
}

/// Columns and rows where command output is drawn for a terminal of `size`
/// (inside the right panel's border, above its status row). `(0, 0)` if the
/// terminal is too small to draw anything. The description uses the same
/// columns and one more row.
pub fn output_body_size(width: u16, height: u16) -> (usize, usize) {
    if width < MIN_WIDTH || height < MIN_HEIGHT {
        return (0, 0);
    }
    let [_, _, right, _] = regions(Rect::new(0, 0, width, height));
    (
        usize::from(right.width.saturating_sub(2)),
        usize::from(right.height.saturating_sub(3)),
    )
}

pub fn draw(frame: &mut Frame, app: &App) {
    let area = frame.area();
    if area.width < MIN_WIDTH || area.height < MIN_HEIGHT {
        let middle = Rect {
            y: area.y + area.height / 2,
            height: 1.min(area.height),
            ..area
        };
        frame.render_widget(Paragraph::new(TOO_SMALL).centered(), middle);
        return;
    }
    let [top, left, right, bottom] = regions(area);
    let focus = app.focus();

    let full = app.snap.banner_parts(false);
    // One leading space, then the text.
    let fits = full.iter().map(|(_, t)| text::width(t)).sum::<usize>() < usize::from(top.width);
    let parts = if fits {
        full
    } else {
        app.snap.banner_parts(true)
    };
    let mut banner = vec![Span::raw(" ")];
    banner.extend(
        parts
            .into_iter()
            .map(|(tone, t)| Span::styled(t, tone_style(tone))),
    );
    frame.render_widget(
        Paragraph::new(Line::from(banner)).style(Style::new().add_modifier(Modifier::BOLD)),
        top,
    );
    draw_menu(frame, app, left, focus == Focus::Menu);
    match &app.view {
        View::Output(o) => draw_output(frame, o, right),
        View::Wizard(w) if w.picker.is_some() => draw_picker(frame, app, right),
        View::Wizard(w) => draw_description(frame, app, w.action.label(), right, true),
        View::Menu => {
            let title = app.selected_entry().map_or("", Entry::label);
            draw_description(frame, app, title, right, false);
        }
    }
    frame.render_widget(
        Paragraph::new(format!(" {}", app.keys_hint())).style(Style::new().fg(DIM)),
        bottom,
    );

    if let View::Wizard(w) = &app.view {
        if let Some(text) = &w.text {
            draw_text_input(frame, text, area);
        }
    }
    if let Some(dialog) = &app.confirm {
        draw_confirm(frame, dialog, area);
    }
}

/// Rows of `items` with the selection marked, scrolled so it stays visible.
fn list_rows(
    items: &[String],
    cursor: usize,
    rows: usize,
    columns: usize,
    focused: bool,
) -> Vec<Line<'static>> {
    let rows = rows.max(1);
    let first = cursor.saturating_sub(rows - 1);
    items
        .iter()
        .enumerate()
        .skip(first)
        .take(rows)
        .map(|(i, item)| {
            let item = text::truncate_right(item, columns.saturating_sub(2));
            if i != cursor {
                Line::from(format!("  {item}"))
            } else if focused {
                // Pad to the full width so the blue bar reads as a selection.
                let row = format!("▸ {item}");
                let pad = columns.saturating_sub(text::width(&row));
                Line::styled(format!("{row}{}", " ".repeat(pad)), selected_style())
            } else {
                Line::styled(
                    format!("▸ {item}"),
                    Style::new().add_modifier(Modifier::BOLD),
                )
            }
        })
        .collect()
}

fn draw_menu(frame: &mut Frame, app: &App, area: Rect, focused: bool) {
    // The deepest part of the path is the useful one: cut from the left.
    let room = usize::from(area.width.saturating_sub(4));
    let block = panel(&text::truncate_left(&app.path_title(), room), focused);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let items: Vec<String> = app
        .current_menu()
        .children
        .iter()
        .map(|e| match e {
            Entry::Menu(m) => format!("{} ›", m.label),
            Entry::Leaf(a) => a.label().to_string(),
        })
        .collect();
    let lines = list_rows(
        &items,
        app.cursor(),
        usize::from(inner.height),
        usize::from(inner.width),
        focused,
    );
    frame.render_widget(Paragraph::new(lines), inner);
}

/// Logical panel lines wrapped at spaces into styled rows.
fn wrap_panel(lines: &[PanelLine], columns: usize) -> Vec<Line<'static>> {
    lines
        .iter()
        .flat_map(|(tone, l)| {
            text::wrap_words(l, columns)
                .into_iter()
                .map(move |row| Line::styled(row, tone_style(*tone)))
        })
        .collect()
}

/// The description (menu view, or the action being set up behind a text box
/// or a question), scrolled by PgUp/PgDn; the bottom border says if there is
/// more to read.
fn draw_description(frame: &mut Frame, app: &App, title: &str, area: Rect, focused: bool) {
    let block = panel(title, focused);
    let inner = block.inner(area);
    let rows = wrap_panel(&app.panel_lines(), usize::from(inner.width));
    let height = usize::from(inner.height);
    let max = rows.len().saturating_sub(height);
    let start = app.desc_scroll.min(max);
    let more = match (start > 0, start < max) {
        (false, false) => None,
        (true, false) => Some(" ↑ PgUp "),
        (false, true) => Some(" ↓ PgDn : suite "),
        (true, true) => Some(" ↑ PgUp · ↓ PgDn "),
    };
    let block = match more {
        Some(m) => block.title_bottom(Span::styled(m, Style::new().fg(DIM))),
        None => block,
    };
    frame.render_widget(block, area);
    let visible: Vec<Line> = rows.into_iter().skip(start).take(height).collect();
    frame.render_widget(Paragraph::new(visible), inner);
}

/// A choice step: the action, the list (keys go here), and the command so far.
fn draw_picker(frame: &mut Frame, app: &App, area: Rect) {
    let View::Wizard(w) = &app.view else {
        return;
    };
    let Some(picker) = &w.picker else {
        return;
    };
    let block = panel(w.action.label(), true);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let columns = usize::from(inner.width);
    let mut lines = vec![
        Line::styled(
            picker.title.to_string(),
            Style::new().add_modifier(Modifier::BOLD),
        ),
        Line::default(),
    ];
    let footer = wrap_panel(
        &[(
            Tone::Dim,
            format!("Commande : {}", w.action.preview(&w.params)),
        )],
        columns,
    );
    let room = usize::from(inner.height).saturating_sub(lines.len() + footer.len() + 1);
    let items: Vec<String> = picker.choices.iter().map(|c| c.label.clone()).collect();
    if items.is_empty() {
        lines.push(Line::from(picker.empty_message.to_string()));
    } else {
        lines.extend(list_rows(&items, picker.cursor, room, columns, true));
    }
    // The rest of the room explains the action again (cut if it does not fit).
    let left = usize::from(inner.height).saturating_sub(lines.len() + footer.len() + 1);
    if left > 1 {
        lines.push(Line::default());
        lines.extend(
            wrap_panel(&[(Tone::Body, w.action.description().to_string())], columns)
                .into_iter()
                .take(left - 1),
        );
    }
    let used = lines.len();
    let [list, bottom] = Layout::vertical([
        Constraint::Length(u16::try_from(used).unwrap_or(u16::MAX)),
        Constraint::Min(0),
    ])
    .areas(inner);
    frame.render_widget(Paragraph::new(lines), list);
    let [_, cmd] = Layout::vertical([
        Constraint::Min(0),
        Constraint::Length(u16::try_from(footer.len()).unwrap_or(1)),
    ])
    .areas(bottom);
    frame.render_widget(Paragraph::new(footer), cmd);
}

fn draw_output(frame: &mut Frame, o: &Output, area: Rect) {
    let (status, status_style) = match o.status {
        RunStatus::Running {
            interrupting: false,
        } => ("en cours…".to_string(), Style::new()),
        RunStatus::Running { interrupting: true } => ("interruption…".to_string(), Style::new()),
        RunStatus::Done(outcome) => (
            outcome.label(),
            Style::new().fg(if outcome.is_success() { GOOD } else { BAD }),
        ),
    };
    let block = panel(&o.title, true);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if inner.height == 0 {
        return;
    }
    let [body, footer] = Layout::vertical([Constraint::Min(0), Constraint::Length(1)]).areas(inner);

    let width = usize::from(body.width);
    let rows: Vec<(LineKind, String)> = o
        .lines
        .iter()
        .flat_map(|(kind, text)| {
            text::wrap(text, width)
                .into_iter()
                .map(move |row| (*kind, row))
        })
        .collect();
    let height = usize::from(body.height);
    let max_scroll = rows.len().saturating_sub(height);
    let end = rows.len() - o.scroll.min(max_scroll);
    let start = end.saturating_sub(height);
    let lines: Vec<Line> = rows[start..end]
        .iter()
        .map(|(kind, row)| {
            // stdout and stderr in the default colour (stderr is marked « ! »);
            // the console's own `$ zevbox …` lines are secondary.
            let style = match kind {
                LineKind::Out | LineKind::Err => Style::new(),
                LineKind::Info => Style::new().fg(DIM),
            };
            Line::styled(row.clone(), style)
        })
        .collect();
    frame.render_widget(Paragraph::new(lines), body);
    let scrolled = if o.scroll.min(max_scroll) > 0 {
        format!("   (↑ {} lignes)", o.scroll.min(max_scroll))
    } else {
        String::new()
    };
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(status, status_style.add_modifier(Modifier::BOLD)),
            Span::styled(scrolled, Style::new().fg(DIM)),
        ])),
        footer,
    );
}

fn popup(area: Rect, width: u16, height: u16) -> Rect {
    let width = width.min(area.width);
    let height = height.min(area.height);
    Rect {
        x: area.x + (area.width - width) / 2,
        y: area.y + (area.height - height) / 2,
        width,
        height,
    }
}

fn draw_text_input(frame: &mut Frame, text: &super::app::TextInput, area: Rect) {
    use super::menu::TextField;
    let rect = popup(area, 66, 7);
    frame.render_widget(Clear, rect);
    let block = panel(text.field.prompt(), true);
    let inner = block.inner(rect);
    frame.render_widget(block, rect);

    // Horizontal scroll, in columns, so the cursor stays visible in long values.
    let shown: Vec<char> = text.shown().chars().collect();
    let room = usize::from(inner.width).saturating_sub(3).max(1);
    let start = text::scroll_start(&shown, text.cursor, room);
    let mut visible = String::new();
    let mut used = 0;
    for &c in &shown[start..] {
        let w = text::char_width(c);
        if used + w > room {
            break;
        }
        visible.push(c);
        used += w;
    }
    let cursor_column: String = shown[start..text.cursor.min(shown.len())].iter().collect();
    let hint = match text.field {
        TextField::Key if text.optional => "Entrée sans rien taper : garder la clé actuelle.",
        TextField::Key => "La clé n'est ni affichée ni journalisée.",
        TextField::Model => "Entrée sans rien taper : le plus petit modèle du serveur.",
        TextField::Packages => "Exemple : ripgrep jq",
        _ => "",
    };
    let last = match text.error {
        Some(error) => Line::styled(error, Style::new().fg(BAD)),
        None => Line::styled(hint, Style::new().fg(DIM)),
    };
    let lines = vec![
        Line::default(),
        Line::from(format!("> {visible}")),
        Line::default(),
        last,
    ];
    frame.render_widget(Paragraph::new(lines), inner);
    let x = inner.x + 2 + u16::try_from(text::width(&cursor_column)).unwrap_or(0);
    frame.set_cursor_position(Position::new(
        x.min(inner.right().saturating_sub(1)),
        inner.y + 1,
    ));
}

/// « Êtes-vous sûr ? »: red frame (an action with an impact), the focused
/// button in blue.
fn draw_confirm(frame: &mut Frame, dialog: &super::app::ConfirmDialog, area: Rect) {
    let rect = popup(area, 62, 10);
    frame.render_widget(Clear, rect);
    let block = Block::bordered()
        .border_style(Style::new().fg(BAD))
        .title(Span::styled(
            " Êtes-vous sûr ? ",
            Style::new().fg(BAD).add_modifier(Modifier::BOLD),
        ));
    let inner = block.inner(rect);
    frame.render_widget(block, rect);
    let button = |label: &'static str, on: bool| {
        if on {
            Span::styled(label, selected_style())
        } else {
            Span::raw(label)
        }
    };
    let columns = usize::from(inner.width);
    let mut lines = wrap_panel(
        &[(Tone::Body, dialog.confirmation.question.clone())],
        columns,
    )
    .into_iter()
    .map(|l| l.style(Style::new().add_modifier(Modifier::BOLD)))
    .collect::<Vec<_>>();
    lines.push(Line::default());
    lines.extend(wrap_panel(
        &[(Tone::Body, dialog.confirmation.consequence.clone())],
        columns,
    ));
    let [text, buttons] =
        Layout::vertical([Constraint::Min(0), Constraint::Length(1)]).areas(inner);
    frame.render_widget(Paragraph::new(lines), text);
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            button("[ Oui ]", dialog.yes_selected),
            Span::raw("   "),
            button("[ Non ]", !dialog.yes_selected),
        ]))
        .centered(),
        buttons,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::console::app::{App, Outcome};
    use crate::console::menu::Action;
    use crate::console::state::{AgentState, InstanceInfo, Snapshot};
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use ratatui::backend::TestBackend;
    use ratatui::buffer::Buffer;
    use ratatui::Terminal;

    fn snap() -> Snapshot {
        Snapshot {
            installed: true,
            agents: crate::agents::AGENTS
                .iter()
                .map(|a| AgentState {
                    id: a.id,
                    display: a.display,
                    version: (a.id != "omlx").then(|| "0.99.1".to_string()),
                    has_previous: false,
                    authenticated: false,
                })
                .collect(),
            instances: vec![InstanceInfo {
                id: "1a2b3c4d".to_string(),
                agent: "claude".to_string(),
                project: "/tmp/p".to_string(),
            }],
            ..Snapshot::default()
        }
    }

    fn buffer(app: &App, width: u16, height: u16) -> Buffer {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        terminal.draw(|f| draw(f, app)).unwrap();
        terminal.backend().buffer().clone()
    }

    fn render(app: &App, width: u16, height: u16) -> String {
        let buffer = buffer(app, width, height);
        let mut out = String::new();
        for y in 0..buffer.area.height {
            for x in 0..buffer.area.width {
                out.push_str(buffer[(x, y)].symbol());
            }
            out.push('\n');
        }
        out
    }

    fn key(app: &mut App, code: KeyCode) {
        let _ = app.handle_key(KeyEvent::new(code, KeyModifiers::NONE));
    }

    /// Foreground of the first cell of `needle` in `row` (one cell per char).
    fn fg_of(buffer: &Buffer, row: u16, needle: &str) -> Option<Color> {
        let cells: Vec<String> = (0..buffer.area.width)
            .map(|x| buffer[(x, row)].symbol().to_string())
            .collect();
        let wanted: Vec<String> = needle.chars().map(String::from).collect();
        let x = cells
            .windows(wanted.len())
            .position(|w| w == wanted.as_slice())?;
        Some(buffer[(u16::try_from(x).ok()?, row)].fg)
    }

    #[test]
    fn screen_has_banner_two_panels_and_keys() {
        let app = App::new(snap());
        let screen = render(&app, 80, 24);
        let lines: Vec<&str> = screen.lines().collect();
        assert!(lines[0].contains("zevbox ─ Claude 0.99.1 · Antigravity 0.99.1"));
        assert!(
            lines[0].trim_end().ends_with("1 en cours"),
            "compact at 80:\n{screen}"
        );
        assert!(render(&app, 120, 24).contains("1 agent en cours"));
        assert!(
            screen.contains("▸ Agents en cours ›"),
            "left panel:\n{screen}"
        );
        assert!(screen.contains("Première installation"));
        assert!(screen.contains("Contient :"), "right panel:\n{screen}");
        assert!(
            lines[23].contains("Menu : ↑↓ choisir"),
            "key line:\n{screen}"
        );
        assert!(lines[23].contains("quitter"));
        assert!(lines[1].starts_with("┌ zevbox "), "path title:\n{screen}");
    }

    #[test]
    fn the_panel_with_the_keys_is_blue() {
        let mut app = App::new(snap());
        // Menu: left border blue, right border dim.
        let b = buffer(&app, 80, 24);
        assert_eq!(b[(0, 1)].fg, FOCUS, "left top-left corner");
        assert_eq!(b[(40, 1)].fg, DIM, "right top-left corner");
        // The selected row of the focused list is white on blue.
        assert_eq!(b[(1, 2)].bg, FOCUS);
        assert_eq!(b[(1, 2)].fg, Color::White);
        // Inside an action (a picker): right blue, left dim, no blue bar on the left.
        key(&mut app, KeyCode::Enter);
        key(&mut app, KeyCode::Down);
        key(&mut app, KeyCode::Enter); // Arrêter un agent
        let b = buffer(&app, 80, 24);
        assert_eq!(b[(0, 1)].fg, DIM);
        assert_eq!(b[(40, 1)].fg, FOCUS);
        assert_ne!(
            b[(1, 3)].bg,
            FOCUS,
            "the menu's selection is no longer blue"
        );
        let screen = render(&app, 80, 24);
        assert!(screen.contains("Choisir l'agent à arrêter"), "{screen}");
        assert!(screen.contains("▸ 1a2b3c4d · claude"), "{screen}");
        assert!(screen.contains("Commande : zevbox stop <id>"), "{screen}");
        assert!(
            screen.contains("Arrête l'agent choisi"),
            "description under the list"
        );
        assert!(screen.lines().nth(23).unwrap().contains("Liste à droite"));
    }

    #[test]
    fn banner_colours_installed_green_and_missing_red() {
        let app = App::new(snap());
        let b = buffer(&app, 100, 24);
        // "0.99.1" (Claude) is green, "non installé" (oMLX) red.
        assert_eq!(fg_of(&b, 0, "0.99.1"), Some(GOOD));
        assert_eq!(fg_of(&b, 0, "non installé"), Some(BAD));
        assert_eq!(fg_of(&b, 0, "Claude"), Some(Color::Reset));
        let b = buffer(&App::new(Snapshot::default()), 80, 24);
        assert_eq!(fg_of(&b, 0, "zevbox n'est pas"), Some(BAD));
    }

    #[test]
    fn leaf_shows_its_description_cost_and_command() {
        let mut app = App::new(snap());
        key(&mut app, KeyCode::Down);
        key(&mut app, KeyCode::Enter);
        key(&mut app, KeyCode::Down); // Tout mettre à jour
        let screen = render(&app, 80, 24);
        assert!(screen.contains("zevbox › Mises à jour"), "{screen}");
        assert!(screen.contains("Construit une image neuve"), "{screen}");
        assert!(screen.contains("Commande : zevbox update"), "{screen}");
    }

    #[test]
    fn long_description_scrolls_with_page_keys() {
        // At the 80×20 minimum, find an entry whose description is longer than
        // the panel (menus and leaves of every level).
        let mut app = App::new(snap());
        app.output_body = output_body_size(80, 20);
        let mut found = None;
        for top in 0..app.root.children.len() {
            app.path.clear();
            app.cursors = vec![top];
            if app.desc_max_scroll() > 0 {
                found = Some((vec![], vec![top]));
                break;
            }
            if let Some(Entry::Menu(m)) = app.root.children.get(top) {
                for leaf in 0..m.children.len() {
                    app.path = vec![top];
                    app.cursors = vec![top, leaf];
                    if app.desc_max_scroll() > 0 {
                        found = Some((vec![top], vec![top, leaf]));
                        break;
                    }
                }
            }
            if found.is_some() {
                break;
            }
        }
        let (path, cursors) = found.expect("one description is longer than 80×20 allows");
        app.path = path;
        app.cursors = cursors;
        let last = app.panel_lines().last().map(|(_, l)| l.clone()).unwrap();
        let screen = render(&app, 80, 20);
        assert!(screen.contains("↓ PgDn : suite"), "{screen}");
        assert!(!screen.contains(&last), "{screen}");
        for _ in 0..5 {
            key(&mut app, KeyCode::PageDown);
        }
        let screen = render(&app, 80, 20);
        assert!(screen.contains(&last), "{screen}");
        assert!(screen.contains("↑ PgUp"));
    }

    #[test]
    fn auth_leaf_shows_the_connection_in_colour() {
        let mut app = App::new(snap());
        key(&mut app, KeyCode::Down);
        key(&mut app, KeyCode::Down);
        key(&mut app, KeyCode::Enter);
        key(&mut app, KeyCode::Down); // Se connecter à Claude
        let screen = render(&app, 100, 40);
        assert!(screen.contains("État : non connecté"), "{screen}");
        app.snap.agents[0].authenticated = true;
        let b = buffer(&app, 100, 40);
        let row = (0..40)
            .find(|&y| {
                (0..100)
                    .map(|x| b[(x, y)].symbol())
                    .collect::<String>()
                    .contains("État : connecté")
            })
            .unwrap();
        assert_eq!(fg_of(&b, row, "État : connecté"), Some(GOOD));
    }

    #[test]
    fn banner_when_not_installed() {
        let app = App::new(Snapshot::default());
        let screen = render(&app, 80, 24);
        assert!(screen.contains("zevbox n'est pas installé : lancez Première installation"));
    }

    #[test]
    fn too_small_terminal_asks_for_a_bigger_window() {
        let app = App::new(snap());
        for (w, h) in [(79, 24), (80, 19), (40, 10)] {
            let screen = render(&app, w, h);
            assert!(screen.contains(TOO_SMALL), "{w}x{h}:\n{screen}");
            assert!(!screen.contains("Agents en cours"));
        }
        assert!(!render(&app, 80, 20).contains(TOO_SMALL));
    }

    #[test]
    fn confirm_box_is_red_with_a_blue_focused_button() {
        let mut app = App::new(snap());
        key(&mut app, KeyCode::Enter);
        key(&mut app, KeyCode::Down);
        key(&mut app, KeyCode::Down);
        key(&mut app, KeyCode::Enter); // Arrêter tous les agents
        let screen = render(&app, 80, 24);
        assert!(screen.contains("Êtes-vous sûr ?"), "{screen}");
        assert!(screen.contains("Arrêter tous les agents ?"));
        assert!(screen.contains("Les threads ouverts dans Zed seront coupés."));
        assert!(screen.contains("[ Oui ]   [ Non ]"));
        let b = buffer(&app, 80, 24);
        let title_row = (0..24)
            .find(|&y| {
                (0..80)
                    .map(|x| b[(x, y)].symbol())
                    .collect::<String>()
                    .contains("Êtes-vous sûr ?")
            })
            .unwrap();
        assert_eq!(fg_of(&b, title_row, "Êtes-vous"), Some(BAD));
        let button_row = (0..24)
            .find(|&y| {
                (0..80)
                    .map(|x| b[(x, y)].symbol())
                    .collect::<String>()
                    .contains("[ Non ]")
            })
            .unwrap();
        let non = (0..80)
            .find(|&x| b[(x, button_row)].symbol() == "N")
            .unwrap();
        assert_eq!(b[(non, button_row)].bg, FOCUS, "Non is focused by default");
    }

    #[test]
    fn masked_key_is_never_drawn() {
        let mut app = App::new(snap());
        let _ = app.open_action(Action::AuthOmlx);
        key(&mut app, KeyCode::Char('h'));
        key(&mut app, KeyCode::Enter);
        for c in "topsecret".chars() {
            key(&mut app, KeyCode::Char(c));
        }
        let screen = render(&app, 80, 24);
        assert!(screen.contains("Clé d'API (masquée)"), "{screen}");
        assert!(screen.contains("> •••••••••"));
        assert!(!screen.contains("topsecret"));
    }

    #[test]
    fn output_shows_lines_and_coloured_result() {
        let mut app = App::new(snap());
        let _ = app.open_action(Action::Doctor);
        app.push_line(LineKind::Info, "$ zevbox doctor".to_string());
        app.push_line(LineKind::Out, "container: ok".to_string());
        app.push_line(LineKind::Err, "warning: x".to_string());
        let screen = render(&app, 80, 24);
        assert!(screen.contains("en cours…"));
        assert!(screen.contains("container: ok") && screen.contains("! warning: x"));
        let b = buffer(&app, 80, 24);
        assert_eq!(
            fg_of(&b, 4, "! warning"),
            Some(Color::Reset),
            "stderr is not coloured"
        );
        app.finished(Outcome::Failed(Some(3)));
        let screen = render(&app, 80, 24);
        assert!(screen.contains("✗ échec (code 3)"));
        let b = buffer(&app, 80, 24);
        assert_eq!(fg_of(&b, 21, "✗ échec"), Some(BAD));
        app.finished(Outcome::Success);
        assert!(render(&app, 80, 24).contains("✓ terminé"));
        let b = buffer(&app, 80, 24);
        assert_eq!(fg_of(&b, 21, "✓ terminé"), Some(GOOD));
    }

    #[test]
    fn longest_labels_and_paths_fit_at_80_columns() {
        let mut app = App::new(snap());
        key(&mut app, KeyCode::Down);
        key(&mut app, KeyCode::Enter); // Mises à jour
        let screen = render(&app, 80, 24);
        assert!(
            screen.contains("▸ Vérifier s'il y a des mises à jour"),
            "{screen}"
        );
        assert!(text::truncate_left("zevbox › Connexion des agents › x", 20).starts_with('…'));
    }

    #[test]
    fn output_body_size_matches_the_drawn_panel() {
        assert_eq!(output_body_size(79, 24), (0, 0));
        // 80 - 40 (left) - 2 borders; 24 - banner - keys - 2 borders - status.
        assert_eq!(output_body_size(80, 24), (38, 19));
    }

    #[test]
    fn dash_error_and_first_login_note_are_shown() {
        let mut s = snap();
        s.agents.iter_mut().for_each(|a| a.version = None);
        let mut app = App::new(s);
        key(&mut app, KeyCode::Down);
        key(&mut app, KeyCode::Down);
        key(&mut app, KeyCode::Enter);
        key(&mut app, KeyCode::Down); // Se connecter à Claude
        let screen = render(&app, 100, 40);
        assert!(screen.contains("Pas encore installé"), "{screen}");
        let _ = app.open_action(Action::ToolsAdd);
        key(&mut app, KeyCode::Char('-'));
        key(&mut app, KeyCode::Enter);
        let screen = render(&app, 80, 24);
        assert!(
            screen.contains("ne peut pas commencer par « - »"),
            "{screen}"
        );
    }
}
