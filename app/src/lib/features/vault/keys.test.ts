import { expect, test } from 'bun:test';
import { shortFingerprint } from './keys';

test('a long fingerprint keeps its kind, start and end', () => {
  expect(shortFingerprint('SHA256:9fQ2xR7mWc0LqZ1bVt8nKp4sHy6uJd3eGa5oFi2k1c2')).toBe(
    'SHA256:9fQ2xR7mWc0L…i2k1c2',
  );
  expect(shortFingerprint('SHA256:short')).toBe('SHA256:short');
});
