# Design tokens

Implemented in [`app/src/lib/ui/tokens.css`](../../app/src/lib/ui/tokens.css) as CSS custom properties, exposed to Tailwind through `@theme`. Feature code uses the token names (`bg-panel`, `text-dim`, `border-line`), never raw hex values.

## Colour

### Chassis (neutral UI)

The base follows VS Code's "Dark 2026" theme: near-black, neutral grey with no blue or warm cast, so Ela sits naturally next to the editor our users already stare at all day. The remote screen uses the editor background, the darkest surface. Panels around it are one step lighter, so the remote screen reads as the content, exactly like an editor in VS Code.

| Token | Hex | Use | VS Code source |
|---|---|---|---|
| `chassis` | `#121314` | App background, active Tab, welcome | `editor.background` |
| `screen` | `#121314` | Terminal and framebuffer letterbox | `editor.background` |
| `panel` | `#191A1B` | Sidebar, activity rail, tab strip, status bar | `sideBar.background` |
| `raised` | `#262728` | Hovered rows and menu items, secondary buttons | `list.hoverBackground` over panel |
| `selected` | `#1F3038` | Selected rows (tree, palette, file list, move dialog) and pressed `IconButton`s | `signal` at 18% over `panel`, Ela's own: at 22% `dim` text falls under 4.5:1 |
| `line` | `#2A2B2C` | Dividers between regions | `sideBar.border` |
| `edge` | `#333536` | Input and select borders | `input.border` |
| `dim` | `#999999` | Secondary text, icons at rest | `descriptionForeground` `#8C8C8C`, lightened to keep 4.5:1 on `raised` |
| `text` | `#BFBFBF` | Primary text | `foreground` |

### Signal

| Token | Hex | Use |
|---|---|---|
| `signal` | `#3994BC` | Focus rings, links, the tint in `selected`, active indicators. The only interactive accent (`focusBorder`). |
| `signal-strong` | `#297AA0` | Primary button fill, white text 4.8:1 (`button.background`) |
| `on-accent` | `#FFFFFF` | Text on `signal-strong` and `danger` fills. Tailwind's own palette is switched off, so `white` doesn't exist. |
| `label` | `#F1EAD2` | Label tape background (host identity). Also Ela's own ink, the cream of the mark, for what names where you are or what you are deciding: the welcome headline, dialog and Session panel titles, the active Tab's title, the selected tree row's name and a pressed `IconButton`'s icon. Never body text or actions. |
| `label-ink` | `#1C1C1A` | Text on label tape |
| `danger` | `#B5404A` | Fill for destructive buttons, white text (5.5:1) |
| `danger-text` | `#F48771` | Error titles and messages (`errorForeground`, 7.1:1 on `panel`) |

### Environments (cable colours)

Each Connection or Folder may carry an environment. The colours come from VS Code's own status colours and keep common patch-cable meanings (red for danger, green for safe), so they are easy to tell apart and carry meaning without words. The colour always comes with the environment name, so colour is never the only cue (see [accessibility.md](./accessibility.md)).

| Token | Hex | Default meaning |
|---|---|---|
| `env-prod` | `#F14C4C` | Production. Gets the hatched rail and the guardrails in [identity-and-safety.md](./identity-and-safety.md). |
| `env-staging` | `#CCA700` | Staging or pre-production |
| `env-dev` | `#89D185` | Development |
| `env-lab` | `#B180D7` | Lab, homelab, benchmarks |
| `env-none` | `#7A7F87` | No environment set |

Users can rename environments and pick other colours, but `prod` semantics (guardrails) attach to a flag, not to the colour red.

Environment colours are for marks (rails, swatches, LEDs), never for text: several of them fall below 4.5:1 as text on `panel`. Environment names are written in `text` next to a swatch of the colour. Production's swatch is hatched.

### Link LEDs (Session status)

| State | Colour | Behaviour |
|---|---|---|
| Connecting, verifying, authenticating | `#CCA700` | Slow blink (1 Hz) |
| Connected | `#89D185` | Steady |
| Activity in a background Tab | `#89D185` | Brief flicker on output, like a NIC activity LED |
| Reconnecting | `#CCA700` | Fast blink (2 Hz) |
| Closed normally | `#7A7F87` | Off |
| Closed with error | `#F14C4C` | Steady |

Blinks stop entirely under `prefers-reduced-motion`. The colour and the text label still carry the state.

### Terminal palette

