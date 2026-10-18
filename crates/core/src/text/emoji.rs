//! Emoji sequences (Unicode Technical Standard #51): which characters
//! draw as one emoji, so that itemisation keeps them in one font and the
//! font's ligatures (flags, keycaps, skin tones, ZWJ sequences) can form.
//!
//! The scanner follows the "possible emoji" grammar of UTS #51 (ED-27):
//! elements joined by U+200D ZERO WIDTH JOINER, where an element is a pair
//! of regional indicators (a flag), a keycap (`[0-9#*] FE0F? 20E3`), or an
//! emoji character followed by a presentation selector, a skin tone
//! modifier or a tag sequence. It does not check that a sequence is one
//! the standard recommends: fonts decide what they draw.

use std::ops::Range;

use unicode_properties::{EmojiStatus, UnicodeEmoji};

use crate::fonts::{FontKey, FontSet};

pub(super) const ZWJ: char = '\u{200D}';
const VS15: char = '\u{FE0E}';
pub(super) const VS16: char = '\u{FE0F}';
const KEYCAP: char = '\u{20E3}';
const CANCEL_TAG: char = '\u{E007F}';

/// One emoji sequence of a text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Sequence {
    /// Byte range in the text scanned.
    pub range: Range<usize>,
    /// The first character: the one a font must have to draw it.
    pub base: char,
    /// Drawn as an emoji (colour), not as text: `FE0F`, a skin tone, a
    /// tag sequence, a keycap, a flag, a ZWJ sequence, or a character
    /// whose default presentation is emoji, without `FE0E`.
    pub emoji: bool,
}

fn emoji_presentation(c: char) -> bool {
    matches!(
        c.emoji_status(),
        EmojiStatus::EmojiPresentation
            | EmojiStatus::EmojiPresentationAndModifierBase
            | EmojiStatus::EmojiPresentationAndEmojiComponent
            | EmojiStatus::EmojiPresentationAndModifierAndEmojiComponent
    )
}

fn is_modifier(c: char) -> bool {
    matches!(c, '\u{1F3FB}'..='\u{1F3FF}')
}

fn is_regional_indicator(c: char) -> bool {
    matches!(c, '\u{1F1E6}'..='\u{1F1FF}')
}

fn is_tag_spec(c: char) -> bool {
    matches!(c, '\u{E0020}'..='\u{E007E}')
}

/// The element starting at `chars[i]`: its length in characters, and
/// whether it asks for emoji presentation.
fn element(chars: &[(usize, char)], i: usize) -> Option<(usize, bool)> {
    let at = |k: usize| chars.get(k).map(|&(_, c)| c);
    let c = at(i)?;
    if is_regional_indicator(c) {
        return Some(match at(i + 1) {
            Some(n) if is_regional_indicator(n) => (2, true),
            _ => (1, true),
        });
    }
    if c.is_ascii_digit() || c == '#' || c == '*' {
        return match (at(i + 1), at(i + 2)) {
            (Some(VS16), Some(KEYCAP)) => Some((3, true)),
            (Some(KEYCAP | VS16), _) => Some((2, true)),
            _ => None,
        };
    }
    if !c.is_emoji_char() || is_tag_spec(c) || c == CANCEL_TAG {
        return None;
    }
    let mut len = 1;
    let mut emoji = emoji_presentation(c);
    match at(i + 1) {
        Some(VS16) => {
            emoji = true;
            len += 1;
        }
        Some(VS15) => {
            emoji = false;
            len += 1;
        }
        Some(m) if is_modifier(m) => {
            emoji = true;
            len += 1;
        }
        _ => {}
    }
    let tags = chars[i + len..]
        .iter()
        .take_while(|&&(_, t)| is_tag_spec(t))
        .count();
    if tags > 0 && at(i + len + tags) == Some(CANCEL_TAG) {
        emoji = true;
        len += tags + 1;
    }
    Some((len, emoji))
}

/// The emoji sequences of `text`, in order. Text presentation sequences
/// (`©`, `☺︎`) are sequences too: they still keep their selector with
/// their base.
pub(crate) fn sequences(text: &str) -> Vec<Sequence> {
    // Every emoji sequence has a character outside ASCII (a keycap ends in
    // U+20E3, or has U+FE0F).
    if text.is_ascii() {
        return Vec::new();
    }
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    let end_of = |k: usize| chars.get(k).map_or(text.len(), |&(b, _)| b);
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let Some((len, mut emoji)) = element(&chars, i) else {
            i += 1;
            continue;
        };
        let mut j = i + len;
        while chars.get(j).is_some_and(|&(_, c)| c == ZWJ) {
            let Some((next, _)) = element(&chars, j + 1) else {
                break;
            };
            emoji = true;
            j += 1 + next;
        }
        out.push(Sequence {
            range: chars[i].0..end_of(j),
            base: chars[i].1,
            emoji,
        });
        i = j;
    }
    out
}

