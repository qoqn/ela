import { mockIPC } from '@tauri-apps/api/mocks';
import type { Channel } from '@tauri-apps/api/core';
import { ENCODING_JPEG, FRAME_HEADER_LEN, FRAME_RECT_LEN, MSG_FRAME } from '../lib/ipc';
import type {
  PassphraseNote,
  PromptReply,
  PasswordNote,
  SessionStatus_Serialize,
} from '../lib/bindings';
import type {
  FrontendEvent,
  Prompt,
  ItemRef,
  HostKeyCode,
  KnownHost,
  KnownHostSection,
  OpenRequest,
  ProtocolInfo,
  SavedConnection,
  SavedFolder,
  SessionErrorInfo as SessionError,
} from '../lib/ipc';
import { folderAndInside, inheritedSettings } from '../lib/features/tree/tree';
import schemas from './settings-schemas.json';

const protocols: ProtocolInfo[] = [
  ['ssh', 'SSH', 22, 'terminal'],
  ['rdp', 'RDP', 3389, 'framebuffer'],
  ['bench-term', 'Benchmark: terminal flood', null, 'terminal'],
  ['bench-fb', 'Benchmark: framebuffer', null, 'framebuffer'],
].map(
  ([id, display_name, default_port, renderer]) =>
    ({
      id,
      display_name,
      default_port,
      renderer,
      capabilities: {
        resize: true,
        clipboard: id === 'rdp',
        file_transfer: id === 'ssh',
      },
      settings_schema: (schemas as Record<string, unknown>)[id as string] ?? {},
      listed: !String(id).startsWith('bench'),
    }) as ProtocolInfo,
);

const ids = {
  production: 'f0000000-0000-4000-8000-000000000001',
  databases: 'f0000000-0000-4000-8000-000000000002',
  staging: 'f0000000-0000-4000-8000-000000000003',
  lab: 'f0000000-0000-4000-8000-000000000004',
};
const scene = new URLSearchParams(location.search).get('scene') ?? '';
const seeded = scene.startsWith('tree') || scene.startsWith('password');
const MISSING_SECRET = 'a0000000-0000-4000-8000-00000000dead';
const SAVED_SECRET = 'a0000000-0000-4000-8000-000000000001';
const FOLDER_SECRET = 'a0000000-0000-4000-8000-000000000002';
let vaultState: 'absent' | 'locked' | 'unlocked' = ['vaultnew', 'treevaultcreate'].includes(scene)
  ? 'absent'
  : ['vault', 'vaultchange', 'vaultchangewrong', 'vaultchanged'].includes(scene) ||
      scene.startsWith('vaultkeys') ||
      ['treevaultsaved', 'treevaultmissing'].includes(scene) ||
      ['passphrase', 'passphrasewrong', 'passphrasesaved', 'passphrasejump'].includes(scene) ||
      ['jumppassword', 'jumppasswordwrong', 'jumppasswordsaved'].includes(scene) ||
      ['password', 'passwordwrong', 'passwordsaved'].includes(scene)
    ? 'unlocked'
    : 'locked';
let vaultIdleMinutes = 15;
let keyPassphrases = scene.startsWith('vaultkeys')
  ? [
      {
        credential: 'a0000000-0000-4000-8000-0000000000e1',
        path: '~/.ssh/id_ed25519',
        fingerprint: 'SHA256:Kd+y0sfwr2RkP5L44HJyqQWIlJJBlg5VMz2jwGmREUU',
      },
      {
        credential: 'a0000000-0000-4000-8000-0000000000e2',
        path: '~/.ssh/deploy/legacy_rsa.pem',
        fingerprint: null,
      },
      {
        credential: 'a0000000-0000-4000-8000-0000000000e3',
        path: '~/work/keys/gate_ed25519',
        fingerprint: 'SHA256:9fQ2xR7mWc0LqZ1bVt8nKp4sHy6uJd3eGa5oFi2k1c2',
      },
    ]
  : [];
const KEY_SECRET = 'a0000000-0000-4000-8000-0000000000ee';
const secrets = new Set([SAVED_SECRET, FOLDER_SECRET]);
if (scene === 'passphraselocked') secrets.add(KEY_SECRET);
const fakeFingerprint = (seed: string) =>
  `SHA256:${btoa(seed.repeat(32)).slice(0, 43).replaceAll('=', 'A')}`;
const fakeKey = (
  name: string,
  algorithm = 'ssh-ed25519',
  known_as: string | null = null,
): KnownHost => ({
  name,
  hashed: name.startsWith('|'),
  known_as,
  algorithm,
  fingerprint: fakeFingerprint(name + algorithm),
});
const fakeCert = (name: string): KnownHost => ({
  name,
  hashed: false,
  known_as: null,
  algorithm: 'certificate',
  fingerprint: `SHA256:${Array.from({ length: 32 }, (_, i) =>
    ((name.charCodeAt(i % name.length) * (i + 7)) & 255)
      .toString(16)
      .toUpperCase()
      .padStart(2, '0'),
  ).join(':')}`,
});
const fakeKnownHosts: KnownHostSection[] = [
  {
    store: 'ssh_keys',
    path: '/home/you/.ssh/known_hosts',
    entries:
      scene === 'knownhostsempty'
        ? []
        : scene === 'knownhostsbig'
          ? Array.from({ length: 400 }, (_, i) => fakeKey(`host-${String(i).padStart(3, '0')}.lan`))
          : [
              fakeKey('db-01.internal'),
              fakeKey('db-01.internal', 'ecdsa-sha2-nistp256'),
              fakeKey('10.0.0.5'),
              fakeKey('[localhost]:2222'),
              fakeKey('github.com'),
              fakeKey('|1|O33ESRMWPVkMYIwJ1Uw+n877jTo=|nuuC5vEqXlEZ/8BXQR7m619W6Ak='),
              fakeKey(
                '|1|u6OyS7DMCH4RG8pJnAo/yWdyWX8=|xHCmx+reUcLXjeZyUL10tiXE99M=',
                'ssh-ed25519',
                'db-02.internal',
              ),
            ],
    other_lines: scene === 'knownhosts' ? 2 : 0,
    problem: null,
  },
  {
    store: 'certificates',
    path: '/home/you/.config/ela/known_hosts',
    entries:
      scene === 'knownhostsempty' || scene === 'knownhostsproblem'
        ? []
        : [fakeCert('[win-ad.corp]:3389'), fakeCert('[localhost]:3389')],
    other_lines: 0,
    problem:
      scene === 'knownhostsproblem'
        ? "Can't read /home/you/.config/ela/known_hosts: Permission denied (os error 13)"
        : null,
  },
];
let hostKeySetting: HostKeyCode | null = scene === 'shortcutssaved' ? 'AltRight' : null;
let terminalFontSetting: { family: string | null; size: number | null } = {
  family: scene === 'settingsmissingfont' ? 'Gone Mono' : null,
  size: null,
};
let folders: SavedFolder[] = seeded
  ? [
      {
        id: ids.production,
        name: 'Production',
        environment: 'prod',
        username: 'root',
        options: { term: 'screen-256color' },
      },
      {
        id: ids.databases,
        name: 'Databases',
        parent: ids.production,
        username: 'postgres',
        port: 2222,
      },
      {
        id: ids.staging,
        name: 'Staging',
        environment: 'staging',
        username: 'deploy',
        credential: FOLDER_SECRET,
      },
      { id: ids.lab, name: 'Lab', environment: 'lab' },
    ]
  : [];
