//! Hyphenation behind a trait (§5.1), so pattern sources can change
//! without touching line layout.

/// Finds hyphenation points in a word.
pub(crate) trait Hyphenator {
    /// Whether patterns exist for `lang` (an ISO 639-1 code, lowercase).
    fn supports(&self, lang: &str) -> bool;
    /// Byte offsets inside `word` where a hyphen may be inserted.
    fn hyphenate(&self, word: &str, lang: &str) -> Vec<usize>;
}

/// `hypher`: embedded TeX patterns for ~50 languages (no Romanian, see
/// ADR-015).
pub(crate) struct HypherHyphenator;

fn lang_of(code: &str) -> Option<hypher::Lang> {
    let b = code.as_bytes();
    if b.len() != 2 {
        return None;
    }
    hypher::Lang::from_iso([b[0], b[1]])
}

impl Hyphenator for HypherHyphenator {
    fn supports(&self, lang: &str) -> bool {
        lang_of(lang).is_some()
    }

    fn hyphenate(&self, word: &str, lang: &str) -> Vec<usize> {
        let Some(l) = lang_of(lang) else {
            return Vec::new();
        };
        let mut offsets = Vec::new();
        let mut pos = 0;
        for syllable in hypher::hyphenate(word, l) {
            pos += syllable.len();
            if pos < word.len() {
                offsets.push(pos);
            }
        }
        offsets
    }
}

/// Never hyphenates.
#[allow(dead_code)]
pub(crate) struct NoHyphenation;

impl Hyphenator for NoHyphenation {
    fn supports(&self, _: &str) -> bool {
        false
    }

    fn hyphenate(&self, _: &str, _: &str) -> Vec<usize> {
        Vec::new()
    }
}

/// Normalises a BCP-47 tag (`ro`, `en-GB`) to its lowercase ISO 639-1
/// language code.
pub(crate) fn language_code(tag: &str) -> Option<String> {
    let b = tag.as_bytes();
    if b.len() < 2 || (b.len() > 2 && b[2] != b'-' && b[2] != b'_') {
        return None;
    }
    Some(tag[..2].to_ascii_lowercase())
}
