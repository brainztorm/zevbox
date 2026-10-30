//! Text measured in terminal columns (`unicode-width`), not in chars: wrapping
//! of command output, truncation of titles and list items, horizontal scroll of
//! text fields. Pure and shared by `app.rs` (scroll bounds) and `ui.rs` (drawing).

use unicode_width::UnicodeWidthChar;

/// Columns taken by one character (control characters count as 0).
pub fn char_width(c: char) -> usize {
    c.width().unwrap_or(0)
}

/// Columns taken by `text`.
pub fn width(text: &str) -> usize {
    text.chars().map(char_width).sum()
}

/// Cuts `text` into rows of at most `max` columns (a wide character that does
/// not fit goes to the next row). Always at least one row.
pub fn wrap(text: &str, max: usize) -> Vec<String> {
    let max = max.max(1);
    let mut rows = Vec::new();
    let mut row = String::new();
    let mut used = 0;
    for c in text.chars() {
        let w = char_width(c);
        if used + w > max && !row.is_empty() {
            rows.push(std::mem::take(&mut row));
            used = 0;
        }
        row.push(c);
        used += w;
    }
    rows.push(row);
    rows
}

/// Cuts `text` into rows of at most `max` columns at spaces (prose of the
/// description panel); a word longer than a row is cut by columns. Leading
/// spaces (list indentation) are kept. Always at least one row.
pub fn wrap_words(text: &str, max: usize) -> Vec<String> {
    let max = max.max(1);
    let mut rows = Vec::new();
    let mut current: Option<String> = None;
    for word in text.split(' ') {
        let candidate = match &current {
            None => word.to_string(),
            Some(line) => format!("{line} {word}"),
        };
        if width(&candidate) <= max {
            current = Some(candidate);
            continue;
        }
        if let Some(line) = current.take() {
            rows.push(line);
        }
        let mut pieces = wrap(word, max);
        let last = pieces.pop().unwrap_or_default();
        rows.extend(pieces);
        current = Some(last);
    }
    rows.push(current.unwrap_or_default());
    rows
}

/// Screen rows taken by `lines` wrapped at `max` columns.
pub fn wrapped_rows<'a>(lines: impl Iterator<Item = &'a str>, max: usize) -> usize {
    lines.map(|l| wrap(l, max).len()).sum()
}

/// Keeps the end of `text` within `max` columns, with « … » in front if cut
/// (paths: the deepest part is the useful one).
pub fn truncate_left(text: &str, max: usize) -> String {
    if width(text) <= max {
        return text.to_string();
    }
    let mut kept = Vec::new();
    let mut used = 1; // « … »
    for c in text.chars().rev() {
        let w = char_width(c);
        if used + w > max {
            break;
        }
        kept.push(c);
        used += w;
    }
    std::iter::once('…').chain(kept.into_iter().rev()).collect()
}

/// Keeps the start of `text` within `max` columns, with « … » at the end if cut.
pub fn truncate_right(text: &str, max: usize) -> String {
    if width(text) <= max {
        return text.to_string();
    }
    let mut out = String::new();
    let mut used = 1;
    for c in text.chars() {
        let w = char_width(c);
        if used + w > max {
            break;
        }
        out.push(c);
        used += w;
    }
    out.push('…');
    out
}

/// First character shown in a field of `room` columns so that the cursor (an
/// index in `chars`) stays visible: the columns between them fit in `room`.
pub fn scroll_start(chars: &[char], cursor: usize, room: usize) -> usize {
    let cursor = cursor.min(chars.len());
    let mut start = cursor;
    let mut used = 0;
    while start > 0 {
        let w = char_width(chars[start - 1]);
        // Keep one column for the cursor itself.
        if used + w + 1 > room {
            break;
        }
        used += w;
        start -= 1;
    }
    start
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wrap_uses_columns() {
        assert_eq!(wrap("abcdef", 4), ["abcd", "ef"]);
        assert_eq!(wrap("", 4), [""]);
        // Wide characters take two columns each.
        assert_eq!(wrap("日本語", 4), ["日本", "語"]);
        assert_eq!(wrapped_rows(["abcdef", "", "日本語"].into_iter(), 4), 5);
    }

    #[test]
    fn wrap_words_cuts_at_spaces() {
        assert_eq!(
            wrap_words("Arrête tous les agents zevbox", 12),
            ["Arrête tous", "les agents", "zevbox"]
        );
        assert_eq!(
            wrap_words("  · Voir les agents", 40),
            ["  · Voir les agents"]
        );
        assert_eq!(wrap_words("abcdefghij xy", 4), ["abcd", "efgh", "ij", "xy"]);
        assert_eq!(wrap_words("", 4), [""]);
        for row in wrap_words("Une phrase un peu longue pour tester la largeur", 10) {
            assert!(width(&row) <= 10, "{row}");
        }
    }

    #[test]
    fn truncation_marks_the_cut() {
        assert_eq!(
            truncate_left("zevbox › Mises à jour", 30),
            "zevbox › Mises à jour"
        );
        assert_eq!(truncate_left("abcdefgh", 5), "…efgh");
        assert_eq!(truncate_right("abcdefgh", 5), "abcd…");
        assert_eq!(width(&truncate_right("日本語日本語", 5)), 5);
    }

    #[test]
    fn scroll_start_keeps_the_cursor_visible() {
        let chars: Vec<char> = "abcdefghij".chars().collect();
        assert_eq!(scroll_start(&chars, 3, 8), 0);
        assert_eq!(scroll_start(&chars, 10, 5), 6);
        let wide: Vec<char> = "日本語日本語".chars().collect();
        // 5 columns: 2 wide characters + the cursor.
        assert_eq!(scroll_start(&wide, 6, 5), 4);
    }
}
