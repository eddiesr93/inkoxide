//! The built-in hyphenation patterns: hypher's automata for 48 languages
//! (`crates/core/hyphenation`, see its README), and hypher's matching, ported
//! so that the patterns can also come at run time. A build with the
//! `embedded-hyphenation` feature (the default) has them all; one without it
//! (the WebAssembly engine) gets each from [`crate::HyphenationStore`] when a
//! document needs it.
//!
//! The matching is hypher 0.1.8's (`hyphenate_bounded`, MIT or Apache-2.0, by
//! Laurenz Mädje, <https://github.com/typst/hypher>), with every read checked: patterns given
//! at run time are not trusted, and a malformed automaton matches less instead
//! of panicking. A test compares it with hypher in every language.

/// A built-in language: its automaton, the ISO 639-1 codes it serves, and
/// the fewest letters before the first and after the last hyphen.
pub(crate) struct Language {
    /// The automaton's file in `crates/core/hyphenation`, without `.bin`.
    pub file: &'static str,
    pub codes: &'static [&'static str],
    pub bounds: (usize, usize),
}

/// hypher's languages, by file (generated from hypher 0.1.8's `lang.rs`).
#[rustfmt::skip]
pub(crate) const LANGUAGES: &[Language] = &[
    Language { file: "af", codes: &["af"], bounds: (1, 2) }, // Afrikaans
    Language { file: "as", codes: &["as"], bounds: (2, 2) }, // Assamese
    Language { file: "be", codes: &["be"], bounds: (2, 2) }, // Belarusian
    Language { file: "bg", codes: &["bg"], bounds: (2, 2) }, // Bulgarian
    Language { file: "bn", codes: &["bn"], bounds: (2, 2) }, // Bengali
    Language { file: "ca", codes: &["ca"], bounds: (2, 2) }, // Catalan
    Language { file: "cs", codes: &["cs"], bounds: (2, 2) }, // Czech
    Language { file: "da", codes: &["da"], bounds: (2, 2) }, // Danish
    Language { file: "de", codes: &["de"], bounds: (2, 2) }, // German
    Language { file: "el", codes: &["el"], bounds: (1, 1) }, // Greek
    Language { file: "en", codes: &["en"], bounds: (2, 3) }, // English
    Language { file: "es", codes: &["es"], bounds: (2, 2) }, // Spanish
    Language { file: "et", codes: &["et"], bounds: (2, 3) }, // Estonian
    Language { file: "fi", codes: &["fi"], bounds: (2, 2) }, // Finnish
    Language { file: "fr", codes: &["fr"], bounds: (2, 2) }, // French
    Language { file: "gl", codes: &["gl"], bounds: (2, 2) }, // Galician
    Language { file: "gu", codes: &["gu"], bounds: (2, 2) }, // Gujarati
    Language { file: "hi", codes: &["hi"], bounds: (2, 2) }, // Hindi
    Language { file: "hr", codes: &["hr"], bounds: (2, 2) }, // Croatian
    Language { file: "hu", codes: &["hu"], bounds: (2, 2) }, // Hungarian
    Language { file: "is", codes: &["is"], bounds: (2, 2) }, // Icelandic
    Language { file: "it", codes: &["it"], bounds: (2, 2) }, // Italian
    Language { file: "ka", codes: &["ka"], bounds: (1, 2) }, // Georgian
    Language { file: "kn", codes: &["kn"], bounds: (2, 2) }, // Kannada
    Language { file: "ku", codes: &["ku"], bounds: (2, 2) }, // Kurmanji
    Language { file: "la", codes: &["la"], bounds: (2, 2) }, // Latin
    Language { file: "lt", codes: &["lt"], bounds: (2, 2) }, // Lithuanian
    Language { file: "ml", codes: &["ml"], bounds: (2, 2) }, // Malayalam
    Language { file: "mn", codes: &["mn"], bounds: (2, 2) }, // Mongolian
    Language { file: "mr", codes: &["mr"], bounds: (2, 2) }, // Marathi
    Language { file: "nl", codes: &["nl"], bounds: (2, 2) }, // Dutch
    Language { file: "no", codes: &["no", "nb", "nn"], bounds: (2, 2) }, // Norwegian
    Language { file: "or", codes: &["or"], bounds: (2, 2) }, // Oriya
    Language { file: "pa", codes: &["pa"], bounds: (2, 2) }, // Panjabi
    Language { file: "pl", codes: &["pl"], bounds: (2, 2) }, // Polish
    Language { file: "pt", codes: &["pt"], bounds: (2, 3) }, // Portuguese
    Language { file: "ru", codes: &["ru"], bounds: (2, 2) }, // Russian
    Language { file: "sa", codes: &["sa"], bounds: (2, 2) }, // Sanskrit
    Language { file: "sk", codes: &["sk"], bounds: (2, 3) }, // Slovak
    Language { file: "sl", codes: &["sl"], bounds: (2, 2) }, // Slovenian
    Language { file: "sq", codes: &["sq"], bounds: (2, 2) }, // Albanian
    Language { file: "sr", codes: &["sr"], bounds: (2, 2) }, // Serbian
    Language { file: "sv", codes: &["sv"], bounds: (2, 2) }, // Swedish
    Language { file: "ta", codes: &["ta"], bounds: (2, 2) }, // Tamil
    Language { file: "te", codes: &["te"], bounds: (2, 2) }, // Telugu
    Language { file: "tk", codes: &["tk"], bounds: (2, 2) }, // Turkmen
    Language { file: "tr", codes: &["tr"], bounds: (2, 2) }, // Turkish
    Language { file: "uk", codes: &["uk"], bounds: (2, 2) }, // Ukrainian
];

