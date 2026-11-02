import type { LinkState, SessionLink } from '../../ui/types';

// The state that needs the user wins, so a failed Session can't hide behind a healthy one.
const BY_ATTENTION: LinkState[] = [
  'failed',
  'waiting',
  'reconnecting',
  'connecting',
  'connected',
  'closed',
];

export function summariseLinks(states: LinkState[]): SessionLink | undefined {
  const state = BY_ATTENTION.find((candidate) => states.includes(candidate));
  return state && { state, count: states.length };
}

export const linkText = ({ state, count }: SessionLink) =>
  count > 1 ? `${state}, ${count} sessions` : state;
