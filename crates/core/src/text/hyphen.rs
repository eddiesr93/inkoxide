//! Hyphenation behind a trait, so pattern sources can change
//! without touching line layout. Patterns registered at runtime
//! ([`HyphenationStore`]) win over hypher's built-in ones.

use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;

use crate::InkoxideError;

/// Finds hyphenation points in a word.
pub(crate) trait Hyphenator: Send + Sync {
    /// Whether patterns exist for `lang` (an ISO 639-1 code, lowercase).
    fn supports(&self, lang: &str) -> bool;
    /// Byte offsets inside `word` where a hyphen may be inserted.
    fn hyphenate(&self, word: &str, lang: &str) -> Vec<usize>;
}

/// `hypher`: embedded TeX patterns for ~50 languages.
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
#[cfg(test)]
pub(crate) struct NoHyphenation;

#[cfg(test)]
impl Hyphenator for NoHyphenation {
    fn supports(&self, _: &str) -> bool {
        false
    }

    fn hyphenate(&self, _: &str, _: &str) -> Vec<usize> {
        Vec::new()
    }
}

/// Normalises a BCP-47 tag (`en`, `en-GB`) to its lowercase ISO 639-1
/// language code.
pub(crate) fn language_code(tag: &str) -> Option<String> {
    let b = tag.as_bytes();
    if b.len() < 2 || (b.len() > 2 && b[2] != b'-' && b[2] != b'_') {
        return None;
    }
    Some(tag[..2].to_ascii_lowercase())
}

/// Hyphenation patterns registered at runtime, by language.
///
/// Patterns are TeX files as distributed by hyph-utf8 (`hyph-en-us.tex`):
/// one or more `\patterns{…}` groups of Liang patterns and optional
/// `\hyphenation{…}` groups of exception words (`ta-ble`), in UTF-8. When a
/// paragraph asks for hyphenation, registered patterns for its language
/// are used first, then hypher's built-in patterns; soft hyphens in the
/// text are always honoured.
///
/// Cloning is cheap: parsed patterns are shared.
///
/// ```
/// let mut store = inkoxide::HyphenationStore::new();
/// let tex = r"\patterns{ 1ba 1ca 2n1t } \hyphenation{ en-ter-prise }";
/// store.register("en-US", tex.as_bytes())?;
/// assert!(store.contains("en"));
/// let options = inkoxide::RenderOptions::default().with_hyphenation(store);
/// # Ok::<(), inkoxide::InkoxideError>(())
/// ```
#[derive(Clone, Default)]
pub struct HyphenationStore {
    langs: HashMap<String, Arc<Patterns>>,
}

impl fmt::Debug for HyphenationStore {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut langs: Vec<&String> = self.langs.keys().collect();
        langs.sort();
        f.debug_struct("HyphenationStore")
            .field("langs", &langs)
            .finish()
    }
}

impl HyphenationStore {
    /// An empty store.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Parses TeX hyphenation patterns and registers (or replaces) them
    /// for `lang`, a BCP-47 tag whose language subtag has two letters
    /// (`en`, `en-GB`); patterns apply to the language, whatever the
    /// region.
    ///
    /// # Errors
    ///
    /// [`InkoxideError::Hyphenation`] for a tag without a two-letter
    /// language, bytes that are not UTF-8, unbalanced braces, malformed
    /// patterns, or input without any `\patterns` or `\hyphenation` group.
    pub fn register(&mut self, lang: &str, patterns: &[u8]) -> Result<(), InkoxideError> {
        let code = language_code(lang).ok_or_else(|| {
            InkoxideError::Hyphenation(format!(
                "`{lang}` is not a language tag with a two-letter language"
            ))
        })?;
        let parsed = Patterns::parse(patterns)
            .map_err(|e| InkoxideError::Hyphenation(format!("{code}: {e}")))?;
        self.langs.insert(code, Arc::new(parsed));
        Ok(())
    }

    /// Removes the patterns for `lang`; returns whether there were any.
    pub fn unregister(&mut self, lang: &str) -> bool {
        language_code(lang).is_some_and(|code| self.langs.remove(&code).is_some())
    }

    /// Whether patterns are registered for `lang`.
    #[must_use]
    pub fn contains(&self, lang: &str) -> bool {
        language_code(lang).is_some_and(|code| self.langs.contains_key(&code))
    }

    /// Number of languages with registered patterns.
    #[must_use]
    pub fn len(&self) -> usize {
        self.langs.len()
    }

    /// Whether no patterns are registered.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.langs.is_empty()
    }
}

/// Registered patterns first, then hypher.
pub(crate) struct ResolvedHyphenator<'a> {
    pub store: &'a HyphenationStore,
}

impl Hyphenator for ResolvedHyphenator<'_> {
    fn supports(&self, lang: &str) -> bool {
        self.store.langs.contains_key(lang) || HypherHyphenator.supports(lang)
    }

    fn hyphenate(&self, word: &str, lang: &str) -> Vec<usize> {
        match self.store.langs.get(lang) {
            Some(p) => p.hyphenate(word),
            None => HypherHyphenator.hyphenate(word, lang),
        }
    }
}

