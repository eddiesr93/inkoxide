/** The cache of emoji images kept across renders. */
import { describe, expect, it } from 'vitest';

import { ByteCache } from '../src/images.ts';

const bytes = (n: number) => new Uint8Array(n);

describe('ByteCache', () => {
  it('drops the least recently used entries past its budget', () => {
    const cache = new ByteCache(160);
    cache.set('a', bytes(10));
    cache.set('b', bytes(10));
    cache.set('c', bytes(10));
    expect(cache.size).toBe(30);
    // A hit is the most recently used.
    expect(cache.get('a')?.byteLength).toBe(10);
    for (let i = 0; i < 15; i++) cache.set(`x${i}`, bytes(10));
    expect(cache.size).toBe(160);
    expect(cache.get('b')).toBeUndefined();
    expect(cache.get('c')).toBeUndefined();
    expect(cache.get('a')?.byteLength).toBe(10);
  });

  it('counts an entry set again once', () => {
    const cache = new ByteCache(160);
    cache.set('a', bytes(10));
    cache.set('a', bytes(10));
    expect(cache.size).toBe(10);
    cache.set('a', bytes(5));
    expect(cache.size).toBe(5);
  });

  it('keeps no entry over a sixteenth of its budget', () => {
    const cache = new ByteCache(160);
    cache.set('a', bytes(10));
    cache.set('big', bytes(11));
    expect(cache.get('big')).toBeUndefined();
    expect(cache.size).toBe(10);
    // Replacing an entry with one too big drops it.
    cache.set('a', bytes(11));
    expect(cache.get('a')).toBeUndefined();
    expect(cache.size).toBe(0);
  });
});
