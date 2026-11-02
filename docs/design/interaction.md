# Interaction

## Keyboard ownership

A Session with focus owns the keyboard. A terminal needs `Ctrl+W`, `Ctrl+R`, `Ctrl+Shift+anything`, and a remote desktop needs nearly every key. So Ela takes keys back in exactly two ways.

1. **App chord, `Ctrl+Shift` + key, for terminal Sessions.** Terminals rarely need `Ctrl+Shift` combinations, which is why Windows Terminal, GNOME Terminal and kitty use the same convention.
2. **Host key for framebuffer Sessions, default `Right Ctrl`, and any of `Right Alt`, `Right Shift`, `Menu`, `Scroll Lock` or `Pause` for keyboards without it.** While a remote desktop has focus, every key goes to the remote side, `Ctrl+Shift` included. Tapping the host key once releases keyboard grab and moves focus to the [session rail's actions](#session-rail-actions). Holding the host key and pressing a shortcut key runs that app shortcut (VirtualBox users know this model).

The host key is shown on the session rail while released and in the status bar while a framebuffer Session is showing, so it is never a secret. Clicking it in the status bar opens the shortcut sheet, where the key is chosen, so a keyboard without the current host key is never locked out. The choice is kept in `settings.toml` with the rest of the configuration. A choice older Ela versions kept in the webview moves there on the first start, and a key `settings.toml` names that this version doesn't offer counts as no choice and stays in the file. If it can't be saved, the new key still works until Ela closes and the sheet says why it wasn't saved.

## Shortcuts

| Action | Terminal Session | Framebuffer Session | Elsewhere |
|---|---|---|---|
| Command palette | `Ctrl+Shift+P` | `Host+P` | `Ctrl+Shift+P` or `Ctrl+K` |
| Search or connect | `Ctrl+Shift+O` | `Host+O` | `Ctrl+L` |
| New Tab from Connection search | `Ctrl+Shift+T` | `Host+T` | `Ctrl+T` |
| Close tab | `Ctrl+Shift+W` | `Host+W` | `Ctrl+W` |
| Next / previous Tab | `Ctrl+PageDown` / `Ctrl+PageUp` | `Host+→` / `Host+←` | same as terminal |
| Go to tab 1–9 | `Alt+1…9` | `Host+1…9` | `Alt+1…9` |
| Split right / down | `Ctrl+Shift+D` / `Ctrl+Shift+E` | n/a | n/a |
| Toggle sidebar | `Ctrl+Shift+B` | `Host+B` | `Ctrl+B` |
| Settings | `Ctrl+Shift+,` | `Host+,` | `Ctrl+,` |
| Larger, smaller and default terminal text | `Ctrl+Shift+=`, `Ctrl+Shift+-`, `Ctrl+Shift+0` | — | `Ctrl+=`, `Ctrl+-`, `Ctrl+0` |
| Toggle Broadcast | `Ctrl+Shift+I` | n/a | n/a |
| Copy / paste | `Ctrl+Shift+C` / `Ctrl+Shift+V` | clipboard sync, text only | `Ctrl+C` / `Ctrl+V` |
| Find in the terminal | `Ctrl+Shift+F` | n/a | n/a |
| Fullscreen | `F11` | `Host+F` | `F11` |
| Shortcut sheet | `Ctrl+Shift+/` | `Host+/` | `Ctrl+/` |

Other keys pressed while holding the host key go nowhere, and the host key itself never reaches the remote side.

All shortcuts can be rebound in Settings. Conflicts are shown inline before saving.

## Command palette

- One input, fuzzy-matched across Connections, open Tabs, commands, snippets and settings. Today that means open Tabs, Connections and commands; the rest join when they exist. "New Tab" opens the same palette without commands.
- Results are grouped by kind in a fixed order (open Tabs, Recent, Connections, commands), and each result shows its environment rail. Recent shows only before anything is typed: the Connections last opened, newest first with when on the right, and the Connections group leaves them out. Once something is typed, every Connection is matched alike.
- `Enter` runs the result and `Alt+Enter` runs it in a split.
- Typing an address (`user@host`, `rdp://…`) puts "Connect to …" first. It connects through the search field, so an address Ela can't read shows its error there, and the Tab asks for a password if the server wants one.

## Focus

- One focus ring style everywhere: 2px `signal` outline with a 2px offset, shown on keyboard focus only (`:focus-visible`). Rows draw it inside, and lists mark their current row rather than their whole box (see [components.md](./components.md#focus)).
- Moving focus into a Session removes the ring from chrome and shows the Session as active through its full-strength rail.
- `Escape` never closes a Session or a Tab. In chrome it closes the topmost popover or dialog. In a Session it belongs to the remote side.
- Focus returns to the Session after closing a palette, dialog or menu that was opened from it.

## Search or connect

- One field at the top of the Connections panel does both. Typing filters the Connection tree as it goes and selects the first Connection left, typing while the tree has focus lands here too, and `↑`/`↓` move the selection without leaving the field. `Enter` opens the selected row, so what opens is always what is highlighted. `Escape` empties the field, then goes to the tree.
- Search looks at names, hosts, usernames and Tags, and every word has to match. A Folder that matches keeps all it contains, and Folders leading to a match stay open. `#eu` keeps Connections with a Tag starting that way.
- Text with `@` or `:` is an address, whatever it matches, so it can't open a saved Connection by accident. So is one word that matches nothing. Text starting with `#` or holding a space never is. An address opens username, Environment and "Connect" below the field, and `Enter` connects to it.
- While the field has focus or holds text, the Tags in use show below it as chips, and a click adds or removes that Tag from the search.
- It has no password field. Every Protocol asks on the Tab when it needs a password, after the server's identity was checked, so a password is never typed for a server that turns out to be another.

## Settings

- The gear at the bottom of the activity rail, `Ctrl+,` or the palette opens Settings: one page that covers the Tab strip and the Session, like Zed's or VS Code's, and scrolls from the first setting to the last. It is not a Tab, since a Tab shows a Session.
- A list on its left names the sections. A click jumps to that section and moves focus to its heading, and the list follows the scrolling. Below 640px the list hides and the page still scrolls.
- There is no Save: a change applies at once and is kept in `settings.toml`. If it can't be kept, the row says so and the choice lasts until Ela closes.
- Each setting has one home here, and a section lists only what works today. The shortcut sheet keeps the host key too, because the status bar sends a keyboard without that key there.
- Sessions keep running underneath, and the page holds the keyboard: nothing typed here reaches a Session. `Escape`, the close button, the gear, `Ctrl+W`, another rail section or anything that shows a Tab closes the page, and the Session gets the keyboard back.
- **Terminal**: font, from the system's monospaced fonts with the built-in IBM Plex Mono first, and size from 8 to 32, with a preview. Every terminal changes at once. A saved font that is gone says "(not installed)" and the built-in font is used meanwhile.
- **Keyboard**: the host key, and a way to the shortcut sheet.
- **Vault**: how long it stays open without use, and a way to the Vault panel.

## Reconnect

- An ended Session offers "Reconnect" in its bottom banner, in place of "Disconnect" on the session rail, and in Files. It opens the same Connection in the same Tab.
- A saved Connection goes through connecting again, Vault and password prompt included. An address reconnects straight away and asks on the Tab again if it needs a password, since a typed password went to Rust once and is gone (ADR-0002).

## Recent

- Below the Connection tree, above the Trash row, "Recent" lists the last 8 saved Connections opened on this machine, newest first. Opening one again moves it to the top. Addresses typed in the search field or the palette aren't saved, so they aren't listed.
- Each row looks like a tree row (icon, name, host, LED while open, environment rail) and adds when it was opened on the right ("4 min ago", "Yesterday", "17 Oct"), which moves on while it shows. The tooltip has the Folder path, the address and the exact time.
- A click or `Enter` connects. The list is one stop for `Tab`; the arrow keys, `Home` and `End` move within it. `Delete` removes the row from Recent only, and `Alt+Enter` edits the Connection. `Shift+F10`, the menu key or a right-click opens "Connect", "Show in tree", "Edit", "Remove from Recent" and "Clear Recent". "Show in tree" empties the search, opens the Folders on the way and selects the row in the tree.
- The heading folds the list to "Recent" and a count, and the choice is remembered.
- A Connection moved to the Trash leaves Recent, and Undo or Restore brings it back to its place. Emptying the Trash drops it for good.
- The list is kept in `~/.local/share/ela/`, not with the synced configuration, since it is this machine's history and changes on every connect.

## Connection tree

- A Connection's icon says what kind of screen it opens, from its Protocol's `RendererKind`: a terminal or a monitor. Icons stay `dim`, since colour belongs to the Environment.
- In each Folder, Folders come first, then Connections, each by name with numbers counted as numbers (`web-2` before `web-10`). The same order holds in Move, the Trash and Known Hosts. There is no manual order: each item is its own synced file, so one drag would rewrite many and invite conflicts, and a drag already means "move to this Folder". Recent is the place for what is used most.
- Arrow keys move, `→`/`←` expand and collapse, `Enter` connects, `F2` renames, `Delete` moves to Trash (with undo toast).
- `Alt+Enter` edits, `Ctrl+M` moves to a chosen Folder, `Ctrl+N` and `Ctrl+Shift+N` create a Connection and a Folder in the current Folder. `Shift+F10`, the menu key or a right-click opens the same actions as a menu.
- Clicking empty space below the rows, or `Escape` with no search, selects nothing, so new items go to the top of the tree. A right-click there offers "New connection" and "New folder" for the top. The header buttons name the Folder they add to ("New connection in Databases").
- Drag and drop moves items. A drop on a Folder shows which inherited fields change, before it commits, if any field changes. A drop outside any row moves to the top of the tree. `Ctrl+M` shows the same changes for the Folder selected.
- Moving a Folder into itself or its contents isn't offered.
- Connecting signs in with the password saved for the Connection, or inherited from a Folder. A locked Vault asks on the Tab, with "Unlock" and "Connect without it", and the connection carries on by itself either way.
- With nothing saved, a Protocol that can sign in without a password connects straight away and asks on the Tab only if it needs one. SSH tries the identity files named on the Connection, then the saved password, then ssh-agent and the default keys in `~/.ssh` (`id_rsa`, `id_ecdsa`, `id_ed25519`), so a key user is never asked. RDP asks on the Tab once the server's certificate was checked. When the Vault no longer holds the saved password, both sign in as if none were saved and the password panel says so.
- "Import SSH hosts" is on the empty tree, on the first-run Welcome and in the command palette. It first opens a dialog that says what it reads and changes, with "Import" focused, and only then reads `~/.ssh/config`. Each `Host` without a wildcard becomes a Connection named after it, holding only that name: the file is read again at every connect, so a change to it counts at once. Importing again only adds new names. Files named by `Include` count, `Match` is honoured, and `Match exec` is never run: its block is left out and reported.
- An SSH or RDP Connection with Jump Hosts (`proxy_jump`, one `user@host:port` or saved Connection name per line, read like `ProxyJump`) goes through them in order. The Jump Hosts are always SSH. A Jump Host named like a saved SSH Connection signs in with that Connection's address, user, identity files and saved password, and its own Jump Hosts come first when it is the first one. Any other Jump Host is taken as written and tries the Connection's identity files and ssh-agent, never its password. A saved password also stays behind when the route names another user or port for that Jump Host.
- A locked Vault holding a Jump Host's password asks on the Tab, like the Connection's own. One unlock, or one "Connect without it", serves the whole route.
- Every host on the route is checked against Known Hosts under its own name, and asks on its own when it is new or changed.
- A Connection with a ProxyCommand doesn't connect until Ela can take that route. It never goes direct instead, since a password could reach the wrong machine.
- Deleting a Folder takes its contents with it, and Undo brings all of them back.
- The "Trash" row below the tree, or "Open Trash" in the command palette, lists what was deleted. A Folder is one row with its contents, including anything deleted from it earlier, and "Restore" brings all of it back to where it was. Opening the Trash ends any Undo still on offer.
- "Empty Trash" asks first, with "Cancel" focused. It also removes the saved passwords that nothing else uses, so a locked Vault asks to be unlocked first, and leaving it locked removes nothing. A password stays if a file that can't be read names it. Files in the Trash that can't be read stay there, and the Trash says so.
- A Folder's form sets what its contents inherit: username, port, Environment, a saved password, and the options of every Protocol, since it can hold Connections of any of them.
- Both forms use `Tabs`: "General" (name, address, Jump Host, Environment, Tags), "Sign-in" (username, password, and the options marked `"group": "sign_in"` such as identity files or the RDP domain), and one "SSH options", "RDP options"… tab per Protocol with options of its own. A Connection shows only its Protocol's tab, a Folder one per Protocol. An option two Protocols share, like the Jump Host, appears once. The form opens on "General", whose height is the form's, and a longer tab scrolls inside it. An empty name or host, or a port that isn't one, says so under its field, and saving from another tab brings "General" forward with a dot on it. An error from saving sits beside the buttons, where every tab can see it.
- Tags are chips in the form: `Enter` or a comma adds what is typed, `Backspace` in the empty field removes the last, and the line below offers the Tags other Connections use. A Tag has no spaces, since a search term ends at one, and one saved with a space is searched with a dash.
- Option fields come from `settings_schema`: `label`, an optional `hint` under the field, `labels` naming each `enum` value ("Automatic" rather than `auto`), `mono` for paths and addresses, and `group` for the tab, which is the Protocol's own when absent. Fields keep the order the schema lists them in.
- A saved file that can't be read stays untouched on disk, and the tree says so below its rows.
- Double-click connects, single-click selects. This matches every file manager our users know.

## Files

- Files shows the active Tab's Session, over that Session's own connection, so it never signs in again. Each Tab remembers its folder.
- It offers itself when the Protocol has the `file_transfer` capability, never by Protocol name.
- `↑`/`↓`/`Home`/`End` move, `Enter` opens a folder or downloads a file, `Backspace` goes up a folder. Double-click does what `Enter` does. Typing a path and pressing `Enter` goes there.
- Hidden files are off until asked for, per Tab. A link shows what it points to, so a link to a folder opens like a folder, and one that leads nowhere can't be opened.
- Upload and Download ask for the local place in the system's own dialog. The webview never names a local path.
- Files and folders dragged from the desktop onto the Files panel upload to the folder it shows, and the panel says where while they are over it. Dropping anywhere else does nothing. Rust holds the dropped paths and the webview only gets a token for that one drop.
- Folders transfer whole, with Download or "Upload a folder". A folder of the same name asks once, then merges: files with the same name are replaced and the others stay. Cancelling keeps the files already finished, none of them half written.
- Links and special files inside a folder are left out, in both directions, and the row says how many. A link could loop, or lead a transfer outside the folder. A link chosen by itself is followed.
- Permissions travel: an upload keeps the local file's or folder's mode, so private stays private on the server, and replacing a file still keeps the old file's mode.
- `F2` or the row's menu renames, `Delete` or the menu deletes, and "New folder" is in the panel's toolbar. A name already taken is refused, never overwritten.
- Delete asks first, with focus on "Cancel". A folder goes with everything in it, but what a link points to stays. A folder with a part that can't be read isn't touched at all. There is no Trash on the server.
- A transfer never damages what is there. It goes to a temporary file first, so cancelling or failing leaves the destination as it was. Replacing a remote file asks first, one file at a time, and keeps its permissions. A download gets the remote file's read and write permissions but is never executable.
- Cancelling stops the rest of a batch too. A file that fails doesn't stop the others.

## Known Hosts

- Known Hosts lists the SSH keys in `~/.ssh/known_hosts` and the certificates in Ela's own `known_hosts`, one row per name and identity, sorted by name with unnamed hashed names last. A filter matches names and fingerprints.
- A hashed name (`HashKnownHosts yes`) can't be read back, only checked against a name. Ela checks it against the host and port of every saved SSH Connection and of the Jump Hosts their routes name. A match shows, sorts and filters as that name, written as OpenSSH would (`[db-02.internal]:2222` for another port), with a dim "hashed" after it. Without a match the row says "Hashed name" and only its fingerprint can be searched.
- Removing asks first, naming the server and the full fingerprint, with focus on "Cancel". A hashed name also shows the hashed text it is saved as, which is what gets removed. It takes out that name only: other names on the same line stay, and so do the server's keys of other types.
- The previous file stays next to it as `known_hosts.old`, exactly as when replacing a key. A symlink and the file's mode survive.
- Lines with `@revoked` or `@cert-authority`, negated names and lines that can't be parsed are never listed or changed. A note counts them, because dropping an `@revoked` line would trust that key again.
- The webview names the store and the entry, never a path.

## Session rail actions

- The session rail is the Session's toolbar, so its controls sit under its Tab and over its screen, and there is no toolbar on top of the remote screen. Which actions show follows the renderer and the Protocol's capabilities, never its name.
- A terminal has Find. A remote desktop has display size (dynamic, fit or 1:1, in a menu), Send Ctrl+Alt+Del, clipboard sync when the Protocol can sync, and Save a screenshot. Dynamic follows the view by resizing the remote desktop, fit scales it, and 1:1 shows every pixel and scrolls. Screenshots go to the Pictures folder, under `Ela`, and a toast says where.
- Disconnect is always last, a little apart, and becomes Reconnect once the Session has ended. The others show only while connected.
- Below 480px of rail width the actions fold into a "Session actions" menu.
- Pressing an action with the mouse leaves focus where it is, so the keyboard stays with the Session.
- Tapping the host key releases the keyboard: focus moves to the first action and the rail says "Keyboard released" with the key. `Escape`, tapping the host key again or clicking the remote screen gives the keyboard back.

## Find in a terminal

- `Ctrl+Shift+F` or "Find" in the terminal menu opens a find bar at the top right of the Tab, with focus in its field. A selection on one line becomes the search, and otherwise the last search of that Tab comes back, selected so typing replaces it.
- It searches the screen and the whole scrollback as you type, highlights every match and selects the nearest one from where the selection was, scrolling to it. New output is searched too.
- `Enter` or the down-arrow button goes to the next match, `Shift+Enter` or the up-arrow button to the previous one, both wrapping around. "Match case" and "Use regular expression" are toggles beside the field.
- The count reads "3 of 12". Past 1,000 matches it stops counting and says "1,000+".
- `Escape` or the close button hides the bar, clears the highlights and gives the keyboard back to the shell. The selected match stays selected, so `Ctrl+Shift+C` copies it.
- Keys typed in the field never reach the remote side.

## Mouse

- The webview's own context menu offers Reload and Back, which would end every Session, so Ela blocks it. Text fields and selected text keep it for copy and paste. Everywhere else a right-click opens Ela's `Menu` or does nothing.
- Right-click in a terminal opens Ela's `Menu`: "Copy", greyed out while nothing is selected, "Paste", "Find", and "Open Files here" when the Protocol has the `file_transfer` capability. "Here" is the folder the shell last reported (OSC 7). A shell that doesn't report it leaves Files on the folder the Tab remembers. Split joins the menu when it exists.
- Hold `Shift` to pass the right-click to the remote application, for a program that uses the mouse. Without such a program it does nothing.
- Middle-click on a Tab closes it.
- Scroll over the tab strip switches Tabs.

## Feedback for actions

- Every command that changes state answers within 100ms, either with the result or with a pending state on the control itself.
- Destructive or remote-affecting actions (delete, Broadcast, multi-line paste into production) use dialogs that name the exact objects affected.
- Undo is preferred over confirmation for local, reversible actions such as deleting Connections or closing Tabs. Ela shows a toast with "Undo" for 8 seconds.
