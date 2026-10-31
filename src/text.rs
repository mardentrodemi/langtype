//! Character equivalence, line preparation, and wrapping.

pub const SKIP_PUNCT: &str = ".,!?:;'\"«»„“”‘’‹›…";

const DASHES: &str = "‐‑‒–—―−﹣－";
const QUOTES: &str = "“”«»„‟";

fn canon(ch: char) -> char {
    match ch {
        'ё' => 'е',
        'Ё' => 'Е',
        c if DASHES.contains(c) => '-',
        c if QUOTES.contains(c) => '"',
        c => c,
    }
}

/// Yo/ye, dashes, and quotation marks are the same letter for scoring.
pub fn chars_match(typed: char, expected: char) -> bool {
    typed == expected || canon(typed) == canon(expected)
}

pub fn count_correct(target: &str, typed: &str, ignored: &[bool]) -> u64 {
    target
        .chars()
        .zip(typed.chars())
        .enumerate()
        .filter(|(index, (expected, typed))| {
            ignored.get(*index) != Some(&true) && chars_match(*typed, *expected)
        })
        .count() as u64
}

pub fn strip_punctuation(text: &str) -> String {
    let stripped: String = text
        .chars()
        .filter(|ch| !SKIP_PUNCT.contains(*ch))
        .collect();
    stripped.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub fn prepare_line(text: &str, hide_punctuation: bool, no_capitals: bool) -> String {
    let mut text = if hide_punctuation {
        strip_punctuation(text)
    } else {
        text.to_string()
    };
    if no_capitals {
        text = text.to_lowercase();
    }
    text
}

/// Wrap `text` into lines of at most `width` characters, breaking on spaces.
/// Each line is a list of character indexes into `text`.
pub fn wrap_indices(text: &str, width: usize) -> Vec<Vec<usize>> {
    let width = width.max(8);
    let chars: Vec<char> = text.chars().collect();
    if chars.is_empty() {
        return vec![Vec::new()];
    }
    let mut lines = Vec::new();
    let mut start = 0;
    while start < chars.len() {
        let mut chunk = width.min(chars.len() - start);
        if start + chunk < chars.len() {
            let end = (start + chunk + 1).min(chars.len());
            if let Some(rel) = chars[start..end].iter().rposition(|c| *c == ' ') {
                chunk = rel + 1;
            } else if chars[start] == ' ' {
                chunk = 1;
            }
        }
        if chunk == 0 {
            chunk = 1;
        }
        lines.push((start..start + chunk).collect());
        start += chunk;
    }
    lines
}

/// A space that sits at the end of a wrapped line. The line break stands in for it.
pub fn is_wrap_space(text: &str, index: usize, width: usize) -> bool {
    let chars: Vec<char> = text.chars().collect();
    if chars.get(index) != Some(&' ') {
        return false;
    }
    wrap_indices(text, width)
        .iter()
        .any(|line| line.last().copied() == Some(index))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn yo_dash_and_quotes_match() {
        assert!(chars_match('ё', 'е'));
        assert!(chars_match('Ё', 'Е'));
        assert!(chars_match('—', '-'));
        assert!(chars_match('«', '"'));
        assert!(!chars_match('а', 'о'));
    }

    #[test]
    fn wrap_breaks_on_space() {
        let text = "one two three";
        let lines = wrap_indices(text, 7);
        let rendered: Vec<String> = lines
            .iter()
            .map(|idxs| idxs.iter().map(|&i| text.chars().nth(i).unwrap()).collect())
            .collect();
        assert_eq!(rendered, vec!["one two ".to_string(), "three".to_string()]);
    }

    #[test]
    fn wrap_space_is_only_the_break_at_the_end_of_a_line() {
        let text = "one two three";
        assert!(is_wrap_space(text, 7, 8));
        assert!(!is_wrap_space(text, 3, 8));
        assert!(!is_wrap_space(text, 7, 80));
    }
}