let connections: SavedConnection[] = seeded
  ? [
      {
        id: 'c0000000-0000-4000-8000-000000000001',
        name: 'db-01',
        folder: ids.databases,
        protocol: 'ssh',
        host: 'db-01.internal',
        tags: ['eu-west'],
      },
      {
        id: 'c0000000-0000-4000-8000-000000000002',
        name: 'db-02',
        folder: ids.databases,
        protocol: 'ssh',
        host: 'db-02.internal',
        credential: SAVED_SECRET,
        tags: ['eu-west', 'postgres'],
      },
      {
        id: 'c0000000-0000-4000-8000-000000000003',
        name: 'web-1',
        folder: ids.production,
        protocol: 'ssh',
        host: 'web-1.internal',
      },
      {
        id: 'c0000000-0000-4000-8000-000000000004',
        name: 'win-ad',
        folder: ids.production,
        protocol: 'rdp',
        host: 'win-ad.corp',
        username: 'Administrator',
        options: { domain: 'CORP' },
        credential: scene === 'treevaultmissing' ? MISSING_SECRET : undefined,
      },
      {
        id: 'c0000000-0000-4000-8000-000000000005',
        name: 'api-2',
        folder: ids.staging,
        protocol: 'ssh',
        host: 'api-2.stg.internal',
      },
      {
        id: 'c0000000-0000-4000-8000-000000000006',
        name: 'pi-hole',
        folder: ids.lab,
        protocol: 'ssh',
        host: 'fail-pi.lan',
        username: 'pi',
        credential: MISSING_SECRET,
      },
      {
        id: 'c0000000-0000-4000-8000-000000000007',
        name: 'nas',
        protocol: 'ssh',
        host: 'slow-nas.lan',
        username: 'admin',
        environment: 'none',
      },
    ]
  : [];
const problems =
  scene === 'treeproblem'
    ? [
        {
          path: '/home/you/.config/ela/connections/9c1e.sync-conflict-1.toml',
          message: 'the file is named for a different id than the 9c1e it contains',
        },
      ]
    : [];
const nowSeconds = () => Math.floor(Date.now() / 1000);
const minutes = 60;
let recentEntries: { id: string; opened: number }[] =
  seeded && scene !== 'treerecentnone'
    ? [
        ['c0000000-0000-4000-8000-000000000001', 4 * minutes],
        ['c0000000-0000-4000-8000-000000000004', 2 * 60 * minutes],
        ['c0000000-0000-4000-8000-000000000005', 26 * 60 * minutes],
        ['c0000000-0000-4000-8000-000000000006', 3 * 24 * 60 * minutes],
        ['c0000000-0000-4000-8000-000000000007', 15 * 24 * 60 * minutes],
      ].map(([id, ago]) => ({ id: id as string, opened: nowSeconds() - (ago as number) }))
    : [];
const trashed: { connections: SavedConnection[]; folders: SavedFolder[] } = {
  connections: [],
  folders: [],
};

function upsert<T extends { id: string }>(list: T[], item: T): T[] {
  return [...list.filter((each) => each.id !== item.id), item];
}

function fakeUnusedCredentials() {
  const inUse = new Set([...connections, ...folders].map((each) => each.credential));
  const inTrash = [...trashed.connections, ...trashed.folders].flatMap(
    (each) => each.credential ?? [],
  );
  return [...new Set(inTrash)].filter((reference) => !inUse.has(reference));
}

function fakeChanges(item: ItemRef, to: string | null) {
  const own =
    item.kind === 'connection'
      ? connections.find((c) => c.id === item.id)
      : folders.find((f) => f.id === item.id);
  if (!own) throw 'That item no longer exists.';
  if (item.kind === 'folder' && to && folderAndInside(item.id, folders).has(to))
    throw "A folder can't be moved into itself.";
  const from =
    item.kind === 'connection' ? (own as SavedConnection).folder : (own as SavedFolder).parent;
  const [before, after] = [inheritedSettings(from, folders), inheritedSettings(to, folders)];
  const value = (field: 'port' | 'username') => ({
    field,
    before: String(own[field] ?? before[field] ?? '') || null,
    after: String(own[field] ?? after[field] ?? '') || null,
  });
  const environment = {
    field: 'environment',
    before: own.environment ?? before.environment ?? 'none',
    after: own.environment ?? after.environment ?? 'none',
  };
  return [value('port'), value('username'), environment].filter(
    (change) => change.before !== change.after,
  );
}

const DAY = 86400;
const nowS = () => Math.floor(Date.now() / 1000);
const fakeEntry = (
  directory: string,
  name: string,
  kind: 'file' | 'folder' | 'other',
  size = 0,
  age = 3600,
  link = false,
) => ({
  name,
  path: `${directory === '/' ? '' : directory}/${name}`,
  kind,
  link,
  size,
  modified: nowS() - age,
});

