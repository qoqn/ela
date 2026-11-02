# Components

The design system lives in `app/src/lib/ui/`. Feature code composes these primitives and does not re-style them. If a feature needs a variant, add it to the primitive and document it here.

```
app/src/lib/
├─ ui/                  design system: tokens + primitives, no app knowledge
│  ├─ tokens.css
│  ├─ Button.svelte
│  ├─ IconButton.svelte
│  ├─ TextArea.svelte
│  ├─ TextField.svelte
│  ├─ Checkbox.svelte
│  ├─ Select.svelte
│  ├─ Led.svelte
│  ├─ LabelTape.svelte
│  ├─ EnvRail.svelte
│  ├─ EnvBadge.svelte
│  ├─ EnvPicker.svelte
│  ├─ Kbd.svelte
│  ├─ SectionHeader.svelte
│  ├─ Tree.svelte
│  ├─ Dialog.svelte
│  ├─ EmptyState.svelte
│  ├─ Menu.svelte
│  ├─ Panel.svelte
│  ├─ Separator.svelte
│  ├─ Toast.svelte
│  ├─ types.ts         Env, LinkState, names and colours
│  └─ index.ts
├─ shell/               app frame: ActivityRail, TabStrip, StatusBar
├─ features/            sessions, sidebar, welcome, tree, vault, files, knownhosts, (later) tunnels, settings
└─ ipc.ts, sessions.svelte.ts   state and backend bridge
```

## Reviewing UI

Run `bun run dev` and open `http://localhost:1420/?scene=busy` in a browser. The preview backend in `app/src/dev/` fakes Sessions, so every state can be reviewed without servers:

