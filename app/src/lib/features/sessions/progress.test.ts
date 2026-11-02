import { describe, expect, test } from 'bun:test';
import type { SessionStatus } from '../../ipc';
import { advance, START, steps } from './progress';

const run = (...statuses: SessionStatus[]) => statuses.reduce(advance, START);
const target = { name: 'db-01', user: 'pg' };
const fingerprint = 'SHA256:x';

describe('steps of a direct connection', () => {
  test('marks what is done, what is happening and what comes', () => {
    const progress = run(
      { state: 'connecting', jump_host: null },
      { state: 'verifying', fingerprint, jump_host: null },
    );
    expect(steps(progress, target)).toEqual([
      { mark: 'done', text: 'Connect to db-01' },
      { mark: 'current', text: 'Checking its identity…' },
      { mark: 'pending', text: 'Sign in as pg' },
    ]);
  });

  test('a failure marks the step it happened in', () => {
    const progress = run({ state: 'authenticating', jump_host: null });
    expect(steps(progress, target, true).at(-1)).toEqual({ mark: 'failed', text: 'Sign in as pg' });
  });

  test('statuses after connecting leave the steps alone', () => {
    const before = run({ state: 'authenticating', jump_host: null });
    expect(advance(before, { state: 'connected' })).toBe(before);
  });
});

describe('steps through Jump Hosts', () => {
  test('the target waits at the end while a Jump Host is on', () => {
    const progress = run(
      { state: 'connecting', jump_host: null },
      { state: 'connecting', jump_host: 'gate' },
      { state: 'authenticating', jump_host: 'gate' },
    );
    expect(steps(progress, target)).toEqual([
      { mark: 'done', text: 'Connect to Jump Host gate' },
      { mark: 'done', text: 'Check its identity' },
      { mark: 'current', text: 'Signing in…' },
      { mark: 'pending', text: 'db-01' },
    ]);
  });

  test('a Jump Host behind it folds into one done step', () => {
    const progress = run(
      { state: 'connecting', jump_host: null },
      { state: 'connecting', jump_host: 'outer' },
      { state: 'authenticating', jump_host: 'outer' },
      { state: 'connecting', jump_host: 'gate' },
      { state: 'authenticating', jump_host: 'gate' },
      { state: 'connecting', jump_host: null },
    );
    expect(steps(progress, target).map((s) => `${s.mark} ${s.text}`)).toEqual([
      'done Jump Host outer',
      'done Jump Host gate',
      'current Connecting to db-01 through gate…',
      'pending Check its identity',
      'pending Sign in as pg',
    ]);
  });

  test('asking to trust a Jump Host is part of checking its identity', () => {
    const progress = run({
      state: 'asking',
      id: 1,
      jump_host: 'gate',
      prompt: {
        kind: 'trust',
        known_types: [],
        algorithm: 'ssh-ed25519',
        fingerprint,
        known: null,
      },
    });
    expect(progress).toEqual({ hops: [], host: 'gate', phase: 'verify' });
  });

  test('asking for a passphrase is part of signing in', () => {
    const progress = run({
      state: 'asking',
      id: 2,
      jump_host: 'gate',
      prompt: {
        kind: 'passphrase',
        key: '~/.ssh/gate',
        credential: 'a0000000-0000-4000-8000-0000000000ee',
        note: null,
      },
    });
    expect(progress).toEqual({ hops: [], host: 'gate', phase: 'sign_in' });
  });

  test("asking for a Jump Host's password is part of signing in to it", () => {
    const progress = run({
      state: 'asking',
      id: 3,
      jump_host: 'gate',
      prompt: {
        kind: 'password',
        username: 'ops',
        host: 'gate.lan',
        port: 22,
        note: null,
        can_remember: true,
        tried: [],
      },
    });
    expect(progress).toEqual({ hops: [], host: 'gate', phase: 'sign_in' });
  });
});