/// The font for sequence `seq` of a run in font `primary`. For emoji
/// presentation: the first colour fallback font that has the base, then
/// the run's font, then the other fallbacks. For text presentation: the
/// run's font, then the fallbacks, as for any character. A font that has
/// none of it falls back to the run's font (`.notdef`).
pub(crate) fn font(seq: &Sequence, primary: FontKey, fonts: &FontSet<'_>) -> FontKey {
    let has = |f: FontKey| fonts.get(f).glyph(seq.base).is_some();
    let color = |f: FontKey| fonts.get(f).has_color();
    let fallback = |want: &dyn Fn(FontKey) -> bool| fonts.fallback(|f| want(f) && has(f));
    let first = if seq.emoji { fallback(&color) } else { None };
    // A deferred run font that is not registered keeps the sequence, as in `prepare.rs`.
    let deferred = fonts.get(primary).is_deferred();
    first
        .or_else(|| (has(primary) || deferred).then_some(primary))
        .or_else(|| fallback(&|_| true))
        .unwrap_or(primary)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `(text, emoji)` of each sequence of `s`.
    fn scan(s: &str) -> Vec<(&str, bool)> {
        sequences(s)
            .into_iter()
            .map(|q| (&s[q.range], q.emoji))
            .collect()
    }

    #[test]
    fn plain_text_has_no_sequences() {
        assert_eq!(scan("Hello, world 123 #1 *"), []);
        assert_eq!(scan("Grüße — ÀÉ"), []);
    }

    #[test]
    fn single_emoji_and_presentation_selectors() {
        assert_eq!(scan("a😀b"), [("😀", true)]);
        // Text by default: emoji with FE0F, text without or with FE0E.
        assert_eq!(scan("❤"), [("❤", false)]);
        assert_eq!(scan("❤\u{FE0F}"), [("❤\u{FE0F}", true)]);
        assert_eq!(scan("☺\u{FE0E}"), [("☺\u{FE0E}", false)]);
        assert_eq!(scan("😀\u{FE0E}"), [("😀\u{FE0E}", false)]);
        assert_eq!(scan("©"), [("©", false)]);
    }

    #[test]
    fn skin_tones_flags_keycaps_and_tags() {
        assert_eq!(scan("👍🏽"), [("👍🏽", true)]);
        // A lone modifier is an emoji of its own.
        assert_eq!(scan("🏽"), [("🏽", true)]);
        assert_eq!(scan("🇫🇷🇩🇪"), [("🇫🇷", true), ("🇩🇪", true)]);
        // A lone regional indicator.
        assert_eq!(scan("🇫x"), [("🇫", true)]);
        assert_eq!(
            scan("1\u{FE0F}\u{20E3} #\u{20E3} 9"),
            [("1\u{FE0F}\u{20E3}", true), ("#\u{20E3}", true)]
        );
        let england = "🏴\u{E0067}\u{E0062}\u{E0065}\u{E006E}\u{E0067}\u{E007F}";
        assert_eq!(scan(england), [(england, true)]);
        // Tags without their terminator are left out.
        assert_eq!(scan("🏴\u{E0067}\u{E0062}"), [("🏴", true)]);
    }

    #[test]
    fn zwj_sequences() {
        let coder = "👩\u{200D}💻";
        assert_eq!(scan(coder), [(coder, true)]);
        let family = "👨\u{200D}👩\u{200D}👧\u{200D}👦";
        assert_eq!(scan(&format!("x{family}y")), [(family, true)]);
        let toned = "👩🏽\u{200D}💻";
        assert_eq!(scan(toned), [(toned, true)]);
        // A trailing or lone ZWJ is not part of a sequence.
        assert_eq!(scan("😀\u{200D}"), [("😀", true)]);
        assert_eq!(scan("\u{200D}"), []);
        assert_eq!(scan("😀\u{200D}a"), [("😀", true)]);
        // Text presentation joined to an emoji asks for emoji.
        assert_eq!(scan("❤\u{200D}🔥"), [("❤\u{200D}🔥", true)]);
    }

    #[test]
    fn ranges_are_byte_offsets() {
        let s = "ab 👍🏽 c 🇫🇷";
        let q = sequences(s);
        assert_eq!(q.len(), 2);
        assert_eq!(&s[q[0].range.clone()], "👍🏽");
        assert_eq!(q[0].base, '👍');
        assert_eq!(&s[q[1].range.clone()], "🇫🇷");
        assert_eq!(q[1].base, '🇫');
    }
}
