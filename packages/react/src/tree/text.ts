import type { HyphenationCallback } from '../components/index.ts';
import type { TemplateErrorInit } from '../errors.ts';
import type { TextCtx } from './style.ts';

const locales = new Map<string, string | undefined>();

/**
 * `lang` when Intl takes it as a language tag. The engine also takes tags Intl does not
 * (`en_US`); text in those is cased by the default rules.
 */
function caseLocale(lang: string | undefined): string | undefined {
  if (lang === undefined) return undefined;
  if (!locales.has(lang)) {
    let ok: string | undefined;
    try {
      Intl.getCanonicalLocales(lang);
      ok = lang;
    } catch {
      // Not a BCP 47 tag.
    }
    locales.set(lang, ok);
  }
  return locales.get(lang);
}

/** Text as `textTransform` displays it, cased by the rules of `lang`. */
export function transformText(text: string, how: TextCtx['textTransform'], tag: string | undefined): string {
  const lang = how === undefined || how === 'none' ? undefined : caseLocale(tag);
  switch (how) {
    case 'uppercase':
      return text.toLocaleUpperCase(lang);
    case 'lowercase':
      return text.toLocaleLowerCase(lang);
    case 'capitalize':
      return text.replace(
        /(^|[\s\-–—(„"«])(\p{L})/gu,
        (_, pre: string, ch: string) => pre + ch.toLocaleUpperCase(lang),
      );
    default:
      return text;
  }
}

/** A word as a hyphenation callback sees it: letters, their marks, and soft hyphens. */
const WORD = /[\p{L}\p{M}\u00AD]+/gu;
const SHY = '\u00AD';

/** Reports a callback that throws or returns parts that are not the word. */
type Fail = (message: string, init: TemplateErrorInit) => never;

/** Hyphenation callbacks applied to text, each distinct word asked once per callback per build. */
export class Hyphenator {
  private readonly words = new Map<HyphenationCallback, Map<string, string>>();

  /** Joins the parts `cb` returns for each word with soft hyphens, which the engine breaks at. */
  apply(text: string, cb: HyphenationCallback, fail: Fail): string {
    let done = this.words.get(cb);
    if (!done) {
      done = new Map();
      this.words.set(cb, done);
    }
    const cache = done;
    return text.replace(WORD, (word) => {
      // Soft hyphens already in a word are the author's own break points.
      if (word.includes(SHY)) return word;
      let out = cache.get(word);
      if (out === undefined) {
        out = syllablesOf(word, cb, fail).join(SHY);
        cache.set(word, out);
      }
      return out;
    });
  }
}

function syllablesOf(word: string, cb: HyphenationCallback, fail: Fail): string[] {
  let parts: unknown;
  try {
    parts = cb(word);
  } catch (e) {
    fail(`hyphenationCallback threw for "${word}": ${(e as Error).message}`, { kind: 'render', cause: e });
  }
  if (!Array.isArray(parts) || parts.some((p) => typeof p !== 'string') || parts.join('') !== word) {
    fail(
      `hyphenationCallback returned ${JSON.stringify(parts)} for "${word}": return the parts of the word in order (["hy", "phen", "ation"]), or [word] to keep it whole.`,
      { kind: 'props' },
    );
  }
  return (parts as string[]).filter((p) => p !== '');
}
