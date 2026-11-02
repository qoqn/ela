# Ela architecture

Ela is a remote connection manager (SSH, RDP, VNC, Telnet, Serial, SFTP, external Launchers) in the spirit of MobaXterm and Remmina. It draws ideas from [Remota](https://github.com/privum-cloud/remota) (Tauri UI, encrypted Vault, Folder Inheritance, NAT relay) and [RustConn](https://github.com/totoshko88/RustConn) (native protocol clients, breadth of Protocols, Launchers, split view, CLI).

Vocabulary follows [GLOSSARY.md](../GLOSSARY.md). Decisions with lasting consequences are recorded in [adr/](./adr/).

## 1. Shape

```
┌────────────────── UI (Tauri webview · Svelte 5 · TypeScript) ──────────────────┐
│  Terminal renderer (xterm.js) · Framebuffer renderer (<canvas>) · File Browser  │
│  Connection tree · Tabs/Panes · Command palette · Tool Panels                   │
└────────────▲ SessionEvent (binary Channel)          │ SessionCommand / commands ─┘
             │                                        ▼
┌──────────────────────────── app/src-tauri (thin) ──────────────────────────────┐
│  Tauri commands + Channels. No protocol logic, no Secrets ever sent upward.     │
└───────────────────────────────────────┬─────────────────────────────────────────┘
                                        ▼
┌───────────────────────────────── ela-session ──────────────────────────────────┐
│  SessionManager · ProtocolRegistry · event bus · Session state machine          │
├──────────┬──────────┬──────────┬──────────────┬──────────────┬─────────────────┤
│ ela-ssh  │ ela-rdp  │ ela-vnc  │ ela-telnet / │ ela-launcher │ (future crates) │
│ russh    │ IronRDP  │ vnc-rs   │ ela-serial   │ FreeRDP, …   │ SPICE, S3, …    │
├──────────┴──────────┴──────────┴──────────────┴──────────────┴─────────────────┤
│ ela-transport (direct, Jump Host chain, SOCKS, relay, cloud bastions)           │
├─────────────────────────────────────────────────────────────────────────────────┤
│ ela-core (model, Inheritance) · ela-vault (Secret Stores) · ela-import          │
└─────────────────────────────────────────────────────────────────────────────────┘
                 ela-cli uses the same crates without any UI.
```

All protocol work happens in Rust. The webview only draws what it is told and sends input back ([ADR-0001](./adr/0001-native-protocols-behind-a-dumb-webview.md)).

## 2. Workspace layout

```
ela/
├─ crates/
│  ├─ ela-core/       Connection, Folder, Tag, Template, Inheritance resolver, storage + migrations, Trash
│  ├─ ela-vault/      Vault, SecretStore trait, encrypted-file / libsecret / KeePassXC backends
│  ├─ ela-session/    Protocol + Session traits, ProtocolRegistry, SessionManager, event bus
│  ├─ ela-transport/  routes: Jump Host chains planned from saved Connections; later SOCKS, relay
│  ├─ ela-workspace/  opens Sessions from saved Connections and addresses: Inheritance, routes, Secrets, Recent
│  ├─ ela-ssh/        shell, SFTP, Tunnels, X11 forwarding, Known Hosts, ~/.ssh/config, ssh-agent
│  ├─ ela-rdp/        IronRDP client, InputMapper (scancodes/layouts), clipboard
│  ├─ ela-vnc/        vnc-rs client
│  ├─ ela-telnet/, ela-serial/
│  ├─ ela-launcher/   Launchers declared in TOML
│  ├─ ela-import/     Importer trait + ssh_config, Remmina, MobaXterm, mRemoteNG
│  ├─ ela-bench/      synthetic Protocols for measuring Renderer throughput
│  └─ ela-cli/
├─ app/
│  ├─ src-tauri/      thin adapter: commands, Channels, window management
│  └─ src/            Svelte 5 frontend (see §6)
│     ├─ lib/ui/        design system: tokens + primitives (docs/design/components.md)
│     ├─ lib/shell/     app frame: activity rail, tab strip, status bar
│     ├─ lib/features/  sessions, sidebar, welcome, tree, files, vault, knownhosts, later tunnels
│     └─ dev/           browser preview backend + review scenes (dev only)
├─ dev/               Docker test servers (SSH, RDP)
└─ docs/
   ├─ architecture.md, adr/
   └─ design/         UI/UX source of truth
```

Implemented so far: `ela-core`, `ela-vault` (the encrypted file only), `ela-import` (`~/.ssh/config` only), `ela-transport` (Jump Host routes only), `ela-workspace`, `ela-session`, `ela-ssh`, `ela-rdp`, `ela-bench` and the app. The other crates are planned.

Rule: `app/src-tauri` must stay replaceable. If the webview turns out too slow, only `app/` is rewritten (for example as GTK4); `crates/` stay unchanged.

## 3. Extension points

| Extension | Trait | Registered in |
|---|---|---|
| Protocol | `Protocol` | `ProtocolRegistry` |
| Secret Store | `SecretStore`, once a second backend exists | `Vault` |
| Transport | `Transport` (signs in to the Jump Hosts) and `Route` (opens streams through them) | Given to each Protocol that takes a route, e.g. `RdpProtocol::through_jump_hosts` |
| Importer | `Importer`, once a second source exists | `ela-import` |
| Tool Panel | `ToolPanel` (backend data) + UI component | UI panel registry |

```rust
pub trait Protocol: Send + Sync {
    fn id(&self) -> &'static str;                  // "ssh", "rdp"
    fn display_name(&self) -> &'static str;
    fn default_port(&self) -> Option<u16>;
    fn renderer(&self) -> RendererKind;
    fn capabilities(&self) -> Capabilities;        // clipboard, file transfer, resize…
    fn settings_schema(&self) -> serde_json::Value; // drives the Connection editor form
    fn connect(&self, cfg: ResolvedConfig, ctx: ConnectContext) -> BoxFuture<'_, Result<Box<dyn Session>>>;
}

pub enum RendererKind { Terminal, Framebuffer, FileBrowser, Web, External }

#[async_trait]
pub trait Session: Send {
    async fn run(self: Box<Self>, cmds: mpsc::Receiver<SessionCommand>, events: SessionSink) -> Result<()>;
    fn files(&self) -> Option<Arc<dyn RemoteFiles>>;   // browse, transfer, rename, delete on this Session's connection
}
```

Dropping files on the window uses Tauri's native drag handling, the only way to learn their paths. On Linux it lets the Connection tree's HTML5 drags through. Tauri documents that on Windows it doesn't, so a Windows build needs `dragDropEnabled: false` until that is solved.

`RendererKind::FileBrowser` stays for a Protocol that is only files (S3, plain SFTP). Files on an SSH Session is a sidebar view through `Session::files`, not a Renderer.

The frontend knows Renderers, not Protocols ([ADR-0003](./adr/0003-frontend-knows-renderers-not-protocols.md)). Adding SPICE means a new crate that uses `RendererKind::Framebuffer`, with no frontend change. Protocol options live in `Connection.options` as JSON validated against `settings_schema()`, so new Protocols need no storage migration.

Plugin levels, in order of adoption ([ADR-0006](./adr/0006-compile-time-protocols-first.md)):
1. Compile-time crates behind Cargo features.
2. Declarative Launchers in TOML (no code).
3. Runtime plugins (WASM via extism/wasmtime, or a subprocess using JSON-RPC). Deferred until there are third-party authors.

## 4. Session lifecycle

```
Idle → Resolving (Inheritance, Secrets) → Connecting (Transport) → Verifying (Known Host)
     → Authenticating → Connected ⇄ Reconnecting → Closed(reason)
```

- Every Session owns a `CancellationToken`; disconnect cancels the whole task tree.
- `Closed(reason)` carries a typed error: `HostUnreachable`, `Timeout`, `AuthFailed`, `HostKeyChanged`, `CertificateUntrusted`, `ProtocolError`, `Cancelled`, and for routes `JumpHost` (one of these, on a named Jump Host) and `ForwardFailed` (a Jump Host couldn't open the next leg). The UI explains each one in plain words.
- **Resolving.** `ela-workspace` turns a saved Connection's ID or a typed address into a Session: Inheritance and the route are worked out before the Tab opens (an unreadable route opens none), and the Secrets are read inside the Session through `SessionManager::open_with`, so a locked Vault becomes a `vault` prompt on the Tab ("Connect without it" goes on with none). A Secret the Vault lost reaches the Protocol as `ResolvedConfig::saved_password_lost`, so its password prompt can say so. `app/src-tauri` and a future `ela-cli` only hand it an ID or an address.
- **Jump Hosts.** `ela-workspace` plans the route with `ela_transport::plan` before opening: `proxy_jump` is read like OpenSSH's `ProxyJump`, names are looked up among saved SSH Connections, and their Secrets come from the Vault. The plan travels in `ResolvedConfig::jump_hosts`. `ela-ssh` then runs Connecting, Verifying and Authenticating once per Jump Host, each through a `direct-tcpip` channel of the previous one, and finally for the target. Statuses carry `jump_host` while a step is about a Jump Host, and `null` once it is about the target. Every leg signs in the same way (see **Sign-in** below), and a password typed for a Jump Host is sent only on that leg. The Session keeps every Jump Host's connection open until it ends. `ela-ssh` refuses a `proxy_jump` that arrives without a plan rather than going direct.
- **Sign-in.** `ela-ssh` tries, in order, the identity files the leg names, its saved password, the keys ssh-agent holds, and the default key files in `~/.ssh` (`id_rsa`, `id_ecdsa`, `id_ed25519`), skipping a key already tried. A named key goes first because someone chose it, and the password before ssh-agent because an agent with many keys can use up the server's MaxAuthTries first. ssh-agent is `SSH_AUTH_SOCK`, or without it the first socket under `$XDG_RUNTIME_DIR` that holds keys (systemd's ssh-agent, gcr, gnome-keyring, gpg-agent), since a desktop launch may not export it. A default key whose public part can't be read without its passphrase is skipped rather than asked for. When nothing signed in and the server offers password sign-in, the Tab asks (a `password` prompt, `jump_host` `null` for the target), three times. A leg whose `ResolvedConfig::connection` is set, meaning a saved Connection's own account, may offer to remember the password; once it signs in, the Protocol hands it to `SessionSink::keep_password`, and `ela-workspace` saves it in the Vault as the Connection's Credential Reference. RDP asks the same way once its certificate was checked, three times, and remembers through the same `SavedPasswords`, which the `SessionManager` holds for every Protocol (`SessionSink::keep_password`). Nothing asks before connecting. Tests hand `SshProtocol::finding_keys` a `KeySources` of their own so the keys of whoever runs them stay out.
- **Certificates, banner, terminal.** A key with `<key>-cert.pub` beside it, and a certificate ssh-agent holds, is offered as the certificate first and then as the plain key, as OpenSSH does, and a locked key's passphrase is asked once for both. The server's pre-login banner is written to the terminal before anything else, with control characters removed as OpenSSH's `strnvis` does, since it is untrusted text. The PTY carries the terminal modes OpenSSH sends from a UTF-8 terminal (`IUTF8`, `VERASE` 127, the usual control characters), and the PTY and shell requests wait for the server's answer, so a refusal ends the Session with that reason. Without a username the local account's name comes from the account database, not `$USER`.
- **Keyboard-interactive.** Where a server offers it, `ela-ssh` answers each hidden question containing "password" with the leg's saved or typed password, once, and asks the Tab for every other question (a `questions` prompt, one answer per question, write-only like a password). A refused round starts over, up to three rounds, and a leg with no password asks every question. A Jump Host whose server offers only keyboard-interactive asks for its password as above, and that password answers the password question.
- **Prompts.** Anything a Session needs from the user goes through `SessionSink::ask`: the status becomes `Asking { id, jump_host, prompt }`, where `prompt` is a `trust`, `passphrase`, `password` or `questions` prompt, and the Session waits. One command, `session_answer`, carries the answer and must name the prompt's `id`; an answer to a prompt that is no longer open, or of another kind, is dropped, so a late or doubled submit can't reach the next host on a route. A Session has at most one open prompt. A `password` prompt lists what was `tried` before it (`keys`, `agent`), so the panel's copy needs no knowledge of the Protocol.
- **Reaching a host.** Every first leg, SSH's included, goes through `ela_transport::Direct`: one `CONNECT_TIMEOUT` (20 s, paused while the user is asked), the resolver's addresses tried the RFC 8305 way (families taking turns, the next address started after 250 ms or as soon as one fails, the first to connect kept), `TCP_NODELAY`, and TCP keepalive after 30 s idle so a NAT that forgets the link ends the Session. The Jump Hosts field (`proxy_jump`) is `ela_transport::route_field`, the same in every Protocol, and a Jump Host takes from the Connection the options its Protocol's schema groups under `sign_in`, so `ela-transport` names none.
- **Option lists.** A schema field of type `list` is an array of strings in the TOML file (`identity_file`, `proxy_jump`), shown one entry per line. Comma-separated text, which Ela wrote before, is still read by `ResolvedConfig::option_list`; `proxy_jump` written as one line is read strictly, as OpenSSH reads `ProxyJump`.
- Other Protocols take the same route through `ela_transport::route`: with no Jump Hosts it is `Direct` (plain TCP), otherwise the `Transport` that `SshProtocol::transport` returns signs in to each Jump Host exactly as for SSH and hands back a `Route` whose streams are `direct-tcpip` channels of the last one. RDP opens its TLS over such a stream and keeps the `Route` for the Session, since a redirect reconnects through it. Without a `Transport` a planned route is refused, never skipped.
- Keepalive and auto-reconnect handle sleep/resume and network changes.
- `SessionManager` has no knowledge of windows or webviews. Tabs can detach into new windows, and the UI can reload without killing Sessions. This leaves room for a later daemon mode (Sessions outliving the window, shared with `ela-cli`).

## 5. Data flow and backpressure

- **Terminal output** is coalesced on a ~16 ms tick and sent as raw bytes over a Tauri `Channel`. The channel is bounded, and a full channel slows the reader instead of growing memory.
- **Framebuffer updates** use "latest frame wins" ([ADR-0007](./adr/0007-framebuffer-latest-frame-wins.md)): dirty rectangles accumulate into one pending update of up to 16 rectangles (`tokio::sync::watch`-style), and the UI takes the newest once it has painted the last, painting on arrival. A Tab out of view holds on to its credit, so only the newest update is left when it shows again. Intermediate frames are dropped. Large rectangles travel as JPEG while they change and are sent again losslessly once they settle.
- **Wire format** of data-channel messages (`crates/ela-session/src/wire.rs`, read by `FramebufferView` and `TerminalView`), little-endian. Terminal: `[1] bytes…`. Frame: `[2] surface_w:u16 surface_h:u16 count:u16`, then `count` times `x:u16 y:u16 w:u16 h:u16 encoding:u8 len:u32`, then the `len` bytes of each rect in the same order. Raw rects (`encoding` 0) hold RGBA pixels, JPEG rects (`encoding` 1) a complete JPEG image.
- **Event bus.** Every `SessionEvent` also goes through an internal bus. Session logging/recording (asciicast), expect rules, pre/post-connect hooks, Broadcast and future scripting subscribe to it instead of patching Protocols.

## 6. Frontend

Svelte 5 (runes) with TypeScript, built by Vite as a plain SPA (no SvelteKit) ([ADR-0005](./adr/0005-svelte-frontend.md)). How it looks and behaves is specified in [design/](./design/README.md); this section only covers the technology.

**Browser preview.** `bun run dev` opened in a normal browser (outside Tauri) runs against a fake backend in `app/src/dev/`, with review scenes such as `?scene=busy` and `?scene=failure`. This lets UI work and design review happen without compiling Rust. The Protocols' option schemas reach it as `src/dev/settings-schemas.json`, written by the same test that keeps `bindings.ts` current, so its forms match the app's.

| Concern | Package |
|---|---|
| Framework / build | `svelte` 5, `vite`, `@sveltejs/vite-plugin-svelte`, `typescript`, `bun` (package manager and script runner) |
| Tauri | `@tauri-apps/api`; plugins: `dialog`, `clipboard-manager`, `window-state`, `updater`, `process`, `os`, `log`, `global-shortcut`, `notification` |
| Rust↔TS types | `tauri-specta` + `specta-typescript`. The Rust side generates `app/src/lib/bindings.ts`, and a test fails when it is stale |
| Terminal | `@xterm/xterm`, `@xterm/addon-webgl`, `-fit`, `-search`, `-web-links`, `-unicode11`, `-serialize`, `-image` |
| Framebuffer | plain `<canvas>` 2D (`putImageData` per dirty rect, `createImageBitmap` + `drawImage` for JPEG ones); OffscreenCanvas + Worker only if profiling demands it |
| Styling | `tailwindcss` v4 (`@tailwindcss/vite`), with the default palette replaced by Ela tokens |
| Fonts | `@fontsource/ibm-plex-sans`, `-sans-condensed`, `-mono` (bundled, no network) |
| Components | Hand-built in `lib/ui/` on native elements, fully owned |
| Icons | `@lucide/svelte` |
| Split Panes | `paneforge` |
| Connection tree drag & drop | `@atlaskit/pragmatic-drag-and-drop` |
| Long lists / big trees | `virtua` |
| Command palette, toasts | Hand-built (`lib/shell/CommandPalette.svelte`, `lib/ui/Toast.svelte`) |
| Keybindings | `tinykeys` |
| i18n | `@inlang/paraglide-js` (compile-time, typed messages) |
| State | Svelte runes in `*.svelte.ts` classes; no store library |
| Lint / format | `eslint` + `eslint-plugin-svelte`, `prettier` + `prettier-plugin-svelte`, `svelte-check` |
| Tests | `vitest`, `@testing-library/svelte`; end-to-end with WebdriverIO + `tauri-driver` |

The Connection editor form is generated from the Protocol's `settings_schema()` by a small in-house renderer. Validation is authoritative in Rust.

Layout, interaction and visual design are specified in [design/layout.md](./design/layout.md) and its siblings.

## 7. Security

- **The webview never sees Secrets** ([ADR-0002](./adr/0002-webview-never-sees-secrets.md)). It only handles Credential References. Secrets live in Rust memory wrapped in `zeroize`/`secrecy` types.
- The Vault locks after the chosen idle time (15 minutes by default, 4 hours at most) since it was last used, by wall-clock time so that suspend counts, and forgets its key and Secrets. Open Sessions stay open. If the setting can't be read or isn't one Ela offers, the default applies.
- Changing the master password asks for the current one, then writes the Secrets under a new salt and key in one atomic replace, so a crash leaves one password or the other working. Copies of the old file, such as backups, still open with the old password.
- A key's passphrase is a Secret like a password. It is typed into the passphrase panel and goes to Rust once, and is saved in the Vault only after it unlocked the key, under a Credential Reference derived from the key's fingerprint (`CredentialRef::for_key`) and labelled with the key's path and fingerprint, so the Vault panel can list it and forget it. One saved before labels existed is labelled the next time it is read. Only a Secret labelled as a key's passphrase can be forgotten that way; a Connection's password goes with its Connection. `ela-ssh` reaches the Vault through its `Passphrases` trait, so it doesn't depend on `ela-vault`.
- The master password can't be recovered, and costs an attacker Argon2id at 64 MiB, 3 passes, 4 lanes (RFC 9106's second recommendation) per guess.
- Known Hosts: SSH uses trust-on-first-use and stays in sync with `~/.ssh/known_hosts`, read the way OpenSSH reads it: `*`/`?` patterns, `!` negations, hashed names, `@revoked` keys refused outright, host certificates trusted through `@cert-authority` lines, and `~/.ssh/known_hosts2` and `/etc/ssh/ssh_known_hosts{,2}` read but never written. Host key algorithms already saved for a host are asked for first, as OpenSSH orders `HostKeyAlgorithms`, and certificates only where an authority covers the host. New entries are hashed when every entry in the file already is. Every Jump Host on a route is checked under the name and port it is reached by, like a Connection of its own. RDP and VNC-over-TLS use certificate pinning for certificates no trusted authority vouches for, under the name and port the last Jump Host reaches the server by. An RDP redirect is followed only through RDSTLS, to the same host and port, and only to the certificate the redirecting server named; the one-time sign-in it hands over is passed on as-is and never kept. A changed identity blocks the connection behind an explicit, loud prompt. Entries are removed by naming the store and the entry over IPC, never a path, and the previous file stays as `known_hosts.old`.
- Tauri v2 capabilities are minimal and per-window. A strict CSP is set, and no remote content loads in the main webview (the Web Renderer uses a separate, isolated webview).
- Local paths for transfers are chosen in a native dialog in Rust and never cross IPC, so the webview can't name a file to read or overwrite. A server-supplied filename is reduced to a plain name before it suggests a local one.
- Anything that came from a remote host (titles, filenames, banners) is treated as untrusted text and never interpolated as HTML.

