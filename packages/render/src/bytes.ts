/** Byte helpers that work wherever the package runs, without Node's `Buffer`. */

/** The bytes of base64 text (padding optional), which the caller has checked. */
export function base64Bytes(base64: string): Uint8Array {
  const text = atob(base64.padEnd(Math.ceil(base64.length / 4) * 4, '='));
  const bytes = new Uint8Array(text.length);
  for (let i = 0; i < text.length; i++) bytes[i] = text.charCodeAt(i);
  return bytes;
}

const LATIN1 = new TextDecoder('latin1');

/** `bytes` as text, one character a byte. */
export function latin1(bytes: Uint8Array): string {
  return LATIN1.decode(bytes);
}

/** Whether `a` and `b` hold the same bytes. */
export function sameBytes(a: Uint8Array, b: Uint8Array): boolean {
  if (a.byteLength !== b.byteLength) return false;
  for (let i = 0; i < a.byteLength; i++) if (a[i] !== b[i]) return false;
  return true;
}