VS Code's default terminal colours on the `screen` background, so output looks exactly as it does in the VS Code terminal. Background `#121314`, foreground `#CCCCCC`, cursor `#BFBFBF`, selection `#3994BC` at 33%. ANSI: black `#000000`, red `#CD3131`, green `#0DBC79`, yellow `#E5E510`, blue `#2472C8`, magenta `#BC3FBC`, cyan `#11A8CD`, white `#E5E5E5`. Bright: `#666666`, `#F14C4C`, `#23D18B`, `#F5F543`, `#3B8EEA`, `#D670D6`, `#29B8DB`, `#E5E5E5`.

Find matches use VS Code 2026 Dark's `editor.findMatchHighlightBackground` (`#27678280`) and `editor.findMatchBackground` (`#27678290`), blended over `screen` to `#1D3D4B` and `#1E4252` because xterm decorations only take `#RRGGBB`. The two are nearly the same, so the selected match also gets a 1px `signal` border. Terminal text `#CCCCCC` keeps 7.2:1 and 6.7:1 on them.

### Light theme

A light theme (following VS Code "Light 2026") ships later with the same token names. Chassis becomes `#FFFFFF`, panel `#F8F8F8`, text `#3B3B3B`, and environment colours darken to keep 3:1 contrast on light panels. The remote screen stays dark unless the user picks a light terminal theme.

## Typography

One superfamily, IBM Plex, which comes from the same engineering and terminal heritage as the tools Ela replaces. It is bundled locally (no network fonts, strict CSP).

| Family | Role |
|---|---|
| IBM Plex Sans | All UI text |
| IBM Plex Sans Condensed | Tab titles, label tape, dense tree rows. Fits roughly 25% more Tabs before truncation. |
| IBM Plex Mono | Text the user types or reads as an address: hostnames, `user@host:port`, fingerprints, paths. Also the terminal's default font, which Settings can change. The Latin subset is bundled, so other scripts and icon glyphs come from the system's fonts. |

Mono is never used for decoration or for UI labels.

### Scale

Base size 13px, the desktop-app norm, so a dense tree stays readable. The steps follow a 1.125 ratio, rounded to whole pixels.

| Token | Size / line height | Weight | Use |
|---|---|---|---|
| `text-xs` | 11 / 14 | 400 | Status bar, badges, shortcut hints |
| `text-sm` | 12 / 16 | 400 | Secondary text, tree meta |
| `text-base` | 13 / 18 | 400 | Default UI text, tree rows, inputs |
| `text-md` | 15 / 20 | 500 | Dialog titles, section titles in settings |
| `text-lg` | 18 / 24 | 500 | Empty-state headline |
| `text-xl` | 24 / 30 | 600 | Welcome tab headline only |

Rules:
- Sentence case everywhere. No all-caps labels, no letter-spaced eyebrows.
- Numbers that change (throughput, fps, counts) use `font-variant-numeric: tabular-nums` so they do not jitter.
- Line length in dialogs and docs panes stays under 72 characters.

## Spacing and sizing

A 4px base grid. The common steps are 4, 8, 12, 16, 24 and 32.

| Element | Size |
|---|---|
| Activity rail width | 44px |
| Tree row height | 26px (compact 22px) |
| Tab height | 34px |
| Status bar height | 24px |
| Input and button height | 28px (dialogs 32px) |
| Hit target minimum | 24×24px, with 28×28px preferred for icon buttons |

## Radius

Radius encodes hierarchy, so it differs by element type.

| Token | Value | Use |
|---|---|---|
| `radius-none` | 0 | Session frame, tab strip, panels: anything structural |
| `radius-sm` | 3px | Inputs, buttons, tree row highlight, label tape |
| `radius-md` | 6px | Popovers, menus, toasts |
| `radius-lg` | 10px | Dialogs |

## Elevation

The app is mostly flat. Elevation is reserved for things that float above the remote screen.

| Level | Treatment | Use |
|---|---|---|
| 0 | none | Panels, tabs |
| 1 | `0 1px 0 #00000040` + `line` border | Popovers, menus |
| 2 | `0 8px 24px #00000066` + `line` border | Dialogs, command palette |

## Motion

| Token | Value | Use |
|---|---|---|
| `motion-fast` | 90ms ease-out | Hover, press |
| `motion-base` | 160ms cubic-bezier(.2,.7,.2,1) | Panels opening, popovers |
| `motion-led` | 1s steps(2) | Connecting LED blink |

Ela has no entrance animations, staggered reveals or animated backgrounds. Motion only answers a user action or shows a link state. Everything respects `prefers-reduced-motion: reduce` (durations drop to 0 and LEDs stop blinking).