| Scene | Shows |
|---|---|
| (none), `treeroot` | Welcome, with nothing saved yet and with Connections |
| `busy` | Five Tabs across all environments, production terminal active |
| `desktop` | A framebuffer Session |
| `failure` | A failed Session with its error banner |
| `tree` | The Connection tree with Folders and every environment |
| `treefilter`, `treenomatch` | Searching with matches, and with none |
| `settings`, `settingsvault`, `settingsnotsaved`, `settingsmissingfont` | The Settings page over a Session, jumped to a section, a change it couldn't keep, and a saved font that is gone |
| `treesearchfocus`, `treesearchtag`, `treesearchaddress` | The field with its Tag chips, a chip chosen, and an address with its connect form |
| `treerename` | Renaming a row |
| `treeedit`, `treeeditrdp`, `treenew`, `treefolder`, `treefoldersaved` | The editors, with options from `settings_schema`, the Folder form inheriting an option, and with a saved password |
| `treeeditsshconfig` | The Connection editor for a host `~/.ssh/config` describes: "Connects to web-1.prod.example.net:2222, from ~/.ssh/config." under Host, the file's port as the placeholder, and the Advanced tab |
| `treeeditsignin`, `treeeditrdpoptions`, `treefoldersignin` | The editors' other tabs: sign-in with identity files, an RDP Connection's own options, and a Folder's sign-in |
| `treenewinvalid` | Saving an empty new Connection from "Sign-in": back on "General", with the name and host marked |
| `treemove`, `treedrop` | Choosing where to move, and confirming a drop that changes inherited values |
| `treesessions` | LEDs at the right: a Connection open in two Tabs, a remote desktop, and one that needs the user |
| `treeconnect`, `treeopen`, `treefailed` | Connecting from the tree, an SSH Connection without a password connecting straight away with its LED, and "Edit connection" on failure |
| (none), `importask`, `importnotes` | "Import SSH hosts" on the empty tree, the dialog it opens first, and its report of what was left out |
| `treeroot` | Nothing selected, with the menu for the top of the tree |
| `devtools` | The Development tools dialog (development builds only) |
| `reconnect` | Reconnecting a Session that was opened with a typed password |
| `treetrash` | The undo toast |
| `treetrashview`, `treetrashnone`, `treetrashproblem` | The Trash: a Folder with contents and a Connection, nothing in it, and a file that can't be read |
| `treetrashconfirm`, `treetrashunlock` | Confirming "Empty Trash", and the Vault asked to unlock for it |
| `files`, `filesbig`, `filesempty` | Files for an SSH Session: a home folder, 400 rows, and an empty folder |
| `filesnone`, `filesrdp`, `filesended` | Files with no Session, with one that can't browse files, and with one that has ended |
| `filesdenied`, `filestransfer` | A folder that refused, and a download under way |
| `filesfolder`, `filesdrop` | Folder transfers (running, with something left out, and partly failed), and a drag from the desktop over the panel |
| `filesmenu`, `filesnewfolder`, `filesrename`, `filesnametaken` | The row menu, naming a new folder, renaming, and a name already taken |
| `filesdelete`, `filesdeletefolder`, `filesdeletelink`, `filesdeletedenied` | Confirming delete of a file, a folder and a link, and a delete that was refused |
| `terminalmenu`, `terminalmenucopy` | The terminal menu with nothing selected, and with a word selected |
| `treeproblem` | A saved file that can't be read |
| `treepalette` | Connections in the palette |
| `treerecent`, `treerecentnone`, `treerecentfolded` | Recent with an open Connection on top (every `tree…` scene has Recent), with nothing opened yet, and folded to its count |
| `treerecentmenu`, `treerecentpalette` | The Recent row menu, and Recent first in the connect palette |
| `vault`, `vaultlocked`, `vaultnew` | The Vault panel: unlocked, locked, and with no Vault yet |
| `vaultkeys`, `vaultkeysforget`, `vaultkeysforgotten` | Key passphrases in the Vault panel (one without a public key), confirming forgetting one, and afterwards |
| `vaultchange`, `vaultchangewrong`, `vaultchanged` | Changing the master password: the dialog, the wrong current password, and the panel afterwards |
| `knownhosts`, `knownhostsbig` | Known Hosts: saved keys and certificates with a named and an unnamed hashed name and lines left alone, and 400 rows |
| `knownhostsclosed` | SSH keys folded away to their heading and count |
| `knownhostshashed`, `knownhostsremovehashed` | A hashed name known from a saved Connection, filtered by that name, and confirming its removal |
| `knownhostsfilter`, `knownhostsnomatch` | Filtering, with matches and with none |
| `knownhostsempty`, `knownhostsproblem` | Nothing saved yet, and a file that can't be read |
| `knownhostsremove`, `knownhostscert` | Confirming the removal of a key and of a certificate |
| `knownhostsdone`, `knownhostsgone` | After removing, and an entry that was already gone |
| `treevaultunlock`, `treevaultwrong` | Connecting with a saved password while the Vault is locked: the Tab asks to unlock it, and a wrong master password asks again |
| `treevaultcreate` | The first saved password: creating the Vault above the editor |
| `treevaultsaved`, `treevaultinherited` | The editor with a saved password, and with one from a Folder |
| `treevaultmissing` | The RDP password panel on the Tab when the Vault no longer holds the password |
| `passphrase`, `passphrasewrong`, `passphrasesaved` | A key asking for its passphrase, after a wrong one, and when the saved one no longer unlocks it |
| `passphraselocked`, `passphrasenotsaved` | "Unlock the Vault" while it is locked, and a Vault that holds no passphrase for the key |
| `passphrasejump` | A Jump Host's key asking for its passphrase |
| `questions`, `questionspassword`, `questionsmany`, `questionsjump`, `questionsrefused` | Keyboard-interactive questions: a one-time code, the password when none was given, two questions with the server's text, a Jump Host asking, and a round after a refused one |
| `password`, `passwordwrong`, `passwordsaved` | A saved SSH Connection that no key signs in to asking for its password on the Tab with "Remember in the Vault", after a wrong one, and when the saved one no longer signs in |
| `jumppassword`, `jumppasswordwrong`, `jumppasswordsaved`, `jumppasswordaddress` | A saved Jump Host asking for its password with "Remember in the Vault", after a wrong one, when the saved one no longer signs in, and a hop written as `user@host` that offers nothing to remember |
| `jumprdp` | An RDP desktop behind a Jump Host, asking to trust its certificate after the Jump Host |

Add a scene whenever a new state or screen is built.

## Focus

The global ring (2px `signal`, 2px outside) suits buttons and fields. Two cases differ:

- A row that fills its container (tabs, list options, the Trash row, anything with `data-row`) draws the ring 2px inside, where it can't be clipped or touch a neighbour.
- A list that keeps focus itself (`Tree`, the Files list) uses the `list-focus` utility: no ring around the list, a ring inside the row marked `data-current` instead.

`ui/` must not import from `shell/`, `features/` or the IPC layer. It knows nothing about Sessions, only about visuals and behaviour.

## Primitives