function fakeListing(asked: string | null) {
  const path = asked ?? '/home/you';
  const parent = path === '/' ? null : path.slice(0, path.lastIndexOf('/')) || '/';
  const folders: Record<string, ReturnType<typeof fakeEntry>[]> = {
    '/': ['home', 'root', 'var'].map((name) => fakeEntry('/', name, 'folder', 4096, 90 * DAY)),
    '/home': [fakeEntry('/home', 'you', 'folder', 4096, DAY)],
    '/home/you': [
      fakeEntry(path, '.ssh', 'folder', 4096, 40 * DAY),
      fakeEntry(path, 'backups', 'folder', 4096, 2 * DAY),
      fakeEntry(path, 'empty', 'folder', 4096, 400),
      fakeEntry(path, 'logs', 'folder', 4096, 7200, true),
      fakeEntry(path, '.bashrc', 'file', 3771, 400 * DAY),
      fakeEntry(path, 'deploy.sh', 'file', 1420, 600),
      fakeEntry(path, 'dump-2026-10-30-with-a-very-long-name.sql', 'file', 1_420_000_000, 3 * DAY),
      fakeEntry(path, 'old-link', 'other', 12, 10 * DAY, true),
    ],
    '/home/you/backups': Array.from({ length: 400 }, (_, i) =>
      fakeEntry(
        path,
        `backup-${String(i).padStart(3, '0')}.tar.gz`,
        'file',
        23_000_000 + i,
        i * DAY,
      ),
    ),
    '/home/you/empty': [],
    '/home/you/logs': [fakeEntry(path, 'app.log', 'file', 88_000, 30)],
    '/var': [],
  };
  if (path === '/root') throw 'No permission to use /root.';
  const listed = folders[path] ?? (fakeMade.has(path) ? [] : null);
  if (!listed) throw `${path} doesn't exist.`;
  const entries = [...listed, ...(fakeAdded.get(path) ?? [])]
    .filter((entry) => !fakeGone.has(entry.path))
    .sort((a, b) => Number(a.kind !== 'folder') - Number(b.kind !== 'folder'));
  return { path, parent, entries };
}

const fakeGone = new Set<string>();
const fakeMade = new Set<string>();
const fakeAdded = new Map<string, ReturnType<typeof fakeEntry>[]>();

function fakeAdd(directory: string, name: string, kind: 'file' | 'folder', size: number) {
  const taken = fakeListing(directory).entries.some((entry) => entry.name === name);
  if (taken) throw `${directory}/${name} already exists.`;
  fakeAdded.set(directory, [
    ...(fakeAdded.get(directory) ?? []),
    fakeEntry(directory, name, kind, size, 0),
  ]);
  if (kind === 'folder') fakeMade.add(`${directory}/${name}`);
}

const fakeTransfers = new Map<string, () => void>();

/** Says what the real backend would, with the native dialog assumed to be answered. */
function fakeTransfer(transfer: string, events: { id: number }, name: string, total: number) {
  let index = 0;
  const say = (message: unknown) => run(events, index++, message);
  let done = 0;
  say({ type: 'progress', name, done, total });
  if (scene === 'filestransfer') {
    say({ type: 'progress', name, done: total * 0.42, total });
    fakeTransfers.set(transfer, () => say({ type: 'cancelled', name }));
    return;
  }
  const timer = setInterval(() => {
    done = Math.min(total, done + total / 4);
    say({ type: 'progress', name, done, total });
    if (done >= total) {
      clearInterval(timer);
      say({ type: 'finished', name });
    }
  }, 60);
  fakeTransfers.set(transfer, () => {
    clearInterval(timer);
    say({ type: 'cancelled', name });
  });
}

function savedRequest(id: string, size: { width: number; height: number }) {
  const connection = connections.find((each) => each.id === id);
  if (!connection) throw 'That connection no longer exists.';
  const inherited = inheritedSettings(connection.folder, folders);
  return {
    protocol: connection.protocol,
    host: connection.host,
    port: connection.port ?? inherited.port ?? null,
    username: connection.username ?? inherited.username ?? null,
    options: { ...inherited.options, ...connection.options },
    size,
  };
}

type Live = {
  events: Channel<FrontendEvent>;
  data: Channel<ArrayBuffer>;
  ev: number;
  dt: number;
  renderer: string;
  prompt?: number | null;
  onTrust?: (trust: boolean) => void;
  onPassphrase?: (reply: Extract<PromptReply, { type: `${string}passphrase` }>) => void;
  onPassword?: (password: string, remember: boolean) => void;
  onAnswers?: (answers: string[]) => void;
  onVault?: (master: string | null) => void;
};
const sessions = new Map<string, Live>();
const run = (ch: { id: number }, index: number, message: unknown) =>
  (window as any).__TAURI_INTERNALS__.runCallback(ch.id, { index, message });
const emit = (s: Live, e: FrontendEvent) => {
  if (e.type === 'status' && e.status.state === 'asking') s.prompt = e.status.id;
  run(s.events, s.ev++, e);
};
let promptIds = 0;
const asking = (jump_host: string | null, prompt: Prompt): SessionStatus_Serialize => ({
  state: 'asking',
  id: ++promptIds,
  jump_host,
  prompt,
});
const send = (s: Live, bytes: Uint8Array) => run(s.data, s.dt++, bytes.buffer);
const text = (s: Live, str: string) => {
  const b = new TextEncoder().encode(str);
  const msg = new Uint8Array(b.length + 1);
  msg[0] = 1;
  msg.set(b, 1);
  send(s, msg);
};

function sampleShell(host: string, user: string) {
  return [
    `Last login: Fri Oct 30 22:14:09 2026 from 10.0.4.17\r\n`,
    `\x1b[1;32m${user}@${host}\x1b[0m:\x1b[1;34m~\x1b[0m$ systemctl status postgresql --no-pager | head -4\r\n`,
    `\x1b[1;32m●\x1b[0m postgresql.service - PostgreSQL RDBMS\r\n`,
    `     Loaded: loaded (/lib/systemd/system/postgresql.service; enabled)\r\n`,
    `     Active: \x1b[1;32mactive (exited)\x1b[0m since Thu 2026-10-29 03:12:44 UTC; 1 day 19h ago\r\n`,
    `\x1b[1;32m${user}@${host}\x1b[0m:\x1b[1;34m~\x1b[0m$ df -h /var/lib/postgresql\r\n`,
    `Filesystem      Size  Used Avail Use% Mounted on\r\n/dev/nvme1n1p1  1.8T  1.2T  540G  70% /var/lib/postgresql\r\n`,
    `\x1b[1;32m${user}@${host}\x1b[0m:\x1b[1;34m~\x1b[0m$ cd backups\r\n`,
    `\x1b]7;file://${host}/home/you/backups\x07`,
    `\x1b[1;32m${user}@${host}\x1b[0m:\x1b[1;34m~/backups\x1b[0m$ `,
  ].join('');
}