/// The built-in language of a normalised tag: by its language subtag.
pub(crate) fn language(tag: &str) -> Option<&'static Language> {
    let code = tag.split('-').next()?;
    LANGUAGES.iter().find(|l| l.codes.contains(&code))
}

/// The embedded automaton in `file`.
#[cfg(feature = "embedded-hyphenation")]
#[rustfmt::skip]
pub(crate) fn embedded(file: &str) -> Option<&'static [u8]> {
    Some(match file {
        "af" => include_bytes!("../../hyphenation/af.bin"),
        "as" => include_bytes!("../../hyphenation/as.bin"),
        "be" => include_bytes!("../../hyphenation/be.bin"),
        "bg" => include_bytes!("../../hyphenation/bg.bin"),
        "bn" => include_bytes!("../../hyphenation/bn.bin"),
        "ca" => include_bytes!("../../hyphenation/ca.bin"),
        "cs" => include_bytes!("../../hyphenation/cs.bin"),
        "da" => include_bytes!("../../hyphenation/da.bin"),
        "de" => include_bytes!("../../hyphenation/de.bin"),
        "el" => include_bytes!("../../hyphenation/el.bin"),
        "en" => include_bytes!("../../hyphenation/en.bin"),
        "es" => include_bytes!("../../hyphenation/es.bin"),
        "et" => include_bytes!("../../hyphenation/et.bin"),
        "fi" => include_bytes!("../../hyphenation/fi.bin"),
        "fr" => include_bytes!("../../hyphenation/fr.bin"),
        "gl" => include_bytes!("../../hyphenation/gl.bin"),
        "gu" => include_bytes!("../../hyphenation/gu.bin"),
        "hi" => include_bytes!("../../hyphenation/hi.bin"),
        "hr" => include_bytes!("../../hyphenation/hr.bin"),
        "hu" => include_bytes!("../../hyphenation/hu.bin"),
        "is" => include_bytes!("../../hyphenation/is.bin"),
        "it" => include_bytes!("../../hyphenation/it.bin"),
        "ka" => include_bytes!("../../hyphenation/ka.bin"),
        "kn" => include_bytes!("../../hyphenation/kn.bin"),
        "ku" => include_bytes!("../../hyphenation/ku.bin"),
        "la" => include_bytes!("../../hyphenation/la.bin"),
        "lt" => include_bytes!("../../hyphenation/lt.bin"),
        "ml" => include_bytes!("../../hyphenation/ml.bin"),
        "mn" => include_bytes!("../../hyphenation/mn.bin"),
        "mr" => include_bytes!("../../hyphenation/mr.bin"),
        "nl" => include_bytes!("../../hyphenation/nl.bin"),
        "no" => include_bytes!("../../hyphenation/no.bin"),
        "or" => include_bytes!("../../hyphenation/or.bin"),
        "pa" => include_bytes!("../../hyphenation/pa.bin"),
        "pl" => include_bytes!("../../hyphenation/pl.bin"),
        "pt" => include_bytes!("../../hyphenation/pt.bin"),
        "ru" => include_bytes!("../../hyphenation/ru.bin"),
        "sa" => include_bytes!("../../hyphenation/sa.bin"),
        "sk" => include_bytes!("../../hyphenation/sk.bin"),
        "sl" => include_bytes!("../../hyphenation/sl.bin"),
        "sq" => include_bytes!("../../hyphenation/sq.bin"),
        "sr" => include_bytes!("../../hyphenation/sr.bin"),
        "sv" => include_bytes!("../../hyphenation/sv.bin"),
        "ta" => include_bytes!("../../hyphenation/ta.bin"),
        "te" => include_bytes!("../../hyphenation/te.bin"),
        "tk" => include_bytes!("../../hyphenation/tk.bin"),
        "tr" => include_bytes!("../../hyphenation/tr.bin"),
        "uk" => include_bytes!("../../hyphenation/uk.bin"),
        _ => return None,
    })
}

