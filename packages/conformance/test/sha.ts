/** What the conformance tests compare PDFs by: their SHA-256. */
import { createHash } from 'node:crypto';

export const sha = (pdf: Uint8Array): string => createHash('sha256').update(pdf).digest('hex');
