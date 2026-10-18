//! Romanian hyphenation by rule, not by patterns: the phonetic rules of
//! DOOM2, the Romanian orthographic dictionary. hyph-utf8's Romanian
//! patterns have no licence, so they are not built in.
//!
//! A hyphen goes only between the consonants that separate two vowels,
//! never between vowels: whether two vowels are a diphthong or a hiatus
//! depends on the word (`ia-tă`, `a-ici`), so the boundaries inside a group
//! of vowels are left out, and every boundary given is a syllable's.

/// Fewest letters before the first and after the last hyphen.
const LEFT_MIN: usize = 2;
const RIGHT_MIN: usize = 2;

/// Groups of three consonants split after the second (`sculp-tor`,
/// `punc-te`, `ast-mă`); others after the first (`as-tru`).
const SPLIT_AFTER_TWO: [[char; 3]; 10] = [
    ['l', 'p', 't'],
    ['m', 'p', 't'],
    ['m', 'p', 'ț'],
    ['n', 'c', 't'],
    ['n', 'c', 'ț'],
    ['n', 'c', 'ș'],
    ['n', 'd', 'v'],
    ['r', 'c', 't'],
    ['r', 't', 'f'],
    ['s', 't', 'm'],
];

/// Whether text in `lang` (a normalised tag) is hyphenated by these rules.
pub(crate) fn serves(lang: &str) -> bool {
    lang.split('-').next() == Some("ro")
}

/// Byte offsets inside `word` where a hyphen may go.
pub(crate) fn hyphenate(word: &str) -> Vec<usize> {
    let mut letters = Vec::with_capacity(word.len());
    for (at, c) in word.char_indices() {
        if !c.is_alphabetic() {
            return Vec::new();
        }
        letters.push((at, lower(c)));
    }
    let n = letters.len();
    let vowel = |i: usize| {
        let c = letters[i].1;
        // A final `i` after a consonant is not a syllable (`pomi`, `ochi`).
        is_vowel(c) && !(c == 'i' && i + 1 == n && i > 0 && !is_vowel(letters[i - 1].1))
    };
    // A consonant, `ch` and `gh` as one, by the index of its first letter.
    let consonants = |from: usize, to: usize| {
        let mut units = Vec::new();
        let mut i = from;
        while i < to {
            units.push(i);
            let digraph =
                matches!(letters[i].1, 'c' | 'g') && i + 1 < to && letters[i + 1].1 == 'h';
            i += if digraph { 2 } else { 1 };
        }
        units
    };

    let mut out = Vec::new();
    // The end of the last group of vowels.
    let mut after_vowel: Option<usize> = None;
    let mut i = 0;
    while i < n {
        if !vowel(i) {
            i += 1;
            continue;
        }
        if let Some(from) = after_vowel.filter(|&from| from < i) {
            let units = consonants(from, i);
            let at = split(&units, |u| letters[u].1);
            if at >= LEFT_MIN && n - at >= RIGHT_MIN {
                out.push(letters[at].0);
            }
        }
        while i < n && vowel(i) {
            i += 1;
        }
        after_vowel = Some(i);
    }
    out
}

/// Where the consonants `units` between two vowels split: the index of the
/// letter the hyphen goes before.
fn split(units: &[usize], letter: impl Fn(usize) -> char) -> usize {
    match units {
        [one] => *one,
        // A stop or `f`, `h`, `v` with `l` or `r` begins a syllable
        // (`co-bră`, `su-plu`, `a-flu`).
        [first, second] => {
            let liquid = matches!(letter(*second), 'l' | 'r')
                && matches!(
                    letter(*first),
                    'b' | 'c' | 'd' | 'f' | 'g' | 'h' | 'p' | 't' | 'v'
                );
            if liquid {
                *first
            } else {
                *second
            }
        }
        [a, b, c, ..] => {
            let three = [letter(*a), letter(*b), letter(*c)];
            if SPLIT_AFTER_TWO.contains(&three) {
                *c
            } else {
                *b
            }
        }
        [] => unreachable!("vowels are separated by a consonant"),
    }
}