## 8. Storage

Configuration and Secrets are stored separately, and configuration is stored one file per item ([ADR-0004](./adr/0004-config-split-from-secrets-one-file-per-item.md)).

```
~/.config/ela/
├─ settings.toml               the Vault's idle time, the host key and the terminal font, keeping keys this version doesn't know
├─ connections/<uuid>.toml     one file per Connection
├─ folders/<uuid>.toml         one file per Folder
├─ trash/                      deleted items, in the same layout, not loaded
├─ launchers/*.toml
└─ known_hosts                 (non-SSH identities; SSH also uses ~/.ssh/known_hosts)
~/.local/share/ela/
├─ vault.bin                   encrypted Secret Store (AES-256-GCM, Argon2id)
├─ recent.toml                 Recent: up to 8 `[[opened]]` tables of `id` and `opened` (seconds since the Unix epoch), newest first
└─ logs/, recordings/
```

Every file carries `schema_version`, and `ela-core` owns the migrations. All IDs are UUIDs.

`vault.bin`, little-endian: `"ELAVAULT"`, format `2` as a byte, Argon2id `memory_kib:u32 iterations:u32 parallelism:u32`, `salt[16]`, `nonce[12]`, then the AES-256-GCM ciphertext and tag. All that comes before the ciphertext is authenticated, and the nonce is new on every write. The plaintext repeats `uuid[16] len:u32 utf8[len] label_len:u32 label[label_len]`, one per Secret. The label says what a Secret belongs to when no Connection names it, and is empty otherwise: a key's passphrase is labelled `{"kind":"key_passphrase","path":…,"fingerprint":…}`, so the Vault panel can list it. Format `1` had no label; it is still read, and the next write makes it format `2`. Writes replace the file atomically, readable only by its owner.

