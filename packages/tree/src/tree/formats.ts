// The format of a `<TextField>`: its readable keys, or the other names Acrobat's own use.
import type { FieldFormat } from '@inkoxide/schema';

import type { TextFormat } from '../components/index.ts';
import type { Fail } from '../errors.ts';

/** A `TextField`'s format, from its readable keys or their other names. */
export function textFieldFormat(f: TextFormat, fail: Fail): FieldFormat {
  if (typeof f !== 'object' || f === null || Array.isArray(f)) {
    fail("<TextField format> is an object, as format={{ type: 'number', decimals: 2 }}.", {
      kind: 'props',
    });
  }
  const type = f.type === 'zip4' ? 'zipPlus4' : f.type;
  const out: FieldFormat = { type };
  // The other names come with their own defaults: no decimals and no thousands separator.
  const aliased = ALIAS_KEYS.some((k) => f[k] !== undefined) && (type === 'number' || type === 'percent');
  const decimals = f.decimals ?? f.nDec ?? (aliased ? 0 : undefined);
  if (decimals !== undefined) out.decimals = decimals;
  const sepComma = f.sepComma ?? (aliased ? false : undefined);
  const separators = f.separators ?? (sepComma === undefined ? undefined : sepComma ? '1,234.56' : '1234.56');
  if (separators !== undefined) out.separators = separators;
  const negative = f.negative ?? (f.negStyle === undefined ? undefined : NEGATIVE[f.negStyle]);
  if (negative !== undefined) out.negative = negative;
  if (f.currency !== undefined) out.currency = f.currency;
  const before = f.currencyBefore ?? f.currencyPrepend;
  if (before !== undefined) out.currencyBefore = before;
  const pattern = f.pattern ?? (f.param === undefined ? undefined : formatParam(type, f.param, fail));
  if (pattern !== undefined) out.pattern = pattern;
  return out;
}

/** A format's `param`: the pattern of a date, or one of the four styles of a time, by index. */
function formatParam(type: FieldFormat['type'], param: string | number, fail: Fail): string {
  if (type !== 'time') return String(param);
  const style = /^[0-3]$/.test(String(param)) ? TIME_STYLES[Number(param)] : undefined;
  if (style === undefined) {
    fail(
      `<TextField format> param on a time is a style from 0 to 3, not ${JSON.stringify(param)}; ` +
        "give a pattern as pattern: 'HH:MM'.",
      { kind: 'props' },
    );
  }
  return style;
}

/** The other names of format keys, which come with their own defaults. */
const ALIAS_KEYS = ['nDec', 'sepComma', 'negStyle', 'currencyPrepend', 'param'] as const;

/** The patterns of a time format's `param`: the four styles of `AFTime_Format`. */
const TIME_STYLES: readonly string[] = ['HH:MM', 'h:MM tt', 'HH:MM:ss', 'h:MM:ss tt'];

/** The negative styles by their other names (`negStyle`). */
const NEGATIVE: Record<string, FieldFormat['negative']> = {
  MinusBlack: 'minus',
  Red: 'red',
  ParensBlack: 'parens',
  ParensRed: 'parensRed',
};