type DirtyRect = {
  x: number;
  y: number;
  w: number;
  h: number;
  color: (x: number, y: number) => number[];
  jpeg?: Uint8Array;
};

function frameMessage(sw: number, sh: number, rects: DirtyRect[]) {
  const pixelsAt = FRAME_HEADER_LEN + rects.length * FRAME_RECT_LEN;
  const lengths = rects.map((r) => r.jpeg?.length ?? r.w * r.h * 4);
  const msg = new Uint8Array(pixelsAt + lengths.reduce((sum, len) => sum + len, 0));
  const v = new DataView(msg.buffer);
  msg[0] = MSG_FRAME;
  [sw, sh, rects.length].forEach((n, i) => v.setUint16(1 + i * 2, n, true));
  let i = pixelsAt;
  rects.forEach((r, index) => {
    const at = FRAME_HEADER_LEN + index * FRAME_RECT_LEN;
    [r.x, r.y, r.w, r.h].forEach((n, k) => v.setUint16(at + k * 2, n, true));
    v.setUint8(at + 8, r.jpeg ? ENCODING_JPEG : 0);
    v.setUint32(at + 9, lengths[index], true);
    if (r.jpeg) {
      msg.set(r.jpeg, i);
      i += r.jpeg.length;
      return;
    }
    for (let y = r.y; y < r.y + r.h; y++)
      for (let x = r.x; x < r.x + r.w; x++, i += 4) msg.set([...r.color(x, y), 255], i);
  });
  return msg;
}

function desktopFrame(w: number, h: number) {
  const color = (x: number, y: number) =>
    y > h - 40 ? [32, 36, 44] : [20 + (x / w) * 30, 60 + (y / h) * 50, 110 + (x / w) * 60];
  return frameMessage(w, h, [{ x: 0, y: 0, w, h, color }]);
}

async function desktopUpdate(w: number, h: number) {
  const light = () => [214, 218, 226];
  const image = new OffscreenCanvas(360, 200);
  const ctx = image.getContext('2d')!;
  ctx.fillStyle = '#d6dae2';
  ctx.fillRect(0, 0, 360, 200);
  ctx.fillStyle = '#20242c';
  ctx.fillRect(16, 16, 328, 28);
  const blob = await image.convertToBlob({ type: 'image/jpeg' });
  const jpeg = new Uint8Array(await blob.arrayBuffer());
  return frameMessage(w, h, [
    { x: 24, y: 24, w: 320, h: 28, color: light },
    { x: w - 96, y: h - 30, w: 72, h: 20, color: light },
    { x: 24, y: 80, w: 360, h: 200, color: light, jpeg },
  ]);
}

