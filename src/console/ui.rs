//! Drawing of `zevbox console` (docs/SPEC_console.md §3): banner on top, menu on
//! the left, description or command output on the right, valid keys at the
//! bottom, and the boxes (text input, « Êtes-vous sûr ? ») over them. Pure: reads
//! the [`App`], draws into a ratatui `Frame` (tested with `TestBackend`).

use super::app::{App, LineKind, Output, RunStatus, View};
use super::menu::{Action, Entry, Params};
use super::state::Snapshot;
use super::text;
use ratatui::layout::{Constraint, Layout, Position, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, Paragraph, Wrap};
use ratatui::Frame;

pub const MIN_WIDTH: u16 = 80;
pub const MIN_HEIGHT: u16 = 20;
pub const TOO_SMALL: &str = "Agrandissez la fenêtre (80 × 20 minimum)";
/// Fits « ▸ Vérifier s'il y a des mises à jour » and leaves 40 columns on the
/// right at the 80-column minimum.
const LEFT_WIDTH: u16 = 40;

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
/// terminal is too small to draw anything.
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

    frame.render_widget(
        Paragraph::new(format!(" {}", app.snap.banner()))
            .style(Style::new().add_modifier(Modifier::REVERSED | Modifier::BOLD)),
        top,
    );
    draw_left(frame, app, left);
    match &app.view {
        View::Output(o) => draw_output(frame, o, right),
        View::Wizard(w) => {
            draw_description(frame, right, &leaf_lines(w.action, &w.params, &app.snap))
        }
        View::Menu => {
            let lines = match app.selected_entry() {
                Some(Entry::Leaf(a)) => leaf_lines(*a, &Params::default(), &app.snap),
                Some(Entry::Menu(m)) => {
                    let mut lines = title_and_text(m.label, m.description);
                    lines.push(Line::default());
                    lines.push(Line::from("Contient :"));
                    lines.extend(
                        m.children
                            .iter()
                            .map(|c| Line::from(format!("  · {}", c.label()))),
                    );
                    lines
                }
                None => Vec::new(),
            };
            draw_description(frame, right, &lines);
        }
    }
    frame.render_widget(
        Paragraph::new(format!(" {}", app.keys_hint())).style(Style::new().fg(Color::DarkGray)),
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

fn draw_left(frame: &mut Frame, app: &App, area: Rect) {
    // The deepest part of the path is the useful one: cut from the left.
    let room = usize::from(area.width.saturating_sub(4));
    let block = Block::bordered().title(format!(
        " {} ",
        text::truncate_left(&app.path_title(), room)
    ));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let selected = Style::new().add_modifier(Modifier::REVERSED);
    let (heading, items, cursor, empty): (Option<&str>, Vec<String>, usize, &str) = match &app.view
    {
        View::Wizard(w) if w.picker.is_some() => match &w.picker {
            Some(p) => (
                Some(p.title),
                p.choices.iter().map(|c| c.label.clone()).collect(),
                p.cursor,
                p.empty_message,
            ),
            None => (None, Vec::new(), 0, ""),
        },
        _ => (
            None,
            app.current_menu()
                .children
                .iter()
                .map(|e| match e {
                    Entry::Menu(m) => format!("{} ›", m.label),
                    Entry::Leaf(a) => a.label().to_string(),
                })
                .collect(),
            app.cursor(),
            "",
        ),
    };
    let mut lines = Vec::new();
    if let Some(h) = heading {
        lines.push(Line::styled(h, Style::new().add_modifier(Modifier::BOLD)));
        lines.push(Line::default());
    }
    if items.is_empty() {
        lines.push(Line::from(empty));
    }
    let room = usize::from(inner.height).saturating_sub(lines.len()).max(1);
    let first = cursor.saturating_sub(room - 1);
    let columns = usize::from(inner.width).saturating_sub(2);
    for (i, item) in items.iter().enumerate().skip(first).take(room) {
        let item = text::truncate_right(item, columns);
        lines.push(if i == cursor {
            Line::styled(format!("▸ {item}"), selected)
        } else {
            Line::from(format!("  {item}"))
        });
    }
    frame.render_widget(Paragraph::new(lines), inner);
}

fn title_and_text(title: &str, text: &str) -> Vec<Line<'static>> {
    vec![
        Line::styled(title.to_string(), Style::new().add_modifier(Modifier::BOLD)),
        Line::default(),
        Line::from(text.to_string()),
    ]
}

/// Title, plain description, duration/network, `Commande : zevbox …` (§3).
fn leaf_lines(action: Action, params: &Params, snap: &Snapshot) -> Vec<Line<'static>> {
    let mut lines = title_and_text(action.label(), action.description());
    if let Some(cost) = action.cost() {
        lines.push(Line::default());
        lines.push(Line::styled(cost, Style::new().fg(Color::Cyan)));
    }
    if let Some(note) = action.cost_note(snap) {
        lines.push(Line::styled(note, Style::new().fg(Color::Yellow)));
    }
    lines.push(Line::default());
    lines.push(Line::from(vec![
        Span::styled("Commande : ", Style::new().add_modifier(Modifier::BOLD)),
        Span::raw(action.preview(params)),
    ]));
    lines
}

