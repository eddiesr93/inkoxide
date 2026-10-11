import type { Instance } from '../reconciler/instance.ts';

/** Splits an engine path (`pages[0].children[3].runs[1].font`) into keys. */
export function parsePath(path: string): Array<string | number> {
  const out: Array<string | number> = [];
  for (const m of path.matchAll(/([A-Za-z_$][\w$]*)|\[(\d+)\]/g)) {
    if (m[1] !== undefined) out.push(m[1]);
    else if (m[2] !== undefined) out.push(Number(m[2]));
  }
  return out;
}

/** The deepest instance that produced an object on `path`. */
export function locate(doc: object, path: string, origin: Map<object, Instance>): Instance | undefined {
  let cur: unknown = doc;
  let best = origin.get(doc);
  for (const key of parsePath(path)) {
    if (cur === null || typeof cur !== 'object') break;
    cur = (cur as Record<string | number, unknown>)[key];
    if (cur !== null && typeof cur === 'object') {
      const hit = origin.get(cur);
      if (hit) best = hit;
    }
  }
  return best;
}
