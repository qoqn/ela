# Accessibility

The quality floor every screen meets. It is not optional and it is not a later phase.

## Contrast

| Pair | Minimum |
|---|---|
| `text` on `chassis`, `panel` or `raised` | 7:1 |
| `dim` on `panel` | 4.5:1 |
| `text` on `selected` | 7:1 |
| `dim` on `selected` | 4.5:1 |
| `label` on `chassis`, `panel` or `selected` | 7:1 |
| `label-ink` on `label` | 7:1 |
| Environment colours on `panel` (as rails and LEDs) | 3:1 (non-text contrast) |
| Focus ring `signal` against adjacent colours | 3:1 |

Check these values whenever a token changes. The light theme has its own values with the same minimums.

## Never colour alone

- Environment rails always come with the environment name (in the tree tooltip and the session rail text).
- Production also has the hatched pattern.
- LED states also appear as text in the status bar and in the Tab's tooltip.
- Errors use a title, not just red.

## Keyboard

- Every action is reachable without a mouse (see [interaction.md](./interaction.md)).
- Tab order follows the visual order: rail, sidebar, tab strip, Session, tool panel, status bar.
- Focus is always visible on keyboard navigation (`:focus-visible` ring) and never trapped, except inside modal dialogs.
- Keyboard grab in framebuffer Sessions can always be released with the host key, and the host key is displayed on screen.

## Screen readers

| Region | Role |
|---|---|
| Connection tree | `tree` / `treeitem` with `aria-expanded`, `aria-level`, and label "db-01, production, connected" |
| Tab strip | `tablist` / `tab` with `aria-selected`. The LED state is part of the accessible name. |
| Session area | `tabpanel`. The terminal uses xterm.js screen-reader mode, toggled in Settings. Framebuffer Sessions are announced as "Remote desktop, db-01", with no pixel content. |
| Status changes | A polite live region announces "Connected to db-01", "Disconnected" and errors |
| Dialogs | `dialog` with a labelled title. Initial focus goes to the safest action. |

## Motion

- `prefers-reduced-motion: reduce` sets all durations to 0, stops LED blinking, and replaces the Broadcast pulse with a static double outline.
- Ela has no motion that conveys information by itself.

## Scaling

- The UI respects the system font scale and the app's own zoom (`Ctrl+=`/`Ctrl+-` in chrome, separate from terminal font size).
- Layout must survive 150% zoom at the 1280×800 window size.
- Terminal font size is per Connection, with a global default.

## Testing checklist for UI changes

- [ ] Keyboard only: reach, operate and leave the new UI.
- [ ] Screen reader (Orca on Linux): names and states are announced.
- [ ] Reduced motion on.
- [ ] 150% zoom at 1280×800.
- [ ] Greyscale screenshot: production is still distinguishable.
