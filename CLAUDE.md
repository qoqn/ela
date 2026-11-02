# Ela

Remote connection manager (SSH, RDP, VNC): Rust crates under `crates/`, Tauri 2 + Svelte 5 app under `app/`.

## Before changing an area

- Any crate or IPC: `docs/architecture.md` and the ADRs in `docs/adr/`.
- IPC types are generated. After changing a command or a type crossing IPC: `ELA_UPDATE_BINDINGS=1 cargo test -p ela-app bindings`.
- Any UI: `docs/design/` is the source of truth for look, behaviour and copy. Change design docs and code in the same commit.
- Naming: `GLOSSARY.md`. A Connection is saved, a Session is live, a Tab is where a Session is shown.
- Releases: `docs/releasing.md`. Changes a user would notice go under "Unreleased" in `CHANGELOG.md`.

## House rules

- **Commits** are Conventional Commits, scoped by area where one fits: `fix(rdp): …`. Add a body when the reason isn't visible in the diff.
- JavaScript tooling is **bun** (`bun add`, `bun run`, `bunx`).
- Port 1420 belongs to the user's own `bun run dev` / `bun tauri dev`. For your preview screenshots run `bunx vite --port 1430 --strictPort` in `app/` and stop it by PID afterwards. Leave other Vite processes alone, and hold frontend edits while the user says they are testing in the running app (Vite hot-reloads under them).
- Secrets only travel UI → Rust, once, write-only (ADR-0002). The frontend switches on `RendererKind`, never on protocol IDs (ADR-0003).
- **Comments are rare and say why.** Write one only for what the code can't show: a reason, a constraint, a trap, a choice that looks wrong but isn't. Never restate what the code does, and this includes doc comments on functions, fields and modules. Name documents rather than giving their path, which goes stale when a file moves: `ADR-0007`, `design: Keyboard ownership`. Names and tests explain the rest; behaviour and formats belong in `docs/`. Tool directives such as `svelte-ignore` are fine.
- Colours come from tokens in `app/src/lib/ui/tokens.css`. The theme follows VS Code "Dark 2026". Check contrast numerically when tokens change.

## Verifying

```sh
cargo fmt --all --check && cargo clippy --workspace --all-targets && cargo test --workspace
cd app && bun run check && bun run test
```

- **Formatting is a gate.** `cargo fmt --all` for Rust and `bun fmt` in `app/` for the frontend (it formats Svelte natively; settings in `app/bunfig.toml`). `.githooks/pre-commit` refuses unformatted code and `bun run check` fails on it. `bun install` turns the hook on.

- **Fixture tests** run against real servers: `cd dev && ./gen-keys.sh && docker compose up -d`, then `cargo test -p ela-ssh -p ela-rdp -- --ignored`. The RDP test reattaches to the `ela` desktop session and disconnects whoever is using it, so ask first if the user may be connected.
- **UI review** goes through the browser preview, which has a fake backend. Scenes: `?scene=busy|connecting|cancelled|dropped|desktop|failure|reconnect|devtools|hostkey|hostkeynewtype|hostkeyrevoked|firstconnect|certchanged|released|shortcuts|shortcutssaved|shortcutsnotsaved|palette|settings|settingsvault|settingsnotsaved|settingsmissingfont|flood|vault|vaultlocked|vaultnew|vaultchange|vaultchangewrong|vaultchanged|vaultkeys|vaultkeysforget|vaultkeysforgotten|importask|importnotes|files|filesnone|filesrdp|filesended|filesdenied|filesbig|filesempty|filestransfer|filesfolder|filesdrop|filesmenu|filesnewfolder|filesrename|filesnametaken|filesdelete|filesdeletefolder|filesdeletelink|filesdeletedenied|terminalmenu|terminalmenucopy|terminalfind|terminalfindnone|terminalfindregex|jump|jumptrust|jumpchanged|jumpfailed|jumpauth|jumpconnecting|jumppassword|jumppasswordwrong|jumppasswordsaved|jumppasswordaddress|jumprdp|password|passwordwrong|passwordsaved|passwordrdp|passphrase|passphrasewrong|passphrasesaved|passphraselocked|passphrasenotsaved|passphrasejump|questions|questionspassword|questionsmany|questionsjump|questionsrefused`, plus the `tree…` and `knownhosts…` ones listed in `docs/design/components.md`. Headless Chrome screenshots need `--disable-webgl`, otherwise xterm text doesn't appear. Add a scene for every new state.
- **Performance claims** need numbers from `bun tauri dev` (ideally `--release`) on WebKitGTK. Browser preview numbers don't count.
- **Slow Debian mirrors** when building the RDP fixture: `DEBIAN_MIRROR=http://kartolo.sby.datautama.net.id/debian docker compose build rdp`.
