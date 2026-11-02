import type { HostKeyCode } from '../ipc';
import { isHostKeyCode } from './shortcuts';

// Before settings.toml, the webview kept the choice here.
export const LEGACY_HOST_KEY = 'ela.hostKey';

type Storage = Pick<globalThis.Storage, 'getItem' | 'removeItem'>;
type Backend = {
  hostKey: () => Promise<HostKeyCode | null>;
  setHostKey: (code: HostKeyCode) => Promise<null>;
};

// The old value is removed only once settings.toml has a choice, so a failure leaves it to
// try again on the next start.
export async function loadHostKey(storage: Storage, backend: Backend): Promise<HostKeyCode | null> {
  const legacy = storage.getItem(LEGACY_HOST_KEY);
  let saved = await backend.hostKey();
  if (saved === null && isHostKeyCode(legacy)) {
    await backend.setHostKey(legacy);
    saved = legacy;
  }
  if (legacy !== null) storage.removeItem(LEGACY_HOST_KEY);
  return saved;
}
