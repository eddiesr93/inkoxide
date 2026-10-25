/** The value of `key` in `map`, made with `make` and kept the first time it is asked for. */
export function memo<K, V>(map: Map<K, V>, key: K, make: (key: K) => V): V {
  let value = map.get(key);
  if (value === undefined) {
    value = make(key);
    map.set(key, value);
  }
  return value;
}

/** A small LRU: a Map whose insertion order is the recency order. */
export class Lru<V> {
  private readonly map = new Map<string, V>();
  readonly capacity: number;
  constructor(capacity: number) {
    this.capacity = capacity;
  }

  get(key: string): V | undefined {
    const v = this.map.get(key);
    if (v !== undefined) {
      this.map.delete(key);
      this.map.set(key, v);
    }
    return v;
  }

  set(key: string, value: V): void {
    this.map.delete(key);
    this.map.set(key, value);
    if (this.map.size > this.capacity) {
      const oldest = this.map.keys().next().value;
      if (oldest !== undefined) this.map.delete(oldest);
    }
  }

  get size(): number {
    return this.map.size;
  }

  clear(): void {
    this.map.clear();
  }
}