/// Minimum letters before the first and after the last hyphen (TeX's
/// `\lefthyphenmin` / `\righthyphenmin`; hyph-utf8 uses 2/2 for most
/// languages).
const LEFT_MIN: usize = 2;
const RIGHT_MIN: usize = 2;

/// A Liang pattern trie plus exception words.
#[derive(Default)]
pub(crate) struct Patterns {
    /// Trie nodes; node 0 is the root.
    nodes: Vec<TrieNode>,
    /// Inter-letter levels of every pattern, indexed by `TrieNode::levels`.
    levels: Vec<u8>,
    /// Lowercase exception word → char positions a hyphen may follow.
    exceptions: HashMap<String, Vec<usize>>,
}

#[derive(Default)]
struct TrieNode {
    /// Sorted by char.
    edges: Vec<(char, u32)>,
    /// `(start, len)` into `Patterns::levels` when a pattern ends here.
    levels: Option<(u32, u32)>,
}

impl Patterns {
    pub(crate) fn parse(bytes: &[u8]) -> Result<Self, String> {
        let text = std::str::from_utf8(bytes)
            .map_err(|e| format!("patterns are not UTF-8 ({e}); use the hyph-utf8 files"))?;
        // Strip `%` comments.
        let text: String = text
            .lines()
            .map(|l| l.split_once('%').map_or(l, |(code, _)| code))
            .collect::<Vec<_>>()
            .join("\n");
        let mut p = Self {
            nodes: vec![TrieNode::default()],
            ..Self::default()
        };
        let mut groups = 0;
        for (command, is_patterns) in [("\\patterns", true), ("\\hyphenation", false)] {
            let mut rest = text.as_str();
            while let Some(at) = rest.find(command) {
                rest = rest[at + command.len()..].trim_start();
                let body = rest
                    .strip_prefix('{')
                    .ok_or_else(|| format!("`{command}` is not followed by `{{`"))?;
                let end = body
                    .find('}')
                    .ok_or_else(|| format!("unclosed `{command}{{`"))?;
                if body[..end].contains('{') {
                    return Err(format!("nested braces in `{command}{{…}}`"));
                }
                for token in body[..end].split_whitespace() {
                    if is_patterns {
                        p.insert_pattern(token)?;
                    } else {
                        p.insert_exception(token)?;
                    }
                }
                groups += 1;
                rest = &body[end + 1..];
            }
        }
        if groups == 0 {
            return Err("no `\\patterns{…}` or `\\hyphenation{…}` group found".into());
        }
        Ok(p)
    }

    fn insert_pattern(&mut self, token: &str) -> Result<(), String> {
        if token.contains('^') || token.contains('\\') {
            return Err(format!(
                "pattern `{token}` uses TeX escapes; use the UTF-8 (hyph-utf8) files"
            ));
        }
        let mut letters = Vec::new();
        let mut levels = vec![0u8];
        for c in token.chars() {
            if let Some(d) = c.to_digit(10) {
                let last = levels.last_mut().expect("never empty");
                if *last != 0 {
                    return Err(format!("pattern `{token}` has two digits in a row"));
                }
                *last = d as u8;
            } else {
                letters.push(c.to_lowercase().next().unwrap_or(c));
                levels.push(0);
            }
        }
        if letters.is_empty() {
            return Err(format!("pattern `{token}` has no letters"));
        }
        let mut node = 0usize;
        for &c in &letters {
            node = match self.nodes[node].edges.binary_search_by_key(&c, |e| e.0) {
                Ok(i) => self.nodes[node].edges[i].1 as usize,
                Err(i) => {
                    let next = self.nodes.len();
                    self.nodes.push(TrieNode::default());
                    self.nodes[node].edges.insert(i, (c, next as u32));
                    next
                }
            };
        }
        let start = self.levels.len() as u32;
        self.levels.extend_from_slice(&levels);
        self.nodes[node].levels = Some((start, levels.len() as u32));
        Ok(())
    }

    fn insert_exception(&mut self, token: &str) -> Result<(), String> {
        let mut word = String::new();
        let mut points = Vec::new();
        let mut count = 0;
        for c in token.chars() {
            if c == '-' {
                if count == 0 || points.last() == Some(&count) {
                    return Err(format!("exception `{token}` has a misplaced hyphen"));
                }
                points.push(count);
            } else {
                word.extend(c.to_lowercase());
                count += 1;
            }
        }
        if points.last() == Some(&count) {
            return Err(format!("exception `{token}` ends with a hyphen"));
        }
        self.exceptions.insert(word, points);
        Ok(())
    }