/// `c` lowercased, with `ş` and `ţ` (cedilla) read as `ș` and `ț` (comma).
fn lower(c: char) -> char {
    let mut lower = c.to_lowercase();
    let c = match (lower.next(), lower.next()) {
        (Some(l), None) => l,
        _ => c,
    };
    match c {
        'ş' => 'ș',
        'ţ' => 'ț',
        c => c,
    }
}

/// A vowel: Romanian's, `y` and the accented vowels of loanwords.
fn is_vowel(c: char) -> bool {
    "aăâeiîouyáàäéèêëíìóòôöúùûü".contains(c)
}

#[cfg(test)]
mod tests {
    use super::hyphenate;

    fn split(word: &str) -> String {
        let mut out = String::new();
        let mut last = 0;
        for at in hyphenate(word) {
            out.push_str(&word[last..at]);
            out.push('-');
            last = at;
        }
        out + &word[last..]
    }

    #[test]
    fn one_consonant_begins_a_syllable() {
        assert_eq!(split("calitate"), "ca-li-ta-te");
        assert_eq!(split("frumoasă"), "fru-moa-să");
        assert_eq!(split("realizare"), "rea-li-za-re");
        assert_eq!(split("România"), "Ro-mâ-nia");
        assert_eq!(split("ŞCOALĂ"), "ŞCOA-LĂ");
    }

    #[test]
    fn two_consonants_split_unless_a_stop_and_a_liquid() {
        assert_eq!(split("artă"), "ar-tă");
        assert_eq!(split("accent"), "ac-cent");
        assert_eq!(split("dezvoltare"), "dez-vol-ta-re");
        assert_eq!(split("expert"), "ex-pert");
        assert_eq!(split("sacru"), "sa-cru");
        assert_eq!(split("cobră"), "co-bră");
        assert_eq!(split("suplu"), "su-plu");
        assert_eq!(split("întreprindere"), "în-tre-prin-de-re");
    }

    #[test]
    fn three_consonants_split_after_the_first_or_the_second() {
        assert_eq!(split("astru"), "as-tru");
        assert_eq!(split("constantă"), "con-stan-tă");
        assert_eq!(split("sculptor"), "sculp-tor");
        assert_eq!(split("punctul"), "punc-tul");
        assert_eq!(split("funcție"), "func-ție");
        assert_eq!(split("funcţie"), "func-ţie");
        assert_eq!(split("astmă"), "ast-mă");
        assert_eq!(split("jertfă"), "jert-fă");
        assert_eq!(split("sandvici"), "sand-vici");
        assert_eq!(split("instrucțiune"), "in-struc-țiu-ne");
    }

    #[test]
    fn ch_and_gh_are_one_consonant() {
        assert_eq!(split("chemare"), "che-ma-re");
        assert_eq!(split("ochelari"), "oche-lari");
        assert_eq!(split("unghie"), "un-ghie");
    }

    #[test]
    fn vowels_stay_together() {
        assert_eq!(split("știință"), "știin-ță");
        assert_eq!(split("băiatul"), "băia-tul");
        assert_eq!(split("obligatoriu"), "obli-ga-to-riu");
        assert_eq!(split("ploaie"), "ploaie");
        assert_eq!(split("hobby"), "hob-by");
    }

    #[test]
    fn a_final_i_after_a_consonant_is_no_syllable() {
        assert_eq!(split("pomi"), "pomi");
        assert_eq!(split("frumoși"), "fru-moși");
        assert_eq!(split("copaci"), "co-paci");
        assert_eq!(split("copii"), "co-pii");
    }

    #[test]
    fn bounds_and_other_input() {
        // Two letters at least on each side.
        assert_eq!(split("examen"), "exa-men");
        assert_eq!(split("apă"), "apă");
        assert_eq!(split(""), "");
        assert_eq!(split("str"), "str");
        // Words only: the engine passes runs of letters.
        assert!(hyphenate("ca-li").is_empty());
    }
}
