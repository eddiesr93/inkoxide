//! Hyphenation behind a trait, so pattern sources can change
//! without touching line layout. Patterns registered at runtime
//! ([`HyphenationStore`]) win over the built-in ones (`builtin.rs`), and
//! over the rules for Romanian (`romanian.rs`).

use std::collections::HashMap;
use std::fmt;
use std::sync::{Arc, LazyLock};

use super::{builtin, romanian};
use crate::error::Missing;
use crate::InkoxideError;

/// Finds hyphenation points in a word.
pub(crate) trait Hyphenator: Send + Sync {
    /// Whether patterns exist for `lang` (a tag from [`normalize_tag`]).
    fn supports(&self, lang: &str) -> bool;
    /// Byte offsets inside `word` where a hyphen may be inserted.
    fn hyphenate(&self, word: &str, lang: &str) -> Vec<usize>;
}

/// The built-in patterns alone, without registered ones: what a shaper
/// hyphenates with until it is given a store.
pub(crate) fn builtin_only() -> &'static ResolvedHyphenator<'static> {
    static STORE: LazyLock<HyphenationStore> = LazyLock::new(HyphenationStore::new);
    static ONLY: LazyLock<ResolvedHyphenator<'static>> =
        LazyLock::new(|| ResolvedHyphenator::new(&STORE));
    &ONLY
}

#[cfg(feature = "embedded-hyphenation")]
fn embedded(l: &builtin::Language) -> Option<&'static [u8]> {
    builtin::embedded(l.file)
}

#[cfg(not(feature = "embedded-hyphenation"))]
fn embedded(_: &builtin::Language) -> Option<&'static [u8]> {
    None
}

/// The file of the built-in patterns that text in `lang` (a BCP-47 tag) is
/// hyphenated with when no patterns are registered for it: `de` for
/// `de-AT`, `no` for `nb`. What a build without the `embedded-hyphenation`
/// feature loads with [`HyphenationStore::register_builtin`]. Romanian has
/// none: the engine hyphenates it by rule, in every build.
#[must_use]
pub fn builtin_hyphenation_file(lang: &str) -> Option<&'static str> {
    builtin::language(&paragraph_tag(lang)?).map(|l| l.file)
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

/// Normalises a BCP-47 tag (`en-GB`, `en_gb`, `haw`) to lowercase with `-`
/// separators. The language subtag has two or three letters, every other
/// subtag one to eight letters or digits.
pub(crate) fn normalize_tag(tag: &str) -> Option<String> {
    let tag = tag.to_ascii_lowercase().replace('_', "-");
    let mut subtags = tag.split('-');
    let language = subtags.next()?;
    let valid = (2..=3).contains(&language.len())
        && language.bytes().all(|b| b.is_ascii_lowercase())
        && subtags
            .all(|s| (1..=8).contains(&s.len()) && s.bytes().all(|b| b.is_ascii_alphanumeric()));
    valid.then_some(tag)
}

/// The tag a paragraph is hyphenated in: `tag` normalised, or only its
/// language when the rest is malformed (`en-` hyphenates as `en`).
pub(crate) fn paragraph_tag(tag: &str) -> Option<String> {
    normalize_tag(tag).or_else(|| normalize_tag(tag.split(['-', '_']).next()?))
}

/// Hyphenation patterns registered at runtime, by BCP-47 tag.
///
/// Patterns are TeX files as distributed by hyph-utf8 (`hyph-en-us.tex`):
/// one or more `\patterns{…}` groups of Liang patterns and optional
/// `\hyphenation{…}` groups of exception words (`ta-ble`), in UTF-8. When a
/// paragraph asks for hyphenation, the patterns registered for its tag are
/// used first, then those of each less specific tag (`en-GB`, then `en`),
/// then the built-in patterns for the language (for Romanian, the
/// engine's rules). A hyphen from registered patterns leaves as many
/// letters before and after it as the language's built-in patterns do (two
/// and three in English), else two; exception words break where they are
/// written. Soft hyphens in the text are breaks with or without patterns,
/// unless the paragraph does not wrap or its `hyphenationPenalty` is
/// 10 000.
///
/// Cloning is cheap: parsed patterns are shared.
///
/// ```
/// let mut store = inkoxide::HyphenationStore::new();
/// let tex = r"\patterns{ 1ba 1ca 2n1t } \hyphenation{ en-ter-prise }";
/// store.register("en", tex.as_bytes())?;
/// assert!(store.contains("en-GB"));
/// let options = inkoxide::RenderOptions::default().with_hyphenation(store);
/// # Ok::<(), inkoxide::InkoxideError>(())
/// ```
#[derive(Clone, Default)]
pub struct HyphenationStore {
    /// By normalised tag (`en`, `en-gb`).
    langs: HashMap<String, Arc<Patterns>>,
    /// Built-in automata given at run time, by file
    /// ([`HyphenationStore::register_builtin`]).
    builtin: HashMap<&'static str, Arc<[u8]>>,
}