fn draw_description(frame: &mut Frame, area: Rect, lines: &[Line<'static>]) {
    frame.render_widget(
        Paragraph::new(lines.to_vec())
            .wrap(Wrap { trim: false })
            .block(Block::bordered()),
        area,
    );
}

fn draw_output(frame: &mut Frame, o: &Output, area: Rect) {
    let (status, status_style) = match o.status {
        RunStatus::Running {
            interrupting: false,
        } => ("en cours…".to_string(), Style::new().fg(Color::Yellow)),
        RunStatus::Running { interrupting: true } => {
            ("interruption…".to_string(), Style::new().fg(Color::Yellow))
        }
        RunStatus::Done(outcome) => (
            outcome.label(),
            Style::new().fg(if outcome.is_success() {
                Color::Green
            } else {
                Color::Red
            }),
        ),
    };
    let block = Block::bordered().title(format!(" {} ", o.title));
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
            let style = match kind {
                LineKind::Out => Style::new(),
                LineKind::Err => Style::new().fg(Color::Yellow),
                LineKind::Info => Style::new().fg(Color::Cyan),
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
            Span::styled(scrolled, Style::new().fg(Color::DarkGray)),
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
    let block = Block::bordered().title(format!(" {} ", text.field.prompt()));
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
        Some(error) => Line::styled(error, Style::new().fg(Color::Red)),
        None => Line::styled(hint, Style::new().fg(Color::DarkGray)),
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

fn draw_confirm(frame: &mut Frame, dialog: &super::app::ConfirmDialog, area: Rect) {
    let rect = popup(area, 62, 9);
    frame.render_widget(Clear, rect);
    let block = Block::bordered()
        .title(" Êtes-vous sûr ? ")
        .border_style(Style::new().fg(Color::Yellow));
    let inner = block.inner(rect);
    frame.render_widget(block, rect);
    let button = |label: &'static str, on: bool| {
        if on {
            Span::styled(
                label,
                Style::new().add_modifier(Modifier::REVERSED | Modifier::BOLD),
            )
        } else {
            Span::raw(label)
        }
    };
    let lines = vec![
        Line::styled(
            dialog.confirmation.question.clone(),
            Style::new().add_modifier(Modifier::BOLD),
        ),
        Line::default(),
        Line::from(dialog.confirmation.consequence.clone()),
    ];
    let [text, buttons] =
        Layout::vertical([Constraint::Min(0), Constraint::Length(1)]).areas(inner);
    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), text);
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
    use crate::console::app::App;
    use crate::console::state::{AgentState, Snapshot};
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    fn snap() -> Snapshot {
        Snapshot {
            installed: true,
            agents: crate::agents::AGENTS
                .iter()
                .map(|a| AgentState {
                    id: a.id,
                    display: a.display,
                    version: Some("0.99.1".to_string()),
                    has_previous: false,
                    authenticated: false,
                })
                .collect(),
            ..Snapshot::default()
        }
    }

    fn render(app: &App, width: u16, height: u16) -> String {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        terminal.draw(|f| draw(f, app)).unwrap();
        let buffer = terminal.backend().buffer();
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

    #[test]
    fn screen_has_banner_two_panels_and_keys() {
        let app = App::new(snap());
        let screen = render(&app, 80, 24);
        let lines: Vec<&str> = screen.lines().collect();
        assert!(lines[0].contains("zevbox ─ Claude 0.99.1 · Antigravity 0.99.1"));
        assert!(
            screen.contains("▸ Agents en cours ›"),
            "left panel:\n{screen}"
        );
        assert!(screen.contains("Première installation"));
        assert!(screen.contains("Contient :"), "right panel:\n{screen}");
        assert!(lines[23].contains("↑↓ choisir"), "key line:\n{screen}");
        assert!(lines[23].contains("q quitter"));
        assert!(lines[1].starts_with("┌ zevbox "), "path title:\n{screen}");
    }

    #[test]
    fn leaf_shows_its_command() {
        let mut app = App::new(snap());
        key(&mut app, KeyCode::Enter);
        let screen = render(&app, 80, 24);
        assert!(screen.contains("zevbox › Agents en cours"), "{screen}");
        assert!(screen.contains("Commande : zevbox status"), "{screen}");
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
    fn confirm_box_shows_question_consequence_and_buttons() {
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
    fn output_shows_lines_and_result() {
        let mut app = App::new(snap());
        let _ = app.open_action(Action::Doctor);
        app.push_line(LineKind::Info, "$ zevbox doctor".to_string());
        app.push_line(LineKind::Out, "container: ok".to_string());
        app.push_line(LineKind::Err, "warning: x".to_string());
        let screen = render(&app, 80, 24);
        assert!(screen.contains("en cours…"));
        assert!(screen.contains("container: ok") && screen.contains("warning: x"));
        app.finished(crate::console::app::Outcome::Failed(Some(3)));
        assert!(render(&app, 80, 24).contains("✗ échec (code 3)"));
        app.finished(crate::console::app::Outcome::Success);
        assert!(render(&app, 80, 24).contains("✓ terminé"));
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
        // A deep path is cut from the left, the leaf stays readable.
        let _ = app.open_action(Action::UpdateAgent);
        let screen = render(&app, 80, 24);
        let title = screen.lines().nth(1).unwrap_or_default();
        assert!(title.contains("┌ …"), "{title}");
        assert!(title.contains("› Mettre à jour un agent ┐"), "{title}");
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
        let screen = render(&app, 100, 30);
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
