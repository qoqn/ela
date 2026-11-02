// Long enough to tell keys apart at a glance; the full one is in the tooltip and the dialog.
export function shortFingerprint(fingerprint: string): string {
  const [kind, hash] = fingerprint.includes(':')
    ? [
        fingerprint.slice(0, fingerprint.indexOf(':') + 1),
        fingerprint.slice(fingerprint.indexOf(':') + 1),
      ]
    : ['', fingerprint];
  return hash.length > 20 ? `${kind}${hash.slice(0, 12)}…${hash.slice(-6)}` : fingerprint;
}
