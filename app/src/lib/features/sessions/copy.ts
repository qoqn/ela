import type { ForwardFailure } from '../../bindings';
import type { PasswordNote, SessionErrorInfo, SessionStatus, Tried } from '../../ipc';

const STATE_LABEL: Record<SessionStatus['state'], string> = {
  idle: 'Connecting',
  connecting: 'Connecting',
  verifying: 'Verifying',
  asking: 'Waiting for you',
  authenticating: 'Signing in',
  connected: 'Connected',
  reconnecting: 'Reconnecting',
  closed: 'Disconnected',
};

export function stateLabel(status: SessionStatus): string {
  return status.state === 'closed' && status.error ? 'Failed' : STATE_LABEL[status.state];
}

export function jumpHostName(jumpHost: string): string {
  return `Jump Host ${jumpHost}`;
}

export function stepName(about: SessionStatus | { jump_host: string | null }, name: string) {
  return 'jump_host' in about && about.jump_host ? jumpHostName(about.jump_host) : name;
}

export function identityLabel(algorithm: string): string {
  if (algorithm === 'certificate') return 'Certificate';
  return algorithm.replace(/^ssh-/, '').split('-')[0].toUpperCase();
}

export function trustCopy(
  name: string,
  algorithm: string,
  changed: boolean,
  knownTypes: string[] = [],
) {
  const identity = algorithm === 'certificate' ? 'certificate' : 'key';
  if (!changed && knownTypes.length)
    return {
      title: `New key type for ${name}`,
      detail: `Ela knows ${name} by its ${knownTypes.map(identityLabel).join(' and ')} key, and it now presents an ${identityLabel(algorithm)} key. Servers add key types when they are upgraded. If you can, compare the fingerprint with the one your admin published.`,
      action: 'Trust and connect',
    };
  return changed
    ? {
        title: "The server's identity changed",
        detail: `${name} presented a different ${identity} than the one saved for it. This can mean the server was reinstalled, or that someone is intercepting the connection. Replace the saved ${identity} only if you know why it changed.`,
        action: `Replace ${identity} and connect`,
      }
    : {
        title: `First connection to ${name}`,
        detail:
          "Ela hasn't seen this server before. If you can, compare the fingerprint with the one your admin or provider published.",
        action: 'Trust and connect',
      };
}

export function passphraseCopy(key: string, name: string) {
  const file = key.split('/').pop() || key;
  return {
    title: `Passphrase for ${file}`,
    detail: `This key is locked with a passphrase, which Ela needs to sign in to ${name}.`,
  };
}

export const PASSPHRASE_NOTE = {
  wrong: "That passphrase doesn't unlock the key.",
  saved_wrong: 'The passphrase saved in the Vault no longer unlocks this key.',
  not_saved: 'The Vault holds no passphrase for this key.',
};

function triedFirst(tried: Tried[], name: string): string {
  const keys = tried.includes('keys');
  const agent = tried.includes('agent');
  if (keys && agent) return `No key or ssh-agent signed in to ${name}.`;
  if (keys) return `No key signed in to ${name}.`;
  if (agent) return `No key in ssh-agent signed in to ${name}.`;
  return `${name} needs a password.`;
}

export function passwordCopy(jumpHost: string | null, target: string, tried: Tried[] = []) {
  if (!jumpHost)
    return {
      title: `Password for ${target}`,
      detail: triedFirst(tried, target),
      why: `To remember the password of ${target}.`,
    };
  const hop = jumpHostName(jumpHost);
  return {
    title: `Password for ${hop}`,
    detail: `${triedFirst(tried, hop).slice(0, -1)}, which leads to ${target}. Ela sends this password only to ${jumpHost}.`,
    why: `To remember the password of ${hop}.`,
  };
}

export function passwordNote(note: PasswordNote, jumpHost: string | null, target: string): string {
  const name = jumpHost ? jumpHostName(jumpHost) : target;
  switch (note) {
    case 'wrong':
      return `${name} didn't accept that password.`;
    case 'saved_wrong':
      return `The password saved in the Vault no longer signs in to ${name}.`;
    case 'missing':
      return `The Vault no longer holds the password saved for ${name}.`;
  }
}

