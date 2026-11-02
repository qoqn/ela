import { describe, expect, test } from 'bun:test';
import {
  endpoint,
  errorCopy,
  passphraseCopy,
  passwordCopy,
  passwordNote,
  questionLabel,
  questionsCopy,
  signInAs,
  trustCopy,
} from './copy';

describe('errorCopy through a Jump Host', () => {
  test('a changed key on a Jump Host names it and the command removes its key', () => {
    const copy = errorCopy(
      {
        kind: 'jump_host',
        detail: {
          jump_host: 'gate',
          host: 'gate.internal',
          port: 2222,
          error: { kind: 'host_key_changed', detail: 'a does not match b' },
        },
      },
      'db-01',
      'db-01.internal:22',
      { host: 'db-01.internal', port: 22 },
    );
    expect(copy.title).toBe("The server's identity changed");
    expect(copy.detail).toStartWith('Jump Host gate presented a different key');
    expect(copy.fix?.command).toBe('ssh-keygen -R "[gate.internal]:2222"');
  });

  test('a rejected sign-in says which Jump Host rejected it', () => {
    const copy = errorCopy(
      {
        kind: 'jump_host',
        detail: {
          jump_host: 'gate',
          host: 'gate.internal',
          port: 22,
          error: {
            kind: 'auth_failed',
            detail:
              'The server refused ela with every way Ela tried: password, identity files and ssh-agent.',
          },
        },
      },
      'db-01',
      'db-01.internal:22',
    );
    expect(copy.title).toBe('Jump Host gate rejected the password or key');
  });

  test('an unreachable Jump Host gives its own address', () => {
    const copy = errorCopy(
      {
        kind: 'jump_host',
        detail: {
          jump_host: 'gate',
          host: 'fe80::1',
          port: 22,
          error: { kind: 'host_unreachable', detail: 'refused' },
        },
      },
      'db-01',
      'db-01.internal:22',
    );
    expect(copy.title).toBe("Can't reach Jump Host gate");
    expect(copy.detail).toContain('[fe80::1]:22');
  });

  test('a forward the Jump Host could not open names both ends', () => {
    const forward = (failure: 'prohibited' | 'unreachable') =>
      errorCopy(
        { kind: 'forward_failed', detail: { jump_host: 'gate', address: '10.0.0.7:22', failure } },
        'db-01',
        '10.0.0.7:22',
      );
    expect(forward('unreachable').title).toBe("Can't reach db-01 through gate");
    expect(forward('unreachable').detail).toContain(
      "gate couldn't open a connection to 10.0.0.7:22",
    );
    expect(forward('prohibited').detail).toContain('AllowTcpForwarding');
  });

  test('a later Jump Host that the first could not reach is named as the one out of reach', () => {
    const copy = errorCopy(
      {
        kind: 'jump_host',
        detail: {
          jump_host: 'inner',
          host: 'inner',
          port: 22,
          error: {
            kind: 'forward_failed',
            detail: { jump_host: 'gate', address: 'inner:22', failure: 'unreachable' },
          },
        },
      },
      'db-01',
      'db-01.internal:22',
    );
    expect(copy.title).toBe("Can't reach Jump Host inner through gate");
  });
});

describe('passphraseCopy', () => {
  test('names the key by its file and the host it signs in to', () => {
    const copy = passphraseCopy('~/.ssh/id_ed25519', 'Jump Host gate');
    expect(copy.title).toBe('Passphrase for id_ed25519');
    expect(copy.detail).toEndWith('which Ela needs to sign in to Jump Host gate.');
  });
});

