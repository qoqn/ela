export type Env = 'prod' | 'staging' | 'dev' | 'lab' | 'none';

export const ENV_NAME: Record<Env, string> = {
  prod: 'Production',
  staging: 'Staging',
  dev: 'Development',
  lab: 'Lab',
  none: 'No environment',
};

export const ENV_COLOR: Record<Env, string> = {
  prod: 'var(--color-env-prod)',
  staging: 'var(--color-env-staging)',
  dev: 'var(--color-env-dev)',
  lab: 'var(--color-env-lab)',
  none: 'var(--color-env-none)',
};

export type LinkState =
  'connecting' | 'waiting' | 'connected' | 'reconnecting' | 'closed' | 'failed';

export type SessionLink = { state: LinkState; count: number };