impl fmt::Debug for HyphenationStore {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut langs: Vec<&String> = self.langs.keys().collect();
        langs.sort();
        let mut builtin: Vec<&&str> = self.builtin.keys().collect();
        builtin.sort();
        f.debug_struct("HyphenationStore")
            .field("langs", &langs)
            .field("builtin", &builtin)
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
    /// for `lang`, a BCP-47 tag with a two- or three-letter language
    /// (`en`, `en-GB`, `haw`), in any case, with `-` or `_`. Patterns for
    /// `en` also serve `en-GB` and every other `en-…` tag without patterns
    /// of its own; patterns for `en-GB` serve only `en-GB` (and its longer
    /// tags).
    ///
    /// # Errors
    ///
    /// [`InkoxideError::Hyphenation`] for a malformed tag, bytes that are
    /// not UTF-8, unbalanced braces, malformed patterns, or input without
    /// any `\patterns` or `\hyphenation` group.
    pub fn register(&mut self, lang: &str, patterns: &[u8]) -> Result<(), InkoxideError> {
        let tag = normalize_tag(lang).ok_or_else(|| {
            InkoxideError::Hyphenation(format!(
                "`{lang}` is not a language tag such as `en`, `en-GB` or `haw`"
            ))
        })?;
        let parsed = Patterns::parse(patterns)
            .map_err(|e| InkoxideError::Hyphenation(format!("{lang}: {e}")))?;
        self.langs.insert(tag, Arc::new(parsed));
        Ok(())
    }

    /// Registers the built-in patterns of file `file`
    /// ([`builtin_hyphenation_file`]), for a build without the
    /// `embedded-hyphenation` feature, which has none of its own: the
    /// engine's automaton for the language, as `@inkoxide/wasm` ships it in
    /// `hyphenation/<file>.bin`. A build with the feature uses its own.
    ///
    /// # Errors
    ///
    /// [`InkoxideError::Hyphenation`] for a file that is not built in, or
    /// bytes that are not an automaton.
    pub fn register_builtin(&mut self, file: &str, automaton: &[u8]) -> Result<(), InkoxideError> {
        let language = builtin::LANGUAGES
            .iter()
            .find(|l| l.file == file)
            .ok_or_else(|| {
                InkoxideError::Hyphenation(format!("`{file}` has no built-in patterns"))
            })?;
        if !builtin::is_automaton(automaton) {
            return Err(InkoxideError::Hyphenation(format!(
                "{file}: not hyphenation patterns of this engine"
            )));
        }
        self.builtin.insert(language.file, Arc::from(automaton));
        Ok(())
    }

    /// Removes the patterns registered for `lang` itself (not those of a
    /// more or less specific tag); returns whether there were any.
    pub fn unregister(&mut self, lang: &str) -> bool {
        normalize_tag(lang).is_some_and(|tag| self.langs.remove(&tag).is_some())
    }

    /// Whether text in `lang` uses registered patterns: its own, or those
    /// of a less specific tag (`en` for `en-GB`).
    #[must_use]
    pub fn contains(&self, lang: &str) -> bool {
        normalize_tag(lang).is_some_and(|tag| self.lookup(&tag).is_some())
    }

    /// The patterns for a normalised tag: its own, else those of the
    /// nearest less specific tag (`en-gb-x-oed` → `en-gb` → `en`).
    fn lookup(&self, tag: &str) -> Option<&Patterns> {
        let mut tag = tag;
        loop {
            if let Some(p) = self.langs.get(tag) {
                return Some(p.as_ref());
            }
            let (rest, _) = tag.rsplit_once('-')?;
            // A one-letter subtag (`x`, an extension) goes with the next.
            tag = match rest.rsplit_once('-') {
                Some((shorter, s)) if s.len() == 1 => shorter,
                _ => rest,
            };
        }
    }

    /// Number of tags with registered patterns.
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

/// Registered patterns first, then the built-in ones: embedded, or given
/// to the store. A built-in language a build has no patterns for is noted
/// in `missing`, and its text is not hyphenated: the render then fails with
/// [`InkoxideError::NotLoaded`].
pub(crate) struct ResolvedHyphenator<'a> {
    store: &'a HyphenationStore,
    missing: Missing,
}

impl<'a> ResolvedHyphenator<'a> {
    pub fn new(store: &'a HyphenationStore) -> Self {
        Self {
            store,
            missing: Missing::default(),
        }
    }

    /// The files of the built-in patterns the text needed and this build
    /// does not have.
    pub fn not_loaded(&self) -> Vec<String> {
        self.missing.names()
    }
}