describe('passwordCopy', () => {
  test('names the Jump Host and says the password goes nowhere else', () => {
    const copy = passwordCopy('gate', 'db-01');
    expect(copy.title).toBe('Password for Jump Host gate');
    expect(copy.detail).toContain('which leads to db-01');
    expect(copy.detail).toEndWith('Ela sends this password only to gate.');
  });

  test('the target is named by its Tab, with no Jump Host in the copy', () => {
    const copy = passwordCopy(null, 'db-01', ['keys', 'agent']);
    expect(copy.title).toBe('Password for db-01');
    expect(copy.detail).toBe('No key or ssh-agent signed in to db-01.');
    expect(copy.why).toBe('To remember the password of db-01.');
    expect(passwordCopy('gate', 'db-01').why).toBe('To remember the password of Jump Host gate.');
  });

  test('the detail names only the ways that were tried, so it suits any Protocol', () => {
    expect(passwordCopy(null, 'db-01', ['agent']).detail).toBe(
      'No key in ssh-agent signed in to db-01.',
    );
    expect(passwordCopy(null, 'win-ad', []).detail).toBe('win-ad needs a password.');
    expect(passwordCopy('gate', 'db-01', ['keys']).detail).toStartWith(
      'No key signed in to Jump Host gate, which leads to db-01.',
    );
    expect(passwordNote('missing', null, 'db-01')).toBe(
      'The Vault no longer holds the password saved for db-01.',
    );
  });

  test('a rejected password and a stale saved one read differently', () => {
    expect(passwordNote('wrong', 'gate', 'db-01')).toBe(
      "Jump Host gate didn't accept that password.",
    );
    expect(passwordNote('saved_wrong', 'gate', 'db-01')).toContain('saved in the Vault');
    expect(passwordNote('wrong', null, 'db-01')).toBe("db-01 didn't accept that password.");
    expect(passwordNote('saved_wrong', null, 'db-01')).toBe(
      'The password saved in the Vault no longer signs in to db-01.',
    );
  });

  test('the address leaves out the usual port and brackets IPv6', () => {
    expect(signInAs('ops', 'gate.lan', 22)).toBe('ops@gate.lan');
    expect(signInAs('ops', 'gate.lan', 2222)).toBe('ops@gate.lan:2222');
    expect(signInAs('ops', 'fe80::1', 2222)).toBe('ops@[fe80::1]:2222');
  });
});

describe('questionsCopy', () => {
  test('counts the questions and names who gets the answers', () => {
    expect(questionsCopy('db-01', 1).title).toBe('db-01 asks a question');
    expect(questionsCopy('Jump Host gate', 2).title).toBe('Jump Host gate asks 2 questions');
    expect(questionsCopy('Jump Host gate', 1).detail).toContain(
      'Ela sends the answers only to Jump Host gate.',
    );
  });

  test("a terminal prompt's colon and spaces don't reach the label", () => {
    expect(questionLabel('Verification code: ')).toBe('Verification code');
    expect(questionLabel('  Password:')).toBe('Password');
    expect(questionLabel('Answer the riddle')).toBe('Answer the riddle');
    expect(questionLabel(' : ')).toBe('Answer');
  });
});

describe('endpoint', () => {
  test('names the address Ela tried, without the user', () => {
    expect(endpoint('db-01.internal', 22)).toBe('db-01.internal:22');
    expect(endpoint('::1', 2222)).toBe('[::1]:2222');
    expect(endpoint('db-01.internal', null)).toBe('db-01.internal');
  });
});

describe('trust and revoked keys', () => {
  test('a new key type for a known host names the type it is known by', () => {
    const copy = trustCopy('db-01', 'ssh-ed25519', false, ['ecdsa-sha2-nistp256']);
    expect(copy.title).toBe('New key type for db-01');
    expect(copy.detail).toStartWith(
      'Ela knows db-01 by its ECDSA key, and it now presents an ED25519 key.',
    );
  });

  test('without known types it is a first connection', () => {
    expect(trustCopy('db-01', 'ssh-ed25519', false, []).title).toBe('First connection to db-01');
    expect(trustCopy('db-01', 'ssh-ed25519', true, []).title).toBe("The server's identity changed");
  });

  test('a revoked key on a Jump Host names the Jump Host', () => {
    const copy = errorCopy(
      {
        kind: 'jump_host',
        detail: {
          jump_host: 'gate',
          host: 'gate.internal',
          port: 22,
          error: { kind: 'host_key_revoked', detail: 'SHA256:x' },
        },
      },
      'db-01',
      'db-01.internal:22',
    );
    expect(copy.title).toBe("The server's key was revoked");
    expect(copy.detail).toStartWith('Jump Host gate presented a key');
  });
});