export function install() {
  mockIPC(
    async (cmd, args: any) => {
      switch (cmd) {
        case 'list_protocols':
          return protocols;
        case 'parse_quick_connect': {
          const m = /^(?:(\w[\w-]*):\/\/)?(?:([^@]+)@)?([^:/]+)(?::(\d+))?/.exec(
            String(args.input).trim(),
          );
          if (!m) throw 'Type an address to connect to.';
          return {
            protocol: m[1] ?? 'ssh',
            username: m[2] ?? null,
            host: m[3],
            port: m[4] ? Number(m[4]) : null,
          };
        }
        case 'list_tree':
          return { connections, folders, problems };
        case 'save_connection': {
          const c = args.connection as SavedConnection;
          if (!c.name.trim()) throw 'A connection needs a name.';
          if (!c.host.trim()) throw 'A connection needs a host.';
          connections = upsert(connections, c);
          return null;
        }
        case 'save_folder': {
          const f = args.folder as SavedFolder;
          if (!f.name.trim()) throw 'A folder needs a name.';
          folders = upsert(folders, f);
          return null;
        }
        case 'import_ssh_config': {
          let added = 0;
          for (const [name, host, extra] of [
            ['db-01', 'db-01.internal', { username: 'postgres' }],
            ['web', 'web.example.com', { port: 2222 }],
            ['behind-gate', '10.0.0.7', { options: { proxy_jump: 'gate' } }],
          ] as const) {
            if (connections.some((c) => c.name === name)) continue;
            added++;
            connections.push({ id: crypto.randomUUID(), name, protocol: 'ssh', host, ...extra });
          }
          const notes =
            scene === 'importnotes'
              ? [
                  '/home/you/.ssh/config:14: Match exec isn’t evaluated on import, and no command is run, so its settings were left out.',
                  'web: “many” isn’t a port, so it was left out.',
                ]
              : [];
          return { added, already_saved: 3 - added, notes };
        }
        case 'trash_item': {
          const item = args.item as ItemRef;
          if (item.kind === 'connection') {
            trashed.connections.push(...connections.filter((c) => c.id === item.id));
            connections = connections.filter((c) => c.id !== item.id);
            return [item];
          }
          const inside = folderAndInside(item.id, folders);
          const goingC = connections.filter((c) => c.folder && inside.has(c.folder));
          const goingF = folders.filter((f) => inside.has(f.id));
          trashed.connections.push(...goingC);
          trashed.folders.push(...goingF);
          connections = connections.filter((c) => !goingC.includes(c));
          folders = folders.filter((f) => !goingF.includes(f));
          return [
            ...goingC.map((c) => ({ kind: 'connection', id: c.id })),
            ...goingF.map((f) => ({ kind: 'folder', id: f.id })),
          ];
        }
        case 'list_trash': {
          const inTrash = new Set(trashed.folders.map((f) => f.id));
          const items = [
            ...trashed.folders
              .filter((f) => !f.parent || !inTrash.has(f.parent))
              .map((f) => {
                const inside = folderAndInside(f.id, trashed.folders);
                inside.delete(f.id);
                return {
                  item: { kind: 'folder', id: f.id },
                  name: f.name,
                  host: null,
                  inside: [
                    ...trashed.connections
                      .filter((c) => c.folder === f.id || (c.folder && inside.has(c.folder)))
                      .map((c) => ({ kind: 'connection', id: c.id })),
                    ...[...inside].map((id) => ({ kind: 'folder', id })),
                  ],
                };
              }),
            ...trashed.connections
              .filter((c) => !c.folder || !inTrash.has(c.folder))
              .map((c) => ({
                item: { kind: 'connection', id: c.id },
                name: c.name,
                host: c.host,
                inside: [],
              })),
          ].sort((a, b) => a.name.localeCompare(b.name));
          return {
            items,
            saved_credentials: fakeUnusedCredentials().length,
            problems: scene.endsWith('trashproblem')
              ? [
                  {
                    path: '/home/you/.config/ela/trash/connections/9c1e.toml',
                    message:
                      'schema_version 99 comes from a newer Ela, which is needed to open it (this one reads up to 1)',
                  },
                ]
              : [],
          };
        }
        case 'empty_trash': {
          const unused = fakeUnusedCredentials();
          if (unused.length && vaultState === 'locked') throw 'The Vault is locked.';
          unused.forEach((reference) => secrets.delete(reference));
          const removed = trashed.connections.length + trashed.folders.length;
          trashed.connections = [];
          trashed.folders = [];
          return removed;
        }
        case 'restore_items':
          for (const item of args.items as ItemRef[]) {
            if (item.kind === 'connection') {
              connections.push(...trashed.connections.filter((c) => c.id === item.id));
              trashed.connections = trashed.connections.filter((c) => c.id !== item.id);
            } else {
              folders.push(...trashed.folders.filter((f) => f.id === item.id));
              trashed.folders = trashed.folders.filter((f) => f.id !== item.id);
            }
          }
          return null;
        case 'move_item': {
          const item = args.item as ItemRef;
          fakeChanges(item, args.to);
          if (item.kind === 'connection')
            connections = connections.map((c) =>
              c.id === item.id ? { ...c, folder: args.to ?? undefined } : c,
            );
          else
            folders = folders.map((f) =>
              f.id === item.id ? { ...f, parent: args.to ?? undefined } : f,
            );
          return null;
        }
        case 'move_changes':
          return fakeChanges(args.item, args.to);
        case 'outside_settings':
          // treeeditsshconfig stands for a host ~/.ssh/config describes.
          if (scene !== 'treeeditsshconfig' || args.host !== 'web-1.internal') return null;
          return {
            source: '~/.ssh/config',
            hostname: 'web-1.prod.example.net',
            settings: { port: 2222, options: { identity_file: ['~/.ssh/prod_ed25519'] } },
          };
        case 'resolve_connection': {
          const c = connections.find((each) => each.id === args.id);
          if (!c) throw 'That connection no longer exists.';
          const inherited = inheritedSettings(c.folder, folders);
          return {
            id: c.id,
            name: c.name,
            protocol: c.protocol,
            host: c.host,
            port:
              c.port ??
              inherited.port ??
              protocols.find((p) => p.id === c.protocol)?.default_port ??
              0,
            username: c.username ?? inherited.username ?? null,
            credential: c.credential ?? inherited.credential ?? null,
            environment: c.environment ?? inherited.environment ?? 'none',
            options: { ...inherited.options, ...c.options },
          };
        }
        case 'files_list':
          return fakeListing(args.path);
        case 'files_download':
          fakeTransfer(args.transfer, args.events, args.remote.split('/').pop(), 1_420_000_000);
          return null;
        case 'files_download_folder':
          fakeTransfer(args.transfer, args.events, args.remote.split('/').pop(), 88_000);
          return null;
        case 'files_upload':
          fakeTransfer(args.transfer, args.events, 'release.tar.gz', 23_000_000);
          return null;
        case 'files_upload_folder':
        case 'files_upload_dropped':
          fakeTransfer(args.transfer, args.events, 'site', 23_000_000);
          return null;
        case 'files_make_folder':
          if (args.directory === '/') throw 'No permission to use /.';
          fakeAdd(args.directory, args.name, 'folder', 4096);
          return null;
        case 'files_rename': {
          const directory = args.path.slice(0, args.path.lastIndexOf('/'));
          const entry = fakeListing(directory).entries.find((each) => each.path === args.path)!;
          if (entry.name === args.name) return null;
          fakeAdd(directory, args.name, entry.kind === 'folder' ? 'folder' : 'file', entry.size);
          fakeGone.add(args.path);
          return null;
        }
        case 'files_delete':
          if (scene === 'filesdeletedenied') throw `No permission to use ${args.path}/deep.`;
          fakeGone.add(args.path);
          return null;
        case 'files_cancel':
          fakeTransfers.get(args.transfer)?.();
          return null;
        case 'known_hosts_list':
          return structuredClone(fakeKnownHosts);
        case 'known_hosts_remove': {
          const section = fakeKnownHosts.find((candidate) => candidate.store === args.store)!;
          if (scene === 'knownhostsgone') {
            throw `${args.name} is no longer saved in ${section.path}.`;
          }
          section.entries = section.entries.filter(
            (entry) => entry.name !== args.name || entry.fingerprint !== args.fingerprint,
          );
          return null;
        }
        case 'vault_info':
          return {
            state: vaultState,
            idle_minutes: vaultIdleMinutes,
            idle_choices: [1, 5, 15, 30, 60, 240],
          };
        case 'vault_change_master':
          if (args.current === 'wrong') throw "That password doesn't open the Vault.";
          if (args.replacement.length < 8) throw 'The master password needs at least 8 characters.';
          vaultState = 'unlocked';
          return null;
        case 'host_key':
          return hostKeySetting;
        case 'set_host_key':
          if (scene === 'shortcutsnotsaved') {
            throw "Can't update /home/you/.config/ela/settings.toml: Permission denied (os error 13)";
          }
          hostKeySetting = args.code;
          return null;
        case 'terminal_font':
          return terminalFontSetting;
        case 'set_terminal_font':
          if (scene === 'settingsnotsaved') {
            throw "Can't update /home/you/.config/ela/settings.toml: Permission denied (os error 13)";
          }
          terminalFontSetting = args.font;
          return null;
        case 'monospace_fonts':
          return ['DejaVu Sans Mono', 'Fira Code', 'IBM Plex Mono', 'JetBrains Mono'];
        case 'vault_set_idle_minutes':
          vaultIdleMinutes = args.minutes;
          return null;
        case 'vault_create':
          if (args.master.length < 8) throw 'The master password needs at least 8 characters.';
          vaultState = 'unlocked';
          return null;
        case 'vault_unlock':
          if (args.master === 'wrong') throw "That password doesn't open the Vault.";
          vaultState = 'unlocked';
          return null;
        case 'vault_lock':
          vaultState = 'locked';
          return null;
        case 'vault_store_secret': {
          if (vaultState !== 'unlocked') throw 'The Vault is locked.';
          const reference = args.replaces ?? crypto.randomUUID();
          secrets.add(reference);
          return reference;
        }
        case 'vault_remove_secret':
          if (vaultState !== 'unlocked') throw 'The Vault is locked.';
          secrets.delete(args.reference);
          return null;
        case 'vault_key_passphrases':
          if (vaultState !== 'unlocked') throw 'The Vault is locked.';
          return keyPassphrases;
        case 'vault_forget_key_passphrase':
          if (vaultState !== 'unlocked') throw 'The Vault is locked.';
          keyPassphrases = keyPassphrases.filter((key) => key.credential !== args.credential);
          return null;
        case 'vault_credential_state':
          if (vaultState === 'locked') return 'locked';
          return vaultState === 'unlocked' && secrets.has(args.reference) ? 'available' : 'missing';
        case 'list_recent':
          return recentEntries.filter((entry) => connections.some((c) => c.id === entry.id));
        case 'forget_recent':
          recentEntries = recentEntries.filter((entry) => entry.id !== args.id);
          return null;
        case 'clear_recent':
          recentEntries = [];
          return null;
        case 'open_saved_session':
        case 'open_session': {
          if (cmd === 'open_saved_session') {
            const opened = { id: args.request.id as string, opened: nowSeconds() };
            recentEntries = [opened, ...recentEntries.filter((e) => e.id !== opened.id)].slice(
              0,
              8,
            );
          }
          const req = (
            cmd === 'open_session' ? args.request : savedRequest(args.request.id, args.request.size)
          ) as OpenRequest & { size: { width: number; height: number } };
          const id = crypto.randomUUID();
          const renderer = protocols.find((p) => p.id === req.protocol)!.renderer;
          const s: Live = { events: args.events, data: args.data, ev: 0, dt: 0, renderer };
          sessions.set(id, s);
          // Like Rust: a saved password is read before connecting, so a locked Vault asks first.
          let ready = () => {};
          const unlocked = new Promise<void>((resolve) => (ready = resolve));
          const at = (ms: number, f: () => void) => void unlocked.then(() => setTimeout(f, ms));
          const target =
            cmd === 'open_saved_session'
              ? connections.find((c) => c.id === args.request.id)
              : undefined;
          if (target?.credential && vaultState === 'locked') {
            const askVault = (wrong: boolean) =>
              emit(s, {
                type: 'status',
                status: asking(null, { kind: 'vault', wrong, for_jump_host: null }),
              });
            setTimeout(() => askVault(false), 300);
            s.onVault = (master) => {
              if (master === 'wrong') return askVault(true);
              if (master !== null) vaultState = 'unlocked';
              emit(s, { type: 'status', status: { state: 'connecting', jump_host: null } });
              ready();
            };
          } else ready();
          // RDP asks on the Tab when the Vault holds no password for it, like a typed address.
          const savedPassword = !!target?.credential && secrets.has(target.credential);
          at(10, () =>
            emit(s, { type: 'status', status: { state: 'connecting', jump_host: null } }),
          );
          const fingerprint = 'SHA256:9fQ2xR7mWc0LqZ1bVt8nKp4sHy6uJd3eGa5oFi2k1c2';
          const saved = 'SHA256:Kd+y0sfwr2RkP5L44HJyqQWIlJJBlg5VMz2jwGmREUU';
          // otp… asks for a code, otppw… for the password, otpmany… two questions with server text.
          const askQuestions = (jumpHost: string | null, then: () => void) => {
            const many = req.host.startsWith('otpmany');
            const ask = (refused: boolean) =>
              emit(s, {
                type: 'status',
                status: asking(jumpHost, {
                  kind: 'questions',
                  refused,
                  username: jumpHost ? 'ops' : (req.username ?? 'root'),
                  host: jumpHost ? `${jumpHost}.example.com` : req.host,
                  port: 22,
                  name: many ? 'Two-factor sign-in' : '',
                  instructions: many
                    ? 'Use the code from your token app.\nCodes change every 30 seconds.'
                    : '',
                  questions: req.host.startsWith('otppw')
                    ? [{ text: 'Password: ', echo: false }]
                    : many
                      ? [
                          { text: 'Verification code: ', echo: false },
                          { text: 'Token serial (last 4 digits): ', echo: true },
                        ]
                      : [{ text: 'Verification code: ', echo: false }],
                }),
              });
            at(600, () =>
              emit(s, { type: 'status', status: { state: 'authenticating', jump_host: jumpHost } }),
            );
            at(900, () => ask(req.host.startsWith('otpagain')));
            s.onAnswers = (answers) => {
              emit(s, { type: 'status', status: { state: 'authenticating', jump_host: jumpHost } });
              if (answers[0] === 'wrong') return at(400, () => ask(true));
              then();
            };
          };
          const signIn = (from: number) => {
            at(from, () =>
              emit(s, { type: 'status', status: { state: 'authenticating', jump_host: null } }),
            );
            at(from + 300, () => connected());
          };
          const jumpHost = (req.options as { proxy_jump?: string } | undefined)?.proxy_jump?.split(
            ',',
          )[0];
          if (jumpHost) {
            const status = (status: SessionStatus_Serialize) => emit(s, { type: 'status', status });
            const close = (error: SessionError) => status({ state: 'closed', error });
            const onJumpHost = (error: SessionError): SessionError => ({
              kind: 'jump_host',
              detail: { jump_host: jumpHost, host: `${jumpHost}.example.com`, port: 22, error },
            });
            const toTarget = (from: number) => {
              at(from, () => status({ state: 'authenticating', jump_host: jumpHost }));
              at(from + 400, () => status({ state: 'connecting', jump_host: null }));
              if (req.host.startsWith('fail')) {
                const address = `${req.host}:${req.port ?? 22}`;
                at(from + 900, () =>
                  close({
                    kind: 'forward_failed',
                    detail: { jump_host: jumpHost, address, failure: 'unreachable' },
                  }),
                );
                return;
              }
              at(from + 700, () => status({ state: 'verifying', fingerprint, jump_host: null }));
              if (req.host.startsWith('held')) return;
              if (!req.host.startsWith('new-')) return signIn(from + 1000);
              // The target is new as well, so the route asks a second time.
              const algorithm = renderer === 'framebuffer' ? 'certificate' : 'ssh-ed25519';
              const ask = { kind: 'trust', algorithm, fingerprint, known: null, known_types: [] };
              at(from + 1000, () => status(asking(null, ask as Prompt)));
              s.onTrust = (trust) => (trust ? signIn(0) : status({ state: 'closed', error: null }));
            };
            at(20, () => status({ state: 'connecting', jump_host: jumpHost }));
            if (req.host.startsWith('new-') || req.host.startsWith('rekeyed')) {
              const known = req.host.startsWith('rekeyed') ? saved : null;
              at(500, () =>
                status(
                  asking(jumpHost, {
                    kind: 'trust',
                    algorithm: 'ssh-ed25519',
                    fingerprint,
                    known,
                    known_types: [],
                  }),
                ),
              );
              s.onTrust = (trust) => {
                if (trust) return toTarget(0);
                if (!known) return status({ state: 'closed', error: null });
                const detail = `${fingerprint} does not match ${saved}`;
                close(onJumpHost({ kind: 'host_key_changed', detail }));
              };
              return id;
            }
            at(300, () => status({ state: 'verifying', fingerprint, jump_host: jumpHost }));
            if (req.host.startsWith('key')) {
              at(600, () => status({ state: 'authenticating', jump_host: jumpHost }));
              const key = '~/.ssh/gate_ed25519';
              const ask: Prompt = { kind: 'passphrase', key, credential: KEY_SECRET, note: null };
              at(900, () => status(asking(jumpHost, ask)));
              s.onPassphrase = (reply) => {
                if (reply.type !== 'skip_passphrase') return toTarget(0);
                const detail =
                  'The server refused ops with every way Ela tried: password, identity files and ssh-agent.';
                close(onJumpHost({ kind: 'auth_failed', detail }));
              };
              return id;
            }
            if (req.host.startsWith('otp')) {
              askQuestions(jumpHost, () => toTarget(400));
              return id;
            }
            if (req.host.startsWith('pw')) {
              let tries = 0;
              // A hop written as an address is named by its host, like a hop in Rust.
              const [user, address] = jumpHost.includes('@') ? jumpHost.split('@') : ['ops', null];
              const ask = (note: PasswordNote | null) =>
                status(
                  asking(address ?? jumpHost, {
                    kind: 'password',
                    username: user,
                    host: address ?? `${jumpHost}.example.com`,
                    port: 22,
                    note,
                    can_remember: !address,
                    tried: ['agent'],
                  }),
                );
              at(600, () => status({ state: 'authenticating', jump_host: jumpHost }));
              at(900, () => ask(req.host.startsWith('pwsaved') ? 'saved_wrong' : null));
              s.onPassword = (password, remember) => {
                status({ state: 'authenticating', jump_host: jumpHost });
                if (password !== 'wrong') {
                  const gate = connections.find((c) => c.name === jumpHost);
                  if (remember && gate) {
                    gate.credential ??= crypto.randomUUID();
                    secrets.add(gate.credential);
                  }
                  return toTarget(400);
                }
                if (++tries < 3) return at(400, () => ask('wrong'));
                const detail =
                  'The server refused ops with every way Ela tried: password, identity files and ssh-agent.';
                at(400, () => close(onJumpHost({ kind: 'auth_failed', detail })));
              };
              return id;
            }
            if (req.host.startsWith('denied')) {
              at(600, () => status({ state: 'authenticating', jump_host: jumpHost }));
              const detail =
                'The server refused ops with every way Ela tried: password, identity files and ssh-agent.';
              at(1100, () => close(onJumpHost({ kind: 'auth_failed', detail })));
              return id;
            }
            toTarget(600);
            return id;
          }
          // The password scenes stand for a server that no key signs in to.
          if (scene.startsWith('password') && cmd === 'open_saved_session') {
            const status = (status: SessionStatus_Serialize) => emit(s, { type: 'status', status });
            const target = connections.find((c) => c.id === args.request.id)!;
            let tries = 0;
            const ask = (note: PasswordNote | null) =>
              status(
                asking(null, {
                  kind: 'password',
                  username: req.username ?? 'root',
                  host: req.host,
                  port: req.port ?? 22,
                  note,
                  can_remember: true,
                  tried: ['keys', 'agent'],
                }),
              );
            at(300, () => status({ state: 'verifying', fingerprint, jump_host: null }));
            at(600, () => status({ state: 'authenticating', jump_host: null }));
            at(900, () => ask(target.credential ? 'saved_wrong' : null));
            s.onPassword = (password, remember) => {
              status({ state: 'authenticating', jump_host: null });
              if (password !== 'wrong') {
                if (remember) {
                  target.credential ??= crypto.randomUUID();
                  secrets.add(target.credential);
                }
                return signIn(400);
              }
              if (++tries < 3) return at(400, () => ask('wrong'));
              const detail = `The server refused ${req.username ?? 'root'} with every way Ela tried: password, identity files and ssh-agent.`;
              at(400, () => status({ state: 'closed', error: { kind: 'auth_failed', detail } }));
            };
            return id;
          }
          if (req.host.startsWith('revoked')) {
            at(200, () =>
              emit(s, {
                type: 'status',
                status: { state: 'verifying', fingerprint, jump_host: null },
              }),
            );
            at(500, () =>
              emit(s, {
                type: 'status',
                status: {
                  state: 'closed',
                  error: { kind: 'host_key_revoked', detail: fingerprint },
                },
              }),
            );
            return id;
          }
          if (
            req.host.startsWith('rekeyed') ||
            req.host.startsWith('new-') ||
            req.host.startsWith('newtype')
          ) {
            const known = req.host.startsWith('rekeyed') ? saved : null;
            const known_types = req.host.startsWith('newtype') ? ['ecdsa-sha2-nistp256'] : [];
            const algorithm = renderer === 'framebuffer' ? 'certificate' : 'ssh-ed25519';
            at(500, () =>
              emit(s, {
                type: 'status',
                status: asking(null, { kind: 'trust', algorithm, fingerprint, known, known_types }),
              }),
            );
            s.onTrust = (trust) => {
              if (trust) return signIn(0);
              const kind =
                renderer === 'framebuffer'
                  ? ('certificate_untrusted' as const)
                  : ('host_key_changed' as const);
              const error = known
                ? { kind, detail: `${fingerprint} does not match ${saved}` }
                : null;
              emit(s, { type: 'status', status: { state: 'closed', error } });
            };
            return id;
          }
          if (req.host.startsWith('otp')) {
            at(300, () =>
              emit(s, {
                type: 'status',
                status: { state: 'verifying', fingerprint, jump_host: null },
              }),
            );
            askQuestions(null, () => signIn(400));
            return id;
          }
          if (req.host.startsWith('key')) {
            const status = (status: SessionStatus_Serialize) => emit(s, { type: 'status', status });
            const ask = (note: PassphraseNote | null) =>
              status(
                asking(null, {
                  kind: 'passphrase',
                  key: '~/.ssh/id_ed25519',
                  credential: KEY_SECRET,
                  note,
                }),
              );
            at(300, () => status({ state: 'verifying', fingerprint, jump_host: null }));
            at(600, () => status({ state: 'authenticating', jump_host: null }));
            at(900, () => ask(req.host.startsWith('keysaved') ? 'saved_wrong' : null));
            s.onPassphrase = (reply) => {
              if (reply.type === 'skip_passphrase') {
                const detail = `The server refused ${req.username ?? 'root'} with every way Ela tried: password, identity files and ssh-agent.`;
                return status({ state: 'closed', error: { kind: 'auth_failed', detail } });
              }
              status({ state: 'authenticating', jump_host: null });
              if (reply.type === 'saved_passphrase')
                return secrets.has(KEY_SECRET) ? signIn(300) : ask(null);
              if (reply.passphrase === 'wrong') return at(400, () => ask('wrong'));
              if (reply.remember) secrets.add(KEY_SECRET);
              signIn(400);
            };
            return id;
          }
          if (req.host.startsWith('fail')) {
            at(700, () =>
              emit(s, {
                type: 'status',
                status: {
                  state: 'closed',
                  error: { kind: 'host_unreachable', detail: 'connection refused' },
                },
              }),
            );
            return id;
          }
          if (req.host.startsWith('slow')) return id; // stays in "Connecting" for review
          if (req.host.startsWith('held')) {
            at(300, () =>
              emit(s, {
                type: 'status',
                status: { state: 'verifying', fingerprint, jump_host: null },
              }),
            );
            return id;
          }
          at(300, () =>
            emit(s, {
              type: 'status',
              status: { state: 'verifying', fingerprint, jump_host: null },
            }),
          );
          // Scenes that open a desktop by address stand for one the Vault signs in to, except ask-….
          const asksPassword = target ? !savedPassword : req.host.startsWith('ask-');
          if (renderer === 'framebuffer' && asksPassword) {
            let tries = 0;
            const askPassword = (note: PasswordNote | null) =>
              emit(s, {
                type: 'status',
                status: asking(null, {
                  kind: 'password',
                  username: req.username ?? '',
                  host: req.host,
                  port: req.port ?? 3389,
                  note,
                  can_remember: !!target,
                  tried: [],
                }),
              });
            at(600, () => askPassword(target?.credential ? 'missing' : null));
            s.onPassword = (password, remember) => {
              emit(s, { type: 'status', status: { state: 'authenticating', jump_host: null } });
              if (password === 'wrong' && ++tries < 3) return askPassword('wrong');
              if (remember && target) {
                target.credential ??= crypto.randomUUID();
                secrets.add(target.credential);
              }
              signIn(300);
            };
            return id;
          }
          signIn(600);
          function connected() {
            emit(s, { type: 'status', status: { state: 'connected' } });
            if (req.protocol === 'bench-term') {
              const line =
                '\x1b[32m  12345\x1b[0m the quick brown fox jumps over the lazy dog 0123456789\r\n';
              const chunk = line.repeat(Math.ceil(65536 / line.length));
              setInterval(() => text(s, chunk), 20);
            } else if (renderer === 'terminal')
              text(s, sampleShell(req.host, req.username ?? 'root'));
            else {
              const [w, h] = [Math.min(req.size.width, 1600), Math.min(req.size.height, 1000)];
              send(s, desktopFrame(w, h));
              at(300, () => void desktopUpdate(w, h).then((msg) => send(s, msg)));
            }
          }
          let produced = 0;
          setInterval(() => {
            produced += 60;
            emit(s, {
              type: 'stats',
              stats: {
                produced,
                sent: Math.round(produced * 0.8),
                bytes: produced * 400_000,
                take_ns: 0,
                encode_ns: 0,
                send_ns: 0,
                ack_wait_ns: 0,
                credit_wait_ns: 0,
              },
            });
          }, 1000);
          return id;
        }
        case 'session_input': {
          const s = sessions.get(args.id);
          if (s && args.command.type === 'text') text(s, args.command.data.replace(/\r/g, '\r\n'));
          return null;
        }
        case 'session_answer': {
          const s = sessions.get(args.id);
          // Like Rust: only the open prompt takes an answer.
          if (!s || s.prompt !== args.prompt) return null;
          s.prompt = null;
          const reply: PromptReply = args.reply;
          if (reply.type === 'trust') s.onTrust?.(reply.trust);
          else if (reply.type === 'password') s.onPassword?.(reply.password, reply.remember);
          else if (reply.type === 'answers') s.onAnswers?.(reply.answers);
          else if (reply.type === 'vault') s.onVault?.(reply.master);
          else if (reply.type === 'without_vault') s.onVault?.(null);
          else if (reply.type.endsWith('passphrase')) s.onPassphrase?.(reply as never);
          return null;
        }
        case 'save_screenshot':
          return '/home/you/Pictures/Ela/screenshot-1793440000000.png';
        case 'close_session': {
          const s = sessions.get(args.id);
          if (s) emit(s, { type: 'status', status: { state: 'closed', error: null } });
          return null;
        }
        default:
          return null;
      }
    },
    { shouldMockEvents: true },
  );
}