export function vaultPromptCopy(forJumpHost: string | null, target: string) {
  const whose = forJumpHost ? jumpHostName(forJumpHost) : target;
  return {
    title: 'Unlock the Vault',
    detail: `To use the password saved for ${whose}. Without it, Ela asks for the password if ${whose} needs one.`,
    wrong: "That password doesn't open the Vault.",
  };
}

export function questionsCopy(name: string, count: number) {
  return {
    title: count === 1 ? `${name} asks a question` : `${name} asks ${count} questions`,
    detail: `The server wants this before it lets you in. Ela sends the answers only to ${name}.`,
  };
}

export const QUESTIONS_REFUSED = "The server didn't accept those answers and asks again.";

// Servers end their prompts with ": " for a terminal, which reads oddly above a field.
export function questionLabel(text: string): string {
  return text.trim().replace(/:$/, '') || 'Answer';
}

export function signInAs(username: string, host: string, port: number): string {
  return `${username}@${port === 22 ? host : hostAndPort(host, port)}`;
}

export interface ErrorCopy {
  title: string;
  detail: string;
  fix?: { text: string; command: string };
}

const FORWARD_DETAIL: Record<ForwardFailure, (jumpHost: string, address: string) => string> = {
  prohibited: (jumpHost, address) =>
    `${jumpHost} doesn't allow forwarding, so it can't pass the connection on to ${address}. Its sshd_config needs AllowTcpForwarding.`,
  unreachable: (jumpHost, address) =>
    `${jumpHost} couldn't open a connection to ${address}. Check the address, and that ${jumpHost} can reach it.`,
  other: (jumpHost, address) => `${jumpHost} refused to open a connection to ${address}.`,
};

function hostAndPort(host: string, port: number): string {
  return host.includes(':') ? `[${host}]:${port}` : `${host}:${port}`;
}

// Without the user: what didn't answer is an address, not an account.
export function endpoint(host: string, port: number | null | undefined): string {
  return port ? hostAndPort(host, port) : host;
}

/** `ssh-keygen -R` names hosts as `host` on port 22 and `[host]:port` otherwise. */
function knownHostsName(host: string, port: number | null): string {
  return port && port !== 22 ? `"[${host}]:${port}"` : host;
}

export function errorCopy(
  err: SessionErrorInfo,
  name: string,
  address: string,
  target?: { host: string; port: number | null },
): ErrorCopy {
  switch (err.kind) {
    case 'host_unreachable':
      return {
        title: `Can't reach ${name}`,
        detail: `Nothing answered at ${address}. Check the address, VPN or firewall.`,
      };
    case 'timeout':
      return {
        title: `${name} didn't respond in time`,
        detail: 'The host may be down, or a firewall may be dropping packets.',
      };
    case 'auth_failed':
      return { title: 'Password or key was rejected', detail: err.detail };
    case 'host_key_changed':
      return {
        title: "The server's identity changed",
        detail: `${name} presented a different key than the one saved in ~/.ssh/known_hosts. This can mean the server was reinstalled, or that someone is intercepting the connection. Ela did not connect. (${err.detail})`,
        fix: target && {
          text: 'If you know why the key changed, remove the old key and connect again:',
          command: `ssh-keygen -R ${knownHostsName(target.host, target.port)}`,
        },
      };
    case 'host_key_revoked':
      return {
        title: "The server's key was revoked",
        detail: `${name} presented a key that your known_hosts file marks as revoked, so it must not be trusted again. Ela did not connect. Ask the server's admin for its new key.`,
      };
    case 'certificate_untrusted':
      return { title: 'Certificate not trusted', detail: err.detail };
    case 'jump_host': {
      const { jump_host, host, port, error } = err.detail;
      const hop = jumpHostName(jump_host);
      const copy = errorCopy(error, hop, hostAndPort(host, port), { host, port });
      return error.kind === 'auth_failed'
        ? { ...copy, title: `${hop} rejected the password or key` }
        : copy;
    }
    case 'forward_failed': {
      const { jump_host, address, failure } = err.detail;
      return {
        title: `Can't reach ${name} through ${jump_host}`,
        detail: FORWARD_DETAIL[failure](jump_host, address),
      };
    }
    default:
      return { title: 'Connection failed', detail: 'detail' in err ? err.detail : '' };
  }
}