impl Hyphenator for ResolvedHyphenator<'_> {
    fn supports(&self, lang: &str) -> bool {
        self.store.lookup(lang).is_some()
            || romanian::serves(lang)
            || builtin::language(lang).is_some()
    }

    fn hyphenate(&self, word: &str, lang: &str) -> Vec<usize> {
        if let Some(p) = self.store.lookup(lang) {
            // A built-in language keeps its bounds, its hyph-utf8 file's
            // `hyphenmins`: English leaves three letters after the last
            // hyphen, so no `want-ed`.
            let bounds = builtin::language(lang).map_or(BOUNDS, |l| l.bounds);
            return p.hyphenate(word, bounds);
        }
        if romanian::serves(lang) {
            return romanian::hyphenate(word);
        }
        let Some(l) = builtin::language(lang) else {
            return Vec::new();
        };
        if let Some(trie) = embedded(l).or_else(|| self.store.builtin.get(l.file).map(|t| &**t)) {
            builtin::hyphenate(trie, l, word)
        } else {
            self.missing.note(l.file);
            Vec::new()
        }
    }
}

/// Fewest letters before the first and after the last hyphen (TeX's
/// `\lefthyphenmin` and `\righthyphenmin`) with registered patterns for a
/// language that is not built in: hyph-utf8's for most languages.
const BOUNDS: (usize, usize) = (2, 2);

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

    /// Byte offsets in `word` where a hyphen may go, at least `left`
    /// letters from its start and `right` from its end (an exception word's
    /// points are kept as written).
    pub(crate) fn hyphenate(&self, word: &str, (left, right): (usize, usize)) -> Vec<usize> {
        let starts: Vec<usize> = word.char_indices().map(|(i, _)| i).collect();
        let n = starts.len();
        if n < left + right {
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
        (left..=n - right)
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
            split("hyphenation", &p.hyphenate("hyphenation", BOUNDS)),
            "hy-phen-ation"
        );
        assert_eq!(
            split("Hyphenation", &p.hyphenate("Hyphenation", BOUNDS)),
            "Hy-phen-ation"
        );
    }

    #[test]
    fn multibyte_letters_and_minimums() {
        let p = patterns("% comment \\patterns{ignored}\n\\patterns{ 1ç 1d 1n 1c }");
        // Offsets are byte offsets; no hyphen within 2 letters of either end.
        assert_eq!(split("façade", &p.hyphenate("façade", BOUNDS)), "fa-ça-de");
        assert_eq!(split("garçon", &p.hyphenate("garçon", BOUNDS)), "gar-çon");
        assert!(p.hyphenate("ac", BOUNDS).is_empty());
    }

    #[test]
    fn exceptions_win() {
        let p = patterns(r"\patterns{ 1b } \hyphenation{ ab-ba-ba }");
        assert_eq!(split("Abbaba", &p.hyphenate("Abbaba", BOUNDS)), "Ab-ba-ba");
        assert_eq!(split("abbab", &p.hyphenate("abbab", BOUNDS)), "ab-bab");
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
            ("english", br"\patterns{ 1ba }"),
            ("e", br"\patterns{ 1ba }"),
            ("en-", br"\patterns{ 1ba }"),
            ("en--gb", br"\patterns{ 1ba }"),
            ("en-gb!", br"\patterns{ 1ba }"),
            ("", br"\patterns{ 1ba }"),
        ] {
            let err = store.register(lang, tex).unwrap_err();
            assert_eq!(err.code(), "HYPHENATION_ERROR", "{err}");
        }
        assert!(store.is_empty());
    }

    #[test]
    fn tags() {
        assert_eq!(normalize_tag("en-GB").as_deref(), Some("en-gb"));
        assert_eq!(normalize_tag("DE").as_deref(), Some("de"));
        assert_eq!(normalize_tag("fil_PH").as_deref(), Some("fil-ph"));
        assert_eq!(normalize_tag("zh-Hant-HK").as_deref(), Some("zh-hant-hk"));
        assert_eq!(normalize_tag("x"), None);
        assert_eq!(normalize_tag("en-"), None);
        assert_eq!(paragraph_tag("en-").as_deref(), Some("en"));
        assert_eq!(paragraph_tag("EN_us.UTF-8").as_deref(), Some("en"));
        assert_eq!(paragraph_tag("english"), None);
        // The built-in patterns go by the language subtag.
        assert!(builtin_only().supports("de-at"));
        assert!(!builtin_only().supports("deu"));
    }

    #[test]
    fn registered_patterns_come_first() {
        let mut store = HyphenationStore::new();
        let h = ResolvedHyphenator::new(&store);
        // Built in, and none for Japanese.
        assert_eq!(
            h.hyphenate("documentation", "en"),
            builtin_only().hyphenate("documentation", "en")
        );
        assert!(!h.supports("ja"));

        store.register("ja", br"\patterns{ 1m 1n }").unwrap();
        store
            .register("EN", br"\hyphenation{ documentati-on }")
            .unwrap();
        assert!(store.contains("ja-JP") && store.contains("en-GB"));
        let h = ResolvedHyphenator::new(&store);
        assert!(h.supports("ja"));
        assert_eq!(split("banana", &h.hyphenate("banana", "ja")), "ba-na-na");
        assert_eq!(
            split("documentation", &h.hyphenate("documentation", "en")),
            "documentati-on"
        );
        assert!(store.unregister("ja"));
        assert!(!store.unregister("ja"));
    }

    #[test]
    fn registered_patterns_keep_the_bounds_of_their_language() {
        let mut store = HyphenationStore::new();
        store.register("en", br"\patterns{ 1ed }").unwrap();
        store.register("haw", br"\patterns{ 1ed }").unwrap();
        let h = ResolvedHyphenator::new(&store);
        // English leaves three letters after the last hyphen, as its
        // built-in patterns do.
        assert!(h.hyphenate("wanted", "en").is_empty());
        assert!(h.hyphenate("wanted", "en-us").is_empty());
        // A language that is not built in leaves two.
        assert_eq!(split("wanted", &h.hyphenate("wanted", "haw")), "want-ed");
    }

    #[test]
    fn romanian_is_hyphenated_by_rule_unless_patterns_are_registered() {
        let mut store = HyphenationStore::new();
        let h = ResolvedHyphenator::new(&store);
        assert!(h.supports("ro") && h.supports("ro-md"));
        assert_eq!(
            split("calitate", &h.hyphenate("calitate", "ro")),
            "ca-li-ta-te"
        );
        assert!(h.not_loaded().is_empty());
        assert_eq!(builtin_hyphenation_file("ro"), None);

        store.register("ro", br"\hyphenation{ cali-tate }").unwrap();
        let h = ResolvedHyphenator::new(&store);
        assert_eq!(
            split("calitate", &h.hyphenate("calitate", "ro-md")),
            "cali-tate"
        );
    }

    #[test]
    fn regional_patterns_coexist() {
        let mut store = HyphenationStore::new();
        store
            .register("en-GB", br"\hyphenation{ colo-ur }")
            .unwrap();
        store
            .register("en_us", br"\hyphenation{ co-lour }")
            .unwrap();
        assert_eq!(store.len(), 2);
        let h = ResolvedHyphenator::new(&store);
        assert_eq!(split("colour", &h.hyphenate("colour", "en-gb")), "colo-ur");
        assert_eq!(split("colour", &h.hyphenate("colour", "en-us")), "co-lour");
        // Neither serves the bare language or another region: the built-in patterns do.
        assert!(!store.contains("en") && !store.contains("en-AU"));
        assert_eq!(
            h.hyphenate("colour", "en-au"),
            builtin_only().hyphenate("colour", "en")
        );
        // Unregistering one region keeps the other.
        assert!(store.unregister("EN-gb"));
        assert!(!store.unregister("en"));
        assert!(!store.contains("en-GB") && store.contains("en-US"));
    }

    #[test]
    fn three_letter_languages() {
        let mut store = HyphenationStore::new();
        assert!(!ResolvedHyphenator::new(&store).supports("haw"));
        store.register("haw", br"\patterns{ 1k 1l 1n }").unwrap();
        let h = ResolvedHyphenator::new(&store);
        assert!(h.supports("haw") && h.supports("haw-us"));
        assert_eq!(
            split("hanakala", &h.hyphenate("hanakala", "haw")),
            "ha-na-ka-la"
        );
        // `haw` is not `ha` (Hausa).
        assert!(!store.contains("ha"));
    }

    #[test]
    fn lookup_falls_back_from_region_to_language() {
        let mut store = HyphenationStore::new();
        store.register("de", br"\hyphenation{ stra-sse }").unwrap();
        store
            .register("de-CH", br"\hyphenation{ str-asse }")
            .unwrap();
        let h = ResolvedHyphenator::new(&store);
        let of = |lang| split("strasse", &h.hyphenate("strasse", lang));
        assert_eq!(of("de"), "stra-sse");
        assert_eq!(of("de-at"), "stra-sse");
        assert_eq!(of("de-ch"), "str-asse");
        assert_eq!(of("de-ch-1996"), "str-asse");
        // A private-use or extension subtag goes with its singleton.
        assert_eq!(of("de-ch-x-ab"), "str-asse");
        assert_eq!(of("de-latn-ch"), "stra-sse");
    }
}
