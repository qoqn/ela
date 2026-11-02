# States and copy

## Voice

Ela speaks like a calm, experienced colleague at the next desk: short, exact, never cute.

- **Sentence case** for everything: buttons, titles, menu items, tabs.
- **Plain verbs.** "Connect", "Disconnect", "Close tab", "Trust this key", not "Proceed", "Submit" or "OK".
- **Same name through the flow.** The button "Disconnect" produces the status "Disconnected". The menu item "Move to Trash" produces the toast "Moved to Trash".
- **Users' words, not ours.** "Password or key was rejected" instead of "AuthFailed". "The server's identity changed" instead of "HostKeyChanged".
- **No apologies and no blame.** Ela doesn't say "Oops", "Sorry" or "You entered an invalid…".
- **No vague errors.** Every error says what happened, and what to do next when there is something to do.
- **Errors are sentences.** Capital letter, full stop, the thing named: "That connection no longer exists.", not "connection not found". A cause from the system follows a colon in its own words: "Can't use /home/you/.config/ela: Permission denied". This holds for messages made in Rust too, since the UI shows them as written.
- **Things are lower case, Ela's places are not.** In copy a connection, folder, session, tab, password and protocol are ordinary words ("New connection", "Close tab", "Open sessions stay open"). The places Ela names keep their capitals: the Vault, the Trash, Recent, Known Hosts, and a Jump Host ("Jump Host gate"). The docs capitalise every [glossary](../../GLOSSARY.md) term; the UI doesn't.
- **Names in curly quotes.** A name the user chose or typed, when it isn't already set apart, goes in “ ”: "Nothing matches “prod-db”.", "Image quality can't be “blurry”."
- **Keys in mono, words in text.** Shortcuts, addresses, paths and fingerprints are mono. A sentence never is, and something that doesn't exist is a dim "—", never "n/a".
- **Terms come from [GLOSSARY.md](../../GLOSSARY.md).** Connection (saved), Session (live), Tab (where it is shown).

## Session states

| State | LED | Session area | Status bar |
|---|---|---|---|
| Connecting | amber blink | The connecting card: see below. "Connect to db-01" is the current step | "Connecting" |
| Verifying | amber blink | The connecting card, "Checking its identity…" | "Verifying" |
| Unknown or changed host key | amber steady | Inline panel: see "First connection" below | "Waiting for you" |
| Key needs its passphrase | amber steady | Inline panel: see "Key passphrase" below | "Waiting for you" |
| Needs a password | amber steady | Inline panel: see "Password" below | "Waiting for you" |
| Server asks questions | amber steady | Inline panel: see "Sign-in questions" below | "Waiting for you" |
| Authenticating | amber blink | The connecting card, "Signing in as root…" | "Signing in" |
| Connected | green | Remote screen only | "Connected" plus latency |
| Reconnecting | amber fast blink | Last remote screen dimmed to 40%, with a banner at the bottom: "Connection lost. Reconnecting (attempt 2 of 5)…" and a "Stop" button | "Reconnecting" |
| Disconnected | off | Last screen dimmed. Bottom banner: "Disconnected" with "Reconnect" and "Close tab". Before it ever connected: the card, "Disconnected" and "Stopped before connecting to db-01." | "Disconnected" |
| Failed | red | Bottom banner with the error copy below, plus "Reconnect", "Edit connection" and "Close tab". Before it ever connected: the card, with the step that failed | "Failed" |