`~/.ssh/config` is read again at every SSH connect, as the lowest layer of Inheritance (`ela_core::Outside`, below every Folder), by `ela-workspace` through `ela_import::SshConfig`: a Connection's host, a typed address and every Jump Host are looked up as `ssh <host>` would, with `Include`, `Match` (never `exec`), wildcards and `%` tokens. `HostName`, `Port`, `User`, `IdentityFile`, `CertificateFile`, `IdentitiesOnly`, `IdentityAgent`, `UserKnownHostsFile`, `HostKeyAlias`, `ServerAliveInterval`, `ProxyJump`, `ProxyCommand` (refused) and the algorithm lists (`HostKeyAlgorithms`, `KexAlgorithms`, `PubkeyAcceptedAlgorithms`, `Ciphers`, `MACs`, in OpenSSH's `+`/`-`/`^` syntax) count, under the option names the SSH schema uses, so each can also be set on a Connection or Folder. Whatever the Connection or its Folders set wins; what the typed address says wins over the file. Importing therefore saves only each `Host`'s name, and the file goes on describing it. Ela does not keep its own parallel SSH world. It uses the running ssh-agent / gpg-agent socket and the default key files, after any `identity_file` named on the Connection and its saved password (see §4). A key with a passphrase is signed with the agent when the agent holds it, and otherwise asks for its passphrase once the server accepts it.