| Component | Purpose | Variants and rules |
|---|---|---|
| `Button` | Text actions | `primary` (signal fill) for the action a dialog, form or empty state exists for, one per view at most; never for something destructive, `secondary` (raised), `ghost` (text only), `danger` (for destructive confirmation only). Text on a fill is `on-accent`. Sizes `sm` (24px) and `md` (28px). Labels are verbs. |
| `IconButton` | Icon-only actions | Requires `label` (used as tooltip and `aria-label`). `md` is 28×28, `sm` is 24×24 for bars shorter than 32px, such as the session rail. Pressed (`active`) is a `selected` fill with the icon in `label`. Disabled fades to 35%. |
| `Panel` | Everything the activity rail opens | Title in `text`, `text-sm` semibold, in a 36px header with its `IconButton` actions on the right. Fills the width the sidebar gives it. Content is padded 12px; rows and `Separator`s run edge to edge. |
| `ResizeHandle` | Drag to change a width | A 4px `signal` line on hover, focus and drag, across the border it sits on. A `separator` for screen readers. Arrows, `Home` and `End` on the keyboard, double-click for the default. |
| `Separator` | Divides parts of a `Panel` | A `line` rule from edge to edge. Never inset, never inside padding. |
| `TextArea` | Several lines, as for a `list` option with one entry per line | Same border, focus and error text as `TextField`, `mono` variant, resizes vertically. |
| `TextField` | Single-line input | `mono` variant for addresses, paths and fingerprints. Optional leading icon. Error text sits below, never as placeholder. |
| `Checkbox` | Turn one thing on or off | Native checkbox in `signal-strong`, with its label to the right in `text-sm`. The whole label is the hit area. |
| `Tabs` | Parts of one form or view | Underlined: `text-sm`, the chosen one in `text` over a 2px `signal` line, the rest `dim`. Arrow keys, `Home` and `End` move and select. Every panel stays mounted, inert while hidden. The first sets the height, with a floor of 320px, and the others scroll inside it with an 8px scrollbar in the dialog's padding, so nothing below moves when switching and no field changes width. A tab can be marked `invalid`, a `danger-text` dot after its label. |
| `Chip` | A small value in a row | `xs`, 20px tall, `line` border. A toggle (`pressed`, as for a Tag in the search) or with a remove button (`onremove`, as in the Tags field). |
| `SettingRow` | One setting on the Settings page | Label and `dim` description on the left, the control on the right, an error below, a `line` rule above. |
| `Stepper` | A small number | Minus and plus `IconButton`s around the value in tabular figures, each disabled at its limit. |
| `Select` | Choose one of a few | Native `<select>` styled with tokens until a custom listbox is needed. |
| `ConnectionIcon` | What a Connection opens | From `RendererKind`: `terminal` a terminal, `framebuffer` a monitor, anything else or an unknown Protocol a server. Never by Protocol ID. |
| `LinkBadge` | Link state of a saved Connection | A `Led` at the right of its row, with the number of Tabs beside it when there are several. |
| `Led` | Link state | `state`: connecting, connected, reconnecting, closed, failed. `activity` prop triggers one flicker. Always paired with text somewhere visible. |
| `LabelTape` | Host identity | `name`, optional `user`. Never truncated below 12 characters; the full address goes in the tooltip. |
| `EnvRail` | Environment mark | `env` and `orientation` (`vertical` for tree rows, `top` for Tabs). Production renders hatched. The full-width session rail is built from the same tokens in `features/sessions/SessionRail.svelte`. |
| `EnvBadge` | Environment name with swatch | Used wherever an environment is named, such as the session rail. Text stays in `text`, never in the environment colour. |
| `EnvPicker` | Choose an environment | Radio group of swatches with the chosen name spelled out. Arrow keys move the selection. With `inherited`, a dashed first choice takes the Folder's value. |
| `Kbd` | Shortcut hint | Renders key combinations, platform-aware (`Ctrl` vs `⌘`). |
| `SectionHeader` | Titles of parts inside a `Panel` or dialog | Sentence case, `text-sm`, semibold, `dim` colour, so it reads below the `Panel` title. Optional trailing `IconButton`. No caps, no letter-spacing. |
| `Tree` | Saved items in a hierarchy | Virtualised (`virtua`), ARIA tree with focus on the tree and `aria-activedescendant` on the row, drag and drop through pragmatic-drag-and-drop. A click or right-click on empty space clears the active row. The active row is `selected` with its name in `label`; hover is `raised`. |
| `EmptyState` | Nothing to show yet, nothing matches, or waiting | Title says what is the case (`text`, medium), optional detail says what to do (`dim`), then up to two `sm` Buttons stacked full width, the first `primary` when it is the way forward ("New connection", "Connect to …", "Reconnect"), then an optional `xs` hint. Centred, near the top, and it fills the space left, so what sits below it (the Trash row, a toolbar) stays at the bottom. Copy lives in [states-and-copy.md](./states-and-copy.md#empty-states). A section inside a list that has nothing (a Known Hosts store) uses a plain `dim` line instead. |
| `Dialog` | Modal | Hand-built. Keeps focus inside, closes on `Escape`, gives focus back. Initial focus goes to `[data-autofocus]`, the safest action. The title is in `label`. |
| `Menu` | Context menu | Hand-built, shows shortcuts with `Kbd`. Arrow keys, `Enter`, `Escape`. An item that can't be used yet is greyed out and skipped by the arrow keys, so the menu keeps its shape. The only context menu Ela shows outside text fields, see [interaction.md](./interaction.md#mouse). |
| `Toast` | Result of an action | Bottom-right, one at a time, with an optional "Undo". Stays 8 seconds. |

## Planned

| Component | Notes |
|---|---|
| `CommandPalette` | See [interaction.md](./interaction.md) |
| `Tooltip` | 500ms delay, instant when moving between items |