/// Whether `bytes` look like an automaton: a root that can be read.
pub(crate) fn is_automaton(bytes: &[u8]) -> bool {
    State::root(bytes).is_some()
}

/// Byte offsets inside `word` where a hyphen may go, by the automaton
/// `trie` of `language`.
pub(crate) fn hyphenate(trie: &[u8], language: &Language, word: &str) -> Vec<usize> {
    let Some(root) = State::root(trie) else {
        return Vec::new();
    };
    let (left_min, right_min) = language.bounds;
    let dotted = lowercase_and_dot(word);
    let (min_idx, max_idx) = char_to_byte_bounds(word, left_min, right_min);
    // The levels between each two bytes of the word.
    let mut levels = vec![0u8; word.len().saturating_sub(1)];
    for start in 0..dotted.len() {
        if !is_char_boundary(dotted[start]) {
            continue;
        }
        let mut state = root;
        for &b in &dotted[start..] {
            let Some(next) = state.transition(b) else {
                break;
            };
            state = next;
            for (offset, level) in state.levels() {
                let split = start + offset;
                if split >= min_idx && split <= max_idx {
                    if let Some(slot) = split.checked_sub(2).and_then(|i| levels.get_mut(i)) {
                        *slot = (*slot).max(level);
                    }
                }
            }
        }
    }
    // A hyphen goes after each byte with an odd level.
    levels
        .iter()
        .enumerate()
        .filter(|(_, l)| *l % 2 == 1)
        .map(|(i, _)| i + 1)
        .collect()
}

/// The word lowercased, between dots: patterns match the edges of a word
/// through them. A character whose lowercase has another length stays.
fn lowercase_and_dot(word: &str) -> Vec<u8> {
    let mut dotted = Vec::with_capacity(word.len() + 2);
    dotted.push(b'.');
    let mut buf = [0; 4];
    for mut c in word.chars() {
        let mut lower = c.to_lowercase();
        if let (Some(l), None) = (lower.next(), lower.next()) {
            if l.len_utf8() == c.len_utf8() {
                c = l;
            }
        }
        dotted.extend_from_slice(c.encode_utf8(&mut buf).as_bytes());
    }
    dotted.push(b'.');
    dotted
}

/// The bounds in characters as byte offsets in the dotted word.
fn char_to_byte_bounds(word: &str, left_min: usize, right_min: usize) -> (usize, usize) {
    let left_min = left_min.max(1);
    let right_min = right_min.max(1);
    let min_idx = 1 + word
        .chars()
        .take(left_min)
        .map(char::len_utf8)
        .sum::<usize>();
    let max_idx = (1 + word.len()).saturating_sub(
        word.chars()
            .rev()
            .take(right_min)
            .map(char::len_utf8)
            .sum::<usize>(),
    );
    (min_idx, max_idx)
}

fn is_char_boundary(b: u8) -> bool {
    (b as i8) >= -0x40
}

/// A state of the automaton: its levels, and its transitions as labels and
/// relative targets of `stride` bytes.
#[derive(Copy, Clone)]
struct State<'a> {
    data: &'a [u8],
    addr: usize,
    stride: usize,
    levels: &'a [u8],
    trans: &'a [u8],
    targets: &'a [u8],
}

impl<'a> State<'a> {
    /// The root, whose address the automaton starts with.
    fn root(data: &'a [u8]) -> Option<Self> {
        let addr = u32::from_be_bytes(data.get(..4)?.try_into().ok()?) as usize;
        Self::at(data, addr)
    }

    fn at(data: &'a [u8], addr: usize) -> Option<Self> {
        let node = data.get(addr..)?;
        let head = *node.first()?;
        let has_levels = head >> 7 != 0;
        let stride = usize::from((head >> 5) & 3);
        let mut count = usize::from(head & 31);
        let mut pos = 1;
        if count == 31 {
            count = usize::from(*node.get(pos)?);
            pos += 1;
        }
        let mut levels: &[u8] = &[];
        if has_levels {
            let hi = usize::from(*node.get(pos)?) << 4;
            let lo = *node.get(pos + 1)?;
            let offset = hi | usize::from(lo >> 4);
            levels = data.get(offset..offset + usize::from(lo & 15))?;
            pos += 2;
        }
        let trans = node.get(pos..pos + count)?;
        pos += count;
        let targets = node.get(pos..pos + stride * count)?;
        Some(Self {
            data,
            addr,
            stride,
            levels,
            trans,
            targets,
        })
    }

