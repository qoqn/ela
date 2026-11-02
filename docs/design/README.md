# Ela design

UI and UX are part of Ela's architecture, not a finishing coat. This folder is the source of truth for how Ela looks, behaves and speaks. Code follows these documents; when they disagree, fix one of them in the same change.

| Document | Covers |
|---|---|
| [tokens.md](./tokens.md) | Colour, type, spacing, radius, elevation, motion |
| [layout.md](./layout.md) | Window regions, sizes, resizing, what lives where |
| [identity-and-safety.md](./identity-and-safety.md) | Environments, label tape, link LEDs, guardrails for production |
| [interaction.md](./interaction.md) | Keyboard model, focus, shortcuts, palette, drag and drop |
| [states-and-copy.md](./states-and-copy.md) | Session states, empty and error states, voice and vocabulary |
| [accessibility.md](./accessibility.md) | Contrast, focus, motion, screen readers, scaling |
| [components.md](./components.md) | The `app/src/lib/ui` primitives and their rules |

Vocabulary follows [GLOSSARY.md](../../GLOSSARY.md): the UI says **Connection**, **Folder**, **Session**, **Tab**, **Tunnel**, never "host entry", "profile" or "bookmark".

## Who we design for

Ela is for people who keep many machines in their heads: sysadmins, SREs, developers on call and homelab owners. They spend hours a day in Ela, mostly on the keyboard, often under pressure. Two jobs come first:

1. **Get to the right machine fast.** Typing an address, searching the tree or reopening a recent Session should all take seconds.
2. **Always know which machine you are on.** The most expensive mistake in this product is a command typed into production by someone who thought they were on staging. Identity is never hidden, never subtle, and always in the same place.

## Principles

**Identity is the loudest thing on screen.** Ela spends its visual boldness in one place: the environment colour and label tape that follow a Session from the tree to its Tab, its frame and the status bar. Everything else stays quiet, so identity reads instantly. See [identity-and-safety.md](./identity-and-safety.md).

**The remote screen is the content.** The terminal or remote desktop gets the space. Chrome is thin, neutral and never overlaps the remote screen except for what the user summons, such as the find bar.

**Keyboard first, mouse complete.** Every action has a keyboard path, and the command palette reaches everything. Every action is also discoverable with the mouse, because a sixty-year-old Windows admin is as much our user as a tiling-WM enthusiast.

**Hardware, not marketing.** The visual language borrows from the subject's physical world: rack hardware, coloured patch cables, printed cable labels, link and activity LEDs. The product has no gradients, glassmorphism or decorative illustration.

**Dense but calm.** These users want many Sessions and long trees on screen. Ela uses condensed type and tight rows where scanning matters, and generous space only around decisions (dialogs, warnings).

**States are honest.** Ela never shows "Connected" before it is connected. Every failure says what happened and what to do next, in plain words.

**Respect the remote keyboard.** When a Session has focus, keys belong to the remote side. Ela takes keys back only through a small, documented set of shortcuts (see [interaction.md](./interaction.md)).

## The mark

Ela's mark is a pixel-art hermit crab whose borrowed shell is a terminal showing `>_`: Ela lives in other machines' shells. It is drawn on a 20×20 grid in five colours: `label` cream for the shell with a darker cream (`#c2b893`) for its rim, `signal` blue for the prompt, and coral (`#f48771`, `#b5404a`) for the crab. The coral reuses the values of `danger-text` and `danger` but is not a token, and the mark does not follow them if they change.

| File | Use |
|---|---|
| `app/src-tauri/icons/app-icon.svg` | App icon on a `chassis` tile with rounded corners. `bunx tauri icon src-tauri/icons/app-icon.svg` in `app/` makes the PNG, ICO and ICNS files from it; delete the `android/` and `ios/` folders it also writes. |
| `app/src/lib/ui/logo.svg` | The crab alone, cropped to its 19×16 pixels, for the welcome Tab. |

Show it at a whole number of screen pixels per grid pixel (the welcome Tab uses 3, so 57×48) so its edges stay sharp. It is the only illustration in the product; it appears on the welcome Tab and nowhere else in the chrome.

## Process

- New UI starts from these documents and from the primitives in `app/src/lib/ui`. A one-off style in a feature component is a smell. Either it belongs in a primitive, or the design docs need a new rule.
- Every visible string goes through the voice rules in [states-and-copy.md](./states-and-copy.md).
- Changes that alter a principle or token get a short note in the change description explaining why.
- Before merging UI work, check it at 1024×640 and at 2560×1440, with keyboard only, and with `prefers-reduced-motion`.
