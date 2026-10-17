//! Urdu / Arabic display order. egui 0.37 shapes text with harfrust and draws each
//! same-font run in its script's direction, so one Urdu word comes out joined and
//! right-to-left; but runs break at every space (spaces come from the Latin font), so the
//! *order of words* in a phrase, and of RTL/LTR runs in a line, is left to us.
//!
//! shortcut: a simplified Unicode Bidi Algorithm (per line, neutrals take their neighbours'
//! direction); upgrade to `unicode-bidi` if nested mixed-direction text must be exact.

use std::borrow::Cow;

pub fn is_rtl(c: char) -> bool {
    matches!(c as u32, 0x0590..=0x05FF | 0x0600..=0x065F | 0x066A..=0x06EF | 0x06FA..=0x08FF | 0xFB1D..=0xFDFF | 0xFE70..=0xFEFF)
}

/// Harakat and other combining marks: part of the word they sit on.
fn mark(c: char) -> bool {
    matches!(c as u32, 0x0610..=0x061A | 0x064B..=0x065F | 0x0670 | 0x06D6..=0x06ED)
}

fn script(c: char) -> bool {
    is_rtl(c) || mark(c)
}

fn mirror(c: char) -> char {
    match c {
        '(' => ')',
        ')' => '(',
        '[' => ']',
        ']' => '[',
        '{' => '}',
        '}' => '{',
        '<' => '>',
        '>' => '<',
        '«' => '»',
        '»' => '«',
        _ => c,
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Dir {
    L,
    R,
    N,
}

fn class(c: char) -> Dir {
    if script(c) {
        Dir::R
    } else if c.is_alphanumeric() {
        Dir::L
    } else {
        Dir::N
    }
}

/// A right-to-left run with its words in visual order; each word keeps its logical letter
/// order because egui's shaper reverses it.
fn rtl_run(chars: &[char]) -> Vec<char> {
    let mut tokens: Vec<(bool, Vec<char>)> = vec![];
    for &c in chars {
        let s = script(c);
        match tokens.last_mut() {
            Some((ts, t)) if *ts == s => t.push(c),
            _ => tokens.push((s, vec![c])),
        }
    }
    tokens.into_iter().rev().flat_map(|(s, t)| if s { t } else { t.into_iter().rev().map(mirror).collect() }).collect()
}

/// Visual order of one line.
fn reorder(line: &[char]) -> Vec<char> {
    let mut dirs: Vec<Dir> = line.iter().map(|&c| class(c)).collect();
    let para = dirs.iter().find(|d| **d != Dir::N).copied().unwrap_or(Dir::L);
    let n = dirs.len();
    let mut i = 0;
    while i < n {
        if dirs[i] != Dir::N {
            i += 1;
            continue;
        }
        let start = i;
        while i < n && dirs[i] == Dir::N {
            i += 1;
        }
        let before = if start == 0 { para } else { dirs[start - 1] };
        let after = if i == n { para } else { dirs[i] };
        dirs[start..i].fill(if before == after { before } else { para });
    }
    let mut runs: Vec<(Dir, Vec<char>)> = vec![];
    for (&c, &d) in line.iter().zip(&dirs) {
        match runs.last_mut() {
            Some((rd, chars)) if *rd == d => chars.push(c),
            _ => runs.push((d, vec![c])),
        }
    }
    if para == Dir::R {
        runs.reverse();
    }
    runs.into_iter().flat_map(|(d, chars)| if d == Dir::R { rtl_run(&chars) } else { chars }).collect()
}

/// Text in the order egui should draw it. Borrowed (no work) when there is no RTL text.
/// Always a permutation of the input chars, so editors' cursor indices stay valid.
pub fn visual(text: &str) -> Cow<'_, str> {
    if !text.chars().any(is_rtl) {
        return Cow::Borrowed(text);
    }
    let lines: Vec<String> = text
        .split('\n')
        .map(|line| {
            let chars: Vec<char> = line.chars().collect();
            if chars.iter().any(|&c| is_rtl(c)) { reorder(&chars).into_iter().collect() } else { line.to_string() }
        })
        .collect();
    Cow::Owned(lines.join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn latin_is_untouched() {
        assert!(matches!(visual("GET /users {{id}}"), Cow::Borrowed(_)));
    }

    #[test]
    fn single_word_is_left_to_the_shaper() {
        assert_eq!(visual("پاکستان"), "پاکستان");
    }

    #[test]
    fn phrase_words_are_reversed_inside_ltr_line() {
        assert_eq!(visual("  \"author\": \"محمد حسین آزاد\","), "  \"author\": \"آزاد حسین محمد\",");
    }

    #[test]
    fn rtl_line_puts_latin_on_the_left() {
        // "نام: Ali" -> Ali first, then the mirrored ": " and the Urdu word
        assert_eq!(visual("نام: Ali"), "Ali :نام");
        assert_eq!(visual("اردو (کتابیں)"), "(کتابیں) اردو");
    }

    #[test]
    fn output_is_a_permutation() {
        let s = "یہ {{متغیرات}} کہیں بھی متعین نہیں تھے";
        let mut a: Vec<char> = s.chars().map(mirror).collect();
        let mut b: Vec<char> = visual(s).chars().map(mirror).collect();
        a.sort();
        b.sort();
        assert_eq!(a, b);
    }
}
