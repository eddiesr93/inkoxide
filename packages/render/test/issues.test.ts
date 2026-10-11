import { describe, expect, it } from 'vitest';

import { issueMessage } from '../src/pipeline.ts';

describe('issueMessage', () => {
  it('names a missing field', () => {
    expect(issueMessage('Invalid input: expected string, received undefined', 'invalid_type')).toBe(
      'Missing: expected a string',
    );
    expect(issueMessage('Invalid input: expected object, received undefined', 'invalid_type')).toBe(
      'Missing: expected an object',
    );
  });

  it('names the wrong type', () => {
    expect(issueMessage('Invalid input: expected string, received number', 'invalid_type')).toBe(
      'Expected a string, got a number',
    );
    expect(issueMessage('Invalid input: expected array, received null', 'invalid_type')).toBe(
      'Expected an array, got null',
    );
  });

  it('keeps other messages without the generic prefix', () => {
    expect(issueMessage('Too small: expected string to have >=1 characters', 'too_small')).toBe(
      'Too small: expected string to have >=1 characters',
    );
    expect(issueMessage('Invalid input: expected one of "a"|"b"', 'invalid_value')).toBe(
      'Expected one of "a"|"b"',
    );
  });
});
