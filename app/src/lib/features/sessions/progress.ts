import type { SessionStatus } from '../../ipc';
import { jumpHostName } from './copy';

export type Phase = 'connect' | 'verify' | 'sign_in';

export interface Progress {
  hops: string[];
  host: string | null;
  phase: Phase;
}

export const START: Progress = { hops: [], host: null, phase: 'connect' };

const PHASE: Partial<Record<SessionStatus['state'], Phase>> = {
  connecting: 'connect',
  verifying: 'verify',
  authenticating: 'sign_in',
};

function phaseOf(status: SessionStatus): Phase | undefined {
  if (status.state !== 'asking') return PHASE[status.state];
  // The Vault is opened before any host is reached.
  if (status.prompt.kind === 'vault') return 'connect';
  return status.prompt.kind === 'trust' ? 'verify' : 'sign_in';
}

export function advance(progress: Progress, status: SessionStatus): Progress {
  const phase = phaseOf(status);
  if (!phase) return progress;
  const host = ('jump_host' in status && status.jump_host) || null;
  if (host === progress.host) return { ...progress, phase };
  // The manager announces the target before the route names its first Jump Host, so a
  // target that hasn't started yet is not a step that was done.
  const left = progress.host;
  const hops =
    left !== null && !progress.hops.includes(left) ? [...progress.hops, left] : progress.hops;
  return { hops, host, phase };
}

export type Mark = 'done' | 'current' | 'pending' | 'failed';

export interface Step {
  mark: Mark;
  text: string;
}

const ORDER: Phase[] = ['connect', 'verify', 'sign_in'];

export function steps(
  progress: Progress,
  target: { name: string; user: string | null },
  failed = false,
): Step[] {
  const last = progress.hops.at(-1);
  const about =
    progress.host !== null
      ? jumpHostName(progress.host)
      : last
        ? `${target.name} through ${last}`
        : target.name;
  // The UI doesn't know which user a Jump Host signs in as.
  const as = progress.host === null && target.user ? ` as ${target.user}` : '';
  const doing: Record<Phase, [string, string]> = {
    connect: [`Connect to ${about}`, `Connecting to ${about}…`],
    verify: ['Check its identity', 'Checking its identity…'],
    sign_in: [`Sign in${as}`, `Signing in${as}…`],
  };
  const at = ORDER.indexOf(progress.phase);
  const rows: Step[] = progress.hops.map((hop) => ({ mark: 'done', text: jumpHostName(hop) }));
  ORDER.forEach((phase, i) => {
    const [plain, busy] = doing[phase];
    if (i < at) rows.push({ mark: 'done', text: plain });
    else if (i > at) rows.push({ mark: 'pending', text: plain });
    else rows.push(failed ? { mark: 'failed', text: plain } : { mark: 'current', text: busy });
  });
  if (progress.host !== null) rows.push({ mark: 'pending', text: target.name });
  return rows;
}