    fn transition(self, b: u8) -> Option<Self> {
        let idx = self.trans.iter().position(|&x| x == b)?;
        let offset = self.stride * idx;
        let delta = from_be_bytes(self.targets.get(offset..offset + self.stride)?)?;
        let next = usize::try_from(self.addr as isize + delta).ok()?;
        Self::at(self.data, next)
    }

    /// The levels of the state, by their offset from the start of the match.
    fn levels(self) -> impl Iterator<Item = (usize, u8)> + 'a {
        let mut offset = 0;
        self.levels.iter().map(move |&packed| {
            offset += usize::from(packed / 10);
            (offset, packed % 10)
        })
    }
}

/// A signed big-endian number of 1, 2 or 3 bytes.
fn from_be_bytes(buf: &[u8]) -> Option<isize> {
    Some(match *buf {
        [a] => isize::from(a as i8),
        [a, b] => isize::from(i16::from_be_bytes([a, b])),
        [a, b, c] => {
            ((usize::from(a) << 16 | usize::from(b) << 8 | usize::from(c)) as isize) - (1 << 23)
        }
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Words in many scripts and lengths, some longer than hypher hyphenates
    /// without allocating.
    #[cfg(feature = "embedded-hyphenation")]
    const WORDS: &[&str] = &[
        "extensive",
        "hyphenation",
        "documentation",
        "Donaudampfschifffahrtsgesellschaft",
        "διαμερίσματα",
        "Wörterbuch",
        "ﬁnancial",
        "İstanbul",
        "naïveté",
        "electroencefalografista",
        "გამარჯობა",
        "उत्तरप्रदेश",
        "переподготовка",
        "tämäkin",
        "a",
        "",
        "ab",
        "équipement",
        "przeciwdziałanie",
        "nejneobhospodařovávatelnějšími",
        "honorificabilitudinitatibus",
        "Rindfleischetikettierungsüberwachungsaufgabenübertragungsgesetz",
    ];

    /// The same breaks as hypher, in every language, for the embedded automata.
    #[cfg(feature = "embedded-hyphenation")]
    #[test]
    fn matches_hypher_in_every_language() {
        for lang in LANGUAGES {
            for &code in lang.codes {
                let h = hypher::Lang::from_iso(code.as_bytes().try_into().unwrap())
                    .unwrap_or_else(|| panic!("hypher has no {code}"));
                assert_eq!(h.bounds(), lang.bounds, "{code}");
                for word in WORDS {
                    let mut expected = Vec::new();
                    let mut pos = 0;
                    for syllable in hypher::hyphenate(word, h) {
                        pos += syllable.len();
                        if pos < word.len() {
                            expected.push(pos);
                        }
                    }
                    let trie = embedded(lang.file).unwrap();
                    assert_eq!(hyphenate(trie, lang, word), expected, "{code}: {word}");
                }
            }
        }
    }

    /// Every language hypher has is here.
    #[test]
    fn has_every_language_of_hypher() {
        let mut codes = 0;
        for a in b'a'..=b'z' {
            for b in b'a'..=b'z' {
                if hypher::Lang::from_iso([a, b]).is_some() {
                    codes += 1;
                    let code = [a, b];
                    let code = std::str::from_utf8(&code).unwrap();
                    assert!(language(code).is_some(), "{code}");
                }
            }
        }
        assert_eq!(
            codes,
            LANGUAGES.iter().map(|l| l.codes.len()).sum::<usize>()
        );
    }

    /// A malformed automaton matches nothing, or less, without panicking.
    #[test]
    fn malformed_automata_do_not_panic() {
        let en = language("en").unwrap();
        let good = include_bytes!("../../hyphenation/en.bin");
        for cut in [0, 1, 3, 4, 5, 64, good.len() / 2, good.len() - 1] {
            hyphenate(&good[..cut], en, "hyphenation");
        }
        let mut flipped = good.to_vec();
        for i in (0..flipped.len()).step_by(97) {
            flipped[i] ^= 0xff;
            hyphenate(&flipped, en, "hyphenation");
        }
        hyphenate(&[0xff; 16], en, "hyphenation");
    }
}