A server can hand a connected Session over to another (GNOME Remote Login moves it from the login screen to the user's own desktop). Ela follows without a state change: the last screen stays in place, still "Connected", until the new one paints over it. Signing in there uses what the first server handed over, and its certificate must be the one that server named, so neither asks the user anything. A handover Ela can't follow fails with "the server moved the Session to another host, which Ela can't follow yet".

The remote screen is never replaced by a full-page error. The last frame or scrollback stays visible because people need to read what happened right before the drop.

A Session that never connected has nothing on screen to keep, so its failure stays in the connecting card, in the same place the user was watching.

## Connecting card

A card in the middle of the empty Session area lists the steps, so a slow host shows where it is stuck instead of looking frozen. It covers the screen until the Session connects, including the terminal's cursor.

```
┌────────────────────────────────────────────┐
│  ✓  Jump Host gate                          │
│  ✓  Connect to db-01 through gate           │
│  ●  Checking its identity…                  │
│  ○  Sign in as postgres                     │
│                                             │
│  4 s                             [ Cancel ] │
└────────────────────────────────────────────┘
```

- Each host has three steps: "Connect to db-01", "Check its identity", "Sign in as postgres". The current one reads "Connecting to db-01…", "Checking its identity…" or "Signing in as postgres…", with a blinking amber LED. Done steps have a check, steps to come an empty circle, both `dim`.
- A Jump Host gets the same three steps while Ela is on it, named "Jump Host gate", with "Sign in" and no user. The target waits below as one pending row, "db-01". Once a Jump Host is passed it folds into one done row, "Jump Host gate". After the last Jump Host, the target's first step reads "Connect to db-01 through gate".
- The seconds appear from 2 s on, so a quick connection doesn't flash a clock.
- "Cancel" ends the Session as "Disconnected".
- When the Session fails before it connects, the step it was on turns `danger-text` with a cross, and the error copy and its buttons follow under a line, in the same card. A failure's title, in the card and in the bottom banner, is `danger-text` after an alert icon; the card and banner keep their `line` borders, because a red edge on one side alone reads as decoration. The card is 52ch wide in every state, so nothing jumps.
- The trust panels, the passphrase panel, the password panel and the questions panel replace the card while they ask.

## Error copy

| Error kind | Title | Detail and next step |
|---|---|---|
| `host_unreachable` | Can't reach db-01 | "Nothing answered at 10.0.0.12:22. Check the address, VPN or firewall." |
| `timeout` | db-01 didn't respond in time | "The host may be down, or a firewall may be dropping packets." |
| `auth_failed` | Password or key was rejected | "The server refused root with every way Ela tried: password, identity files and ssh-agent." + "Edit connection" |
| `host_key_changed` | The server's identity changed | "db-01 presented a different key than last time. This can mean the server was reinstalled — or that someone is intercepting the connection. Ela did not connect." Shown after "Cancel" in the changed-key panel below. |
| `host_key_revoked` | The server's key was revoked | "db-01 presented a key that your known_hosts file marks as revoked, so it must not be trusted again. Ela did not connect. Ask the server's admin for its new key." Never asked about: an `@revoked` line always wins. |
| `certificate_untrusted` | Certificate not trusted | The new and saved fingerprints, shown after "Cancel" in the changed-certificate panel. |
| `protocol` | Connection failed | Technical detail shown as-is in mono, with "Copy details". |
| `jump_host` | The copy of the error underneath, about "Jump Host gate" and its address, so "Can't reach Jump Host gate" or "Jump Host gate didn't respond in time". A changed key offers `ssh-keygen -R` for the Jump Host. A rejected sign-in reads "Jump Host gate rejected the password or key". | As for that error. |
| `forward_failed` | Can't reach db-01 through gate | "gate couldn't open a connection to 10.0.0.7:22. Check the address, and that gate can reach it." When the Jump Host forbids forwarding: "gate doesn't allow forwarding, so it can't pass the connection on to 10.0.0.7:22. Its sshd_config needs AllowTcpForwarding." |

### Through Jump Hosts

Every step names what it is about. The connecting card lists each Jump Host's steps in turn, "Connecting to Jump Host gate…" first. The panels below say "First connection to Jump Host gate" and "Jump Host gate presented a different key…". Each host on the route asks on its own, in order, and the target last. The status bar's fingerprint is always the target's.

A route that can't be planned doesn't open a Tab, and a toast "Couldn't connect" says why:

| Problem | Copy |
|---|---|
| Can't read it | "“gate:x” isn't a Jump Host Ela can read. Write it as user@host:port." |
| Two Connections share the name | "Several SSH connections are named “gate”, so Ela can't tell which one is the Jump Host." |
| A loop | "The Jump Hosts lead back to “gate”, so the route never ends." |
| The first Jump Host uses a ProxyCommand | "The Jump Host “gate” goes through a ProxyCommand, which Ela can't do yet. It did not connect directly instead." |

A Jump Host's saved password that the Vault no longer holds is treated like one never saved: the Jump Host asks on the Tab if it needs a password, with the note "The Vault no longer holds the password saved for Jump Host gate."

## First connection (unknown host key)

```
┌──────────────────────────────────────────────────────────────┐
│  First connection to db-01                                    │
│                                                               │
│  Ela hasn't seen this server before. If you can, compare the  │
│  fingerprint with the one your admin or provider published.   │
│                                                               │
│  ED25519   SHA256:9fQ2…k1c2                                   │
│                                                               │
│                         [ Cancel ]  [ Trust and connect ]     │
└──────────────────────────────────────────────────────────────┘
```

The same panel covers a certificate no trusted authority vouches for (RDP). The label before the fingerprint reads "Certificate".

A host whose known_hosts entries hold only other key types gets the same calm panel, titled "New key type for db-01": "Ela knows db-01 by its ECDSA key, and it now presents an ED25519 key. Servers add key types when they are upgraded. If you can, compare the fingerprint with the one your admin published." Ela asks for the saved types first, so this shows only when the server no longer offers them.

## Changed host key or certificate

Replacing a saved identity is always the user's explicit choice. The panel has a `danger` border all round, and its title is `danger-text` after a shield icon, so it differs from the first-connection panel in shape as well as colour. The button is `danger`. For certificates the copy says "certificate" where it says "key" here.

```
┌──────────────────────────────────────────────────────────────┐
│  The server's identity changed                                │
│                                                               │
│  db-01 presented a different key than the one saved for it.   │
│  This can mean the server was reinstalled, or that someone is │
│  intercepting the connection. Replace the saved key only if   │
│  you know why it changed.                                     │
│                                                               │
│  Saved   SHA256:Kd+y…REUU                                     │
│  Now     SHA256:9fQ2…k1c2                                     │
│                                                               │
│                   [ Cancel ]  [ Replace key and connect ]     │
└──────────────────────────────────────────────────────────────┘
```

In both panels, initial focus is on "Cancel". "Cancel" on a first connection ends the Session as "Disconnected". On a changed identity it ends as `host_key_changed` or `certificate_untrusted`. Replacing an SSH key removes this host's own name from entries holding its old key of that type, leaves pattern lines such as `*.corp` alone, and leaves the previous file as `known_hosts.old`. A certificate from a trusted authority never asks: for SSH, a host certificate signed by an `@cert-authority` key whose pattern covers the host.

## Key passphrase

An identity file locked with a passphrase asks for it once the server has accepted the key, the way OpenSSH does, so a key the server would refuse never interrupts. A key that ssh-agent already holds isn't asked for. A passphrase saved in the Vault is used without asking while the Vault is open.

```
┌──────────────────────────────────────────────────────────────┐
│  Passphrase for id_ed25519                                    │
│  This key is locked with a passphrase, which Ela needs to     │
│  sign in to db-01.                                            │
│                                                               │
│  Key   ~/.ssh/id_ed25519                                      │
│                                                               │
│  Passphrase                                                   │
│  [                                                    ]       │
│  [x] Remember in the Vault                                    │
│                                                               │
│  Skip this key  Unlock the Vault        [ Cancel ] [ Sign in ]│
└──────────────────────────────────────────────────────────────┘
```

- Focus starts in the field, and `Enter` signs in. A wrong answer brings the panel back with a note under the field and focus in it again.
- "Remember in the Vault" is ticked when the Vault is open, so remembering never brings up a dialog unasked. Ticked with the Vault locked or absent, "Sign in" opens or creates it first. Only a passphrase that unlocked the key is remembered.
- A passphrase is saved per key, not per Connection: every Connection and Jump Host naming the key finds it. It goes under the key's SHA256 fingerprint, or under the file's path when the key's format hides its public part and no `.pub` lies beside it.
- "Unlock the Vault" shows only while the Vault is locked, because only then can it hold a passphrase Ela couldn't use. Once it opens, a saved passphrase signs in, and otherwise the panel says so and stays.
- "Skip this key" goes on to the next way to sign in. With nothing left, the Session fails as `auth_failed`. "Cancel" ends the Session as "Disconnected".
- Through a Jump Host the detail names it: "…which Ela needs to sign in to Jump Host gate."

| Moment | Copy |
|---|---|
| Wrong passphrase | "That passphrase doesn't unlock the key." |
| The saved one no longer works | "The passphrase saved in the Vault no longer unlocks this key." The new one replaces it if "Remember in the Vault" is ticked. |
| The Vault has none | "The Vault holds no passphrase for this key." |
| Why the Vault dialog appeared | "To remember the passphrase of ~/.ssh/id_ed25519.", "To unlock the key ~/.ssh/id_ed25519." |

## Inherited from ~/.ssh/config

In the Connection editor, a value that comes from `~/.ssh/config` is shown like one from a Folder, with the file as its source: "deploy, from ~/.ssh/config". When the file gives the host another name, a line under Host says "Connects to db.prod.example.net:2222, from ~/.ssh/config." A Connection or Folder value always wins over the file. Options most people never touch (the known hosts file, a host key alias and the algorithm lists) sit on an "Advanced" tab.

## Server banner

A banner the SSH server sends before sign-in (often a legal notice) is the first text in the terminal, as `ssh` prints it, so it scrolls up with the rest of the Session. Control characters are removed, so it can't move the cursor or change the title. A framebuffer Session has no terminal, so its Jump Hosts' banners aren't shown.

## Unlocking the Vault

A saved Connection or Jump Host whose password is in a locked Vault asks on the Tab before connecting, since the Secret is read first.

```
┌──────────────────────────────────────────────────────────────┐
│  Unlock the Vault                                             │
│  To use the password saved for Jump Host gate. Without it,    │
│  Ela asks for the password if Jump Host gate needs one.       │
│                                                               │
│  Master password                                              │
│  [                                                    ]       │
│                                                               │
│  Connect without it              [ Cancel ]  [ Unlock ]       │
└──────────────────────────────────────────────────────────────┘
```

- Focus starts in the field. A wrong master password brings the panel back with "That password doesn't open the Vault." under the field.
- "Connect without it" goes on as if nothing were saved, for the whole route. "Cancel" ends the Session as "Disconnected".
- The rest of the window learns that the Vault is open once the Tab moves on.

## Password

No Protocol asks for a password before connecting. An SSH Connection and a Jump Host ask on the Tab only after their keys, because most sign in with a key; RDP asks once the server's certificate was checked, when no password is saved. Each asks on the Tab only once its identity files, its saved password, ssh-agent and the default keys in `~/.ssh` have all failed, and only when the server offers password sign-in, or for a Jump Host keyboard-interactive. Through keyboard-interactive the typed password answers the server's password question, and anything else it asks comes up in the questions panel. A password typed for a Jump Host goes to that Jump Host alone, never to the next one or the target.

```
┌──────────────────────────────────────────────────────────────┐
│  Password for Jump Host gate                                  │
│  No key or ssh-agent signed in to Jump Host gate, which leads │
│  to db-01. Ela sends this password only to gate.              │
│                                                               │
│  Sign in as   ops@gate.example.com                            │
│                                                               │
│  Password                                                     │
│  [                                                    ]       │
│  [x] Remember in the Vault                                    │
│                                                               │
│                                        [ Cancel ] [ Sign in ]│
└──────────────────────────────────────────────────────────────┘
```

- For the Connection itself the title is "Password for db-01" and the detail "No key or ssh-agent signed in to db-01." The first sentence names only what was tried: "No key signed in to db-01.", "No key in ssh-agent signed in to db-01.", or with nothing tried "db-01 needs a password."
- A saved Connection whose Secret the Vault lost shows the note "The Vault no longer holds the password saved for db-01."
- Focus starts in the field, and `Enter` signs in. A wrong answer brings the panel back with a note under the field and focus in it again.
- It asks three times, like OpenSSH. After the third wrong answer the Session fails as `auth_failed`, through a Jump Host as "Jump Host gate rejected the password or key". "Cancel" ends the Session as "Disconnected".
- A server without NLA (xrdp, older Windows) can't refuse a password during sign-in. A wrong one connects, the server's own login screen shows the failure, and the panel doesn't come back.
- "Sign in as" leaves out port 22 and brackets an IPv6 address with another port.
- "Remember in the Vault" shows for a saved Connection, and for a Jump Host that is a saved Connection signed in to as its own user and port, since a password for another account must not land on it. A typed address offers nothing, and neither does a hop written as `user@host`, or as `root@gate` when gate's Connection signs in as ops.
- It is ticked when the Vault is open, and ticked with the Vault locked or absent, "Sign in" opens or creates it first, as for a key's passphrase. Only a password that signed in is remembered: it replaces the Secret the Connection names, or becomes the Connection's own saved password when it has none (even if a Folder lends one), so the next connect doesn't ask.

| Moment | Copy |
|---|---|
| Wrong password | "db-01 didn't accept that password.", "Jump Host gate didn't accept that password." |
| The Vault lost it | "The Vault no longer holds the password saved for db-01." |
| The saved one no longer works | "The password saved in the Vault no longer signs in to db-01." (or "to Jump Host gate"). The new one replaces it if "Remember in the Vault" is ticked. |
| Why the Vault dialog appeared | "To remember the password of db-01.", "To remember the password of Jump Host gate." |

## Sign-in questions

A server can sign in with keyboard-interactive instead of a plain password, usually PAM asking for a password and then a one-time code. Ela answers each question that asks for a password with the saved or typed one, once, and asks on the Tab for everything else. With no password at all, such as a typed address or a saved Connection without one, every question is asked on the Tab, the password too, so a server that wants only a code isn't sent a password.

```
┌──────────────────────────────────────────────────────────────┐
│  db-01 asks a question                                        │
│  The server wants this before it lets you in. Ela sends the   │
│  answers only to db-01.                                       │
│                                                               │
│  Sign in as        postgres@db-01.internal                    │
│  The server says   Use the code from your token app.          │
│                                                               │
│  Verification code                                            │
│  [                                                    ]       │
│                                                               │
│                                        [ Cancel ] [ Sign in ]│
└──────────────────────────────────────────────────────────────┘
```

- One field per question, labelled with the server's own words without the trailing colon (an empty one reads "Answer"). A question the server wants hidden is a password field, otherwise plain text.
- "The server says" shows the server's name for the exchange and its instructions, as plain text, and only when it sent any.
- Focus starts in the first field, and `Enter` signs in. Each round of questions is a new panel. An empty answer is allowed, since some questions expect one.
- A refused round starts over, three rounds in all like OpenSSH, so a mistyped code isn't the end. The saved or typed password answers each round, unless a round asked nothing else and was refused, which means the password itself was wrong: then the Session fails as `auth_failed`, and a Jump Host asks for its password instead. A round after a refused one says so above the fields.
- A password question is recognised by being hidden and containing "password". A server asking in another language gets the question on the Tab instead.
- Answers go to Rust once and are never kept or remembered. "Cancel" ends the Session as "Disconnected".
- Through a Jump Host the title names it: "Jump Host gate asks a question".

| Moment | Copy |
|---|---|
| Title | "db-01 asks a question", "db-01 asks 2 questions" |
| Detail | "The server wants this before it lets you in. Ela sends the answers only to db-01." |
| After a refused round | "The server didn't accept those answers and asks again." |

## Vault

The master password is typed in a dialog, in every case the same one, and whatever needed the Vault carries on once it opens. "Cancel" drops what was going on and changes nothing.

| Moment | Copy |
|---|---|
| Panel, no Vault | "No Vault yet. Create one to save passwords with your connections." + "Create Vault" |
| Panel, locked | "Locked. Connections with a saved password wait until you unlock it." + "Unlock", "Open sessions stay open while it is locked." |
| Panel, unlocked | "Unlocked. It locks after 15 min without use." + "Lock now", "Open sessions stay open when it locks." |
| Panel, a Vault exists | "Change master password". "Change" after the sentence naming the idle time opens "Lock after" in Settings, with 1 min, 5 min, 15 min, 30 min, 1 hour and 4 hours |
| Panel, unlocked, "Key passphrases" | One row per key with a saved passphrase: its path in mono, then its fingerprint shortened (`SHA256:9fQ2xR7mWc0L…i2k1c2`, whole in the tooltip) or "No public key beside it", and a "Forget the passphrase of ~/.ssh/id_ed25519" `IconButton`. Sorted by path. None: "None saved. A key's passphrase is saved when you tick “Remember in the Vault”." Locked, the section isn't shown, since only an open Vault can say what it holds. |
| Forgetting | Title "Forget this passphrase?", the key's path and full fingerprint, then "The next time this key signs in, Ela asks for its passphrase again. The key file stays as it is." Buttons "Cancel" (focused) and "Forget passphrase" (`danger`). Afterwards "Forgot the passphrase of ~/.ssh/id_ed25519." under the list. |
| Dialog, first time | Title "Create the Vault", with "Ela can't recover this password. If you forget it, the passwords in the Vault are lost." |
| Dialog, later | Title "Unlock the Vault" |
| Why the dialog appeared | "To connect to db-01.", "To save the password of db-01.", "To remove the saved password of db-01." |
| Wrong master password | "That password doesn't open the Vault." under the field |
| Vault doesn't hold it any more | "The Vault no longer holds the password saved for this connection." in the password prompt |
| Editor, saved | "Saved in the Vault" with "Replace" and "Remove" |
| Editor, replacing | "The saved password stays until you save this form." |
| Editor, from a Folder | "Empty uses the password saved on Databases." |
| Editor, nothing saved | "Saved in the Vault. Leave it empty to be asked each time." |
| Folder form, nothing saved | "Saved in the Vault. Leave it empty if this folder has no password of its own." |

Changing the master password opens a dialog titled "Change the master password", with "Current master password", "New master password" and "Repeat new master password". It asks for the current one even when the Vault is unlocked, and the Vault is unlocked afterwards.

| Moment | Copy |
|---|---|
| In the dialog | "Ela can't recover this password. If you forget it, the passwords in the Vault are lost.", "Copies of the old Vault file, such as backups, still open with the old password." |
| Buttons | "Cancel" and "Change password" |
| Wrong current password | "That password doesn't open the Vault." |
| New one too short | "The master password needs at least 8 characters." |
| The two differ | "The two passwords are different." under the repeated field |
| Result | "Master password changed." in the panel |

Nothing ever shows a saved password. Moving a Connection to the Trash keeps its password, so Undo and "Restore" bring back something that still works.

## Trash

| Moment | Copy |
|---|---|
| Nothing in it | "The Trash is empty. Deleted connections and folders stay here until you empty it." |
| A row | The name, then the host, or "Folder, 3 items inside", or "Folder" |
| After restoring | "Restored db-01." inside the dialog, which stays open |
| Unreadable file | "1 file in the Trash can't be read, so it stays there." with its path and reason |
| Confirming | Title "Empty the Trash?", "This permanently removes 3 connections and 1 folder, and the 2 passwords saved only for them. It can't be undone.", buttons "Cancel" and "Empty Trash" |
| Why the Vault asks | "To remove the saved passwords of the items in the Trash." |
| Result | Toast "Trash emptied", with "1 file can't be read and stays there." if one remains |

## Import SSH hosts

| Moment | Copy |
|---|---|
| Asking first | Dialog "Import SSH hosts": "Ela reads ~/.ssh/config and the files it includes. Each Host without a wildcard becomes a connection named after it." then "Names already saved are left as they are, the file isn't changed, and Match exec commands are never run." with "Cancel" and "Import" |
| Result | Toast "Imported 12 connections", with "3 already saved." if any were |
| Nothing to add | Toast "Nothing new in ~/.ssh/config" |
| Parts left out | The same words as the dialog's title, then "Some of ~/.ssh/config was left out:" and one line per reason |
| No file | Toast "Couldn't import ~/.ssh/config", detail "/home/you/.ssh/config doesn't exist." |
| Connecting through a ProxyCommand | `protocol` error: "This connection goes through a ProxyCommand, which Ela can't do yet. It did not connect directly instead." |

## Files

| Moment | Copy |
|---|---|
| Missing | "/srv/x doesn't exist." |
| Refused | "No permission to use /root." |
| Server has no SFTP | "This server doesn't offer SFTP: …" with the server's reason |
| Transfer rows | The percentage while running, then "Done", "Cancelled" or "Failed" with the reason below |
| Native dialog on replacing | Title "Replace this file?", "release.tar.gz already exists in /srv.", buttons "Replace" and "Skip" |
| Native dialog on merging | Title "Merge into this folder?", "site already exists in /srv. Files with the same name are replaced, and the others stay.", buttons "Merge" and "Skip" |
| Folder, left out | "2 links or special files were left out." below "Done" |
| Folder, partly failed | "Failed" with "2 of 40 items failed. First: No permission to use /srv/x." |
| Dragging over the panel | "Drop to upload to /home/you" |
| Name taken | "/srv/backups already exists." |
| Name not allowed | "“a/b” can't be used as a name." |
| Naming dialogs | Titles "New folder" and "Rename deploy.sh", buttons "Cancel" and "Create folder" or "Rename" |
| Deleting a file | Title "Delete this file?", the path, "deploy.sh is removed from db-01. This can't be undone." Buttons "Cancel" and "Delete" (`danger`) |
| Deleting a folder | Title "Delete this folder?", the path, "logs and everything in it are removed from db-01. This can't be undone." |
| Deleting a link | Title "Delete this link?", the path, "Only the link logs is removed from db-01. What it points to stays." |

## Known Hosts

"Key" reads "certificate" in the certificates section. The sentence about other tools is only for SSH keys, because only that file is shared.

| Moment | Copy |
|---|---|
| Confirming | Title "Remove this key?", the server, for a hashed name "Saved as" and its hashed text, and the fingerprint, then "The next connection to this server will ask you to trust it again, without warning that anything changed. ssh and other tools use this file too, so they will ask as well. The previous file stays next to it as known_hosts.old." Buttons "Cancel" and "Remove key" (`danger`). |
| Removed | "Removed db-01. The previous file is kept as known_hosts.old." |
| Already gone | "db-01 is no longer saved in ~/.ssh/known_hosts." |
| A hashed name | Its readable name followed by "hashed" (`xs`, `dim`), or "Hashed name" (`dim`) when no saved Connection matches it |
| Lines left alone | "Left as they are: 2 lines that Ela can't manage here, such as revoked keys." |

## Find in a terminal

| Moment | Copy |
|---|---|
| Field | Placeholder "Find". Buttons "Match case", "Use regular expression", "Previous match", "Next match", "Close" (tooltips and labels). |
| Matches | "3 of 12", "12 results" when none is selected, "1,000+" past the limit (`dim`) |
| No match | "No results" (`dim`, it isn't an error) |
| A regular expression that doesn't parse | "Not a valid pattern" (`danger-text`), and the highlights are cleared |

## Shortcut sheet

| Moment | Copy |
|---|---|
| Host key not saved | "Scroll Lock works until Ela closes, but it couldn't be saved: " and the cause, in `danger-text` under the Host key choice. |

## Empty states

Every empty state is an `EmptyState`: a title that says what is the case, a detail or an action that says what to do next.

| Where | Title | Detail | Action |
|---|---|---|---|
| Tree, first run | "No connections yet." | "Save the machines you use, or bring them in from your SSH config." | "New connection", "Import SSH hosts" |
| Tree, search has no match | "Nothing matches “prod-db”." | | "Connect" under the search field, unless it holds a Tag or several words |
| Trash, nothing in it | "The Trash is empty." | "Deleted connections and folders stay here until you empty it." | |
| Tunnels | "No tunnels." | "Forward a port from any SSH session." | "New tunnel" |
| Files with no SSH Session | "Files work over SSH." | "Open an SSH session to browse its files." | |
| Files, Session still connecting | "Waiting for db-01…" | | |
| Files, Session ended | "This session has ended." | "Reconnect to browse its files." | "Reconnect" |
| Files, empty folder | "This folder is empty." | | |
| Known Hosts, filter has no match | "Nothing matches “prod-db”." | | |

Inside a list, a section with nothing in it says so in one `dim` line under its heading:

| Where | Copy |
|---|---|
| Known Hosts, no SSH keys | "No saved keys. Ela saves one when you trust a server." |
| Known Hosts, no certificates | "No saved certificates. Ela saves one when you trust a server." |
| Recent, nothing opened yet | "Connections you open show up here." The section is left out while nothing is saved at all. |
| Recent, file can't be read | "Recent can't be read: …" in `danger-text`. Opening a Connection starts the list afresh. |

## Loading

- Under 300ms: no indicator. Avoid flicker.
- 300ms to 2s: indicator on the control that started it.
- Over 2s: show text that says what Ela is waiting for ("Waiting for db-01…").
- Ela doesn't use skeleton screens. Lists are local and fast, and Sessions have their own states.

## Numbers and units

- Use `23 ms`, `1.4 MB/s` and `60 fps`, with a space between number and unit.
- Durations: "3 s" when under a minute, "2 min" otherwise.
- Dates in lists: relative under 7 days ("2 h ago"), absolute after that ("12 Oct").
