/**
 * Hyphenation pattern files (hyph-utf8) loaded from `assets/hyphenation/`, by the BCP-47 tag they
 * are registered for. The engine falls back from a tag to its language, so the US English patterns,
 * registered for `en`, serve every `lang` that starts with `en` and has no patterns of its own.
 * The release copies these files into the package (scripts/release/stage.mjs).
 */
export const HYPHENATION_FILES: Readonly<Record<string, string>> = { en: 'hyph-en-us.tex' };