## 9. Cross-cutting concerns

- **Input.** An `InputMapper` per Framebuffer Protocol handles scancodes and keyboard layouts. A configurable "host key" separates app shortcuts from remote shortcuts (e.g. `Ctrl+W`). Fullscreen uses keyboard grab (Wayland: keyboard-shortcuts-inhibit).
- **HiDPI / multi-monitor.** Remote resolution is negotiated from logical size × scale factor, with the user's choice of fit, 1:1 or dynamic.
- **Observability.** `tracing` uses one span per Session. "Export debug bundle" collects logs with Secrets redacted, plus versions and OS.
- **i18n.** No user-facing string is hard-coded, from the first component on.
- **Packaging.** AppImage, deb and rpm come first. Flatpak needs extra work: access to `~/.ssh`, the agent socket, and `flatpak-spawn --host` for Launchers.
- **Testing.** A docker-compose fixture (openssh-server, xrdp, tigervnc, telnetd) runs integration tests in CI. A `MockSession` lets the UI be tested without a network.

## 10. Roadmap

1. **Spike.** Tauri + xterm.js + russh, and IronRDP → canvas at 1080p. Measure performance in WebKitGTK and decide go/no-go for the webview ([ADR-0001](./adr/0001-native-protocols-behind-a-dumb-webview.md)).
2. **MVP.** Connection tree, Vault, SSH + SFTP, Known Hosts, Quick Connect, `~/.ssh/config` import.
3. RDP, VNC, Tunnels, split Panes, snippets, Session logging.
4. Launchers (SPICE, mosh, Kubernetes, cloud bastions), Broadcast, host metrics, `ela-cli`.
5. NAT relay + agent (separate project).