    /// Byte offsets in `word` where a hyphen may go.
    pub(crate) fn hyphenate(&self, word: &str) -> Vec<usize> {
        let starts: Vec<usize> = word.char_indices().map(|(i, _)| i).collect();
        let n = starts.len();
        if n < LEFT_MIN + RIGHT_MIN {
            return Vec::new();
        }
        let lower: Vec<char> = word
            .chars()
            .map(|c| c.to_lowercase().next().unwrap_or(c))
            .collect();
        if !self.exceptions.is_empty() {
            let key: String = lower.iter().collect();
            if let Some(points) = self.exceptions.get(&key) {
                return points.iter().map(|&k| starts[k]).collect();
            }
        }
        // `.word.`; `points[k]` is the level before padded char k.
        let mut padded = Vec::with_capacity(n + 2);
        padded.push('.');
        padded.extend_from_slice(&lower);
        padded.push('.');
        let mut points = vec![0u8; padded.len() + 1];
        for i in 0..padded.len() {
            let mut node = 0usize;
            for &c in &padded[i..] {
                let edges = &self.nodes[node].edges;
                match edges.binary_search_by_key(&c, |e| e.0) {
                    Ok(e) => node = edges[e].1 as usize,
                    Err(_) => break,
                }
                if let Some((start, len)) = self.nodes[node].levels {
                    let levels = &self.levels[start as usize..(start + len) as usize];
                    for (j, &l) in levels.iter().enumerate() {
                        points[i + j] = points[i + j].max(l);
                    }
                }
            }
        }
        // A hyphen after word char k-1 sits before padded char k+1.
        (LEFT_MIN..=n - RIGHT_MIN)
            .filter(|&k| points[k + 1] % 2 == 1)
            .map(|k| starts[k])
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn split(word: &str, offsets: &[usize]) -> String {
        let mut out = String::new();
        let mut last = 0;
        for &at in offsets {
            out.push_str(&word[last..at]);
            out.push('-');
            last = at;
        }
        out + &word[last..]
    }

    fn patterns(tex: &str) -> Patterns {
        Patterns::parse(tex.as_bytes()).unwrap()
    }

    #[test]
    fn liang_example() {
        // The patterns Liang's thesis uses to hyphenate "hyphenation".
        let p = patterns(r"\patterns{ hy3ph he2n hena4 hen5at 1na n2at 1tio 2io o2n }");
        assert_eq!(
            split("hyphenation", &p.hyphenate("hyphenation")),
            "hy-phen-ation"
        );
        assert_eq!(
            split("Hyphenation", &p.hyphenate("Hyphenation")),
            "Hy-phen-ation"
        );
    }

    #[test]
    fn multibyte_letters_and_minimums() {
        let p = patterns("% comment \\patterns{ignored}\n\\patterns{ 1ç 1d 1n 1c }");
        // Offsets are byte offsets; no hyphen within 2 letters of either end.
        assert_eq!(split("façade", &p.hyphenate("façade")), "fa-ça-de");
        assert_eq!(split("garçon", &p.hyphenate("garçon")), "gar-çon");
        assert!(p.hyphenate("ac").is_empty());
    }

    #[test]
    fn exceptions_win() {
        let p = patterns(r"\patterns{ 1b } \hyphenation{ ab-ba-ba }");
        assert_eq!(split("Abbaba", &p.hyphenate("Abbaba")), "Ab-ba-ba");
        assert_eq!(split("abbab", &p.hyphenate("abbab")), "ab-bab");
    }

    #[test]
    fn malformed_input_is_an_error() {
        let mut store = HyphenationStore::new();
        for (lang, tex) in [
            ("en", &b"1ba 1ca"[..]),
            ("en", br"\patterns{ a12b }"),
            ("en", br"\patterns{ 1ba "),
            ("en", br"\patterns{ ^^e3 }"),
            ("en", br"\hyphenation{ -ab }"),
            ("en", b"\\patterns{ \xff }"),
            ("eng", br"\patterns{ 1ba }"),
        ] {
            let err = store.register(lang, tex).unwrap_err();
            assert_eq!(err.code(), "HYPHENATION_ERROR", "{err}");
        }
        assert!(store.is_empty());
    }

    #[test]
    fn registered_patterns_come_first() {
        let mut store = HyphenationStore::new();
        let h = ResolvedHyphenator { store: &store };
        // Built in (hypher), and none for Japanese.
        assert_eq!(
            h.hyphenate("documentation", "en"),
            HypherHyphenator.hyphenate("documentation", "en")
        );
        assert!(!h.supports("ja"));

        store.register("ja-JP", br"\patterns{ 1m 1n }").unwrap();
        store
            .register("EN", br"\hyphenation{ documentati-on }")
            .unwrap();
        assert!(store.contains("ja") && store.contains("en-GB"));
        let h = ResolvedHyphenator { store: &store };
        assert!(h.supports("ja"));
        assert_eq!(split("banana", &h.hyphenate("banana", "ja")), "ba-na-na");
        assert_eq!(
            split("documentation", &h.hyphenate("documentation", "en")),
            "documentati-on"
        );
        assert!(store.unregister("ja"));
        assert!(!store.unregister("ja"));
    }
}
