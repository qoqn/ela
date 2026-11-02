# Identity and safety

The most expensive mistake in Ela is acting on the wrong machine. This document defines the visual system that prevents it. It is the one place where Ela is visually loud.

## The three identity marks

### 1. Environment rail

A strip of the environment's cable colour (see [tokens.md](./tokens.md)) that marks everything belonging to an environment:

| Where | Form |
|---|---|
| Connection tree | 3px vertical bar at the right edge of the row, for the Folder and each Connection under it |
| Tab | 3px bar along the top edge of the Tab |
| Session rail | Full-width 26px band above the remote screen: environment colour at 18% over `panel`, with a 2px solid line in full colour at the bottom. Identity on the left, the Session's actions on the right (see [interaction.md](./interaction.md#session-rail-actions)). |

**Production is hatched.** Wherever a rail is shown for a Connection flagged as production, it uses 45° stripes (environment colour alternating with a darker shade) instead of a flat fill. The pattern survives colour blindness, greyscale screenshots and screen sharing over poor video. No other environment may use a pattern.

### 2. Label tape

The host's name printed on a strip that looks like a cable label: `label` background, `label-ink` text, IBM Plex Sans Condensed 600, `radius-sm`, and a 1px darker bottom edge so it reads as a physical label.

- It always appears at the left of the session rail and shows the Connection name, plus `user` in a lighter weight when the user differs from the default.
- In Tabs, the Tab title uses the same text, but plain, because the tape would be too heavy at that size.
- The label shows the name the user gave the Connection, falling back to the hostname. The full address is in the status bar and in the tape's tooltip.

### 3. Link LED

A 7px round LED before every Tab title, and at the right of the row for every open Connection in the tree and Recent, where it can't push the name out of line with its neighbours. It shows the state of the link (colours and blink rates in [tokens.md](./tokens.md)). A Connection open in several Tabs shows how many beside the LED, and the state that needs the user most: failed, waiting, reconnecting, connecting, connected, closed. For Tabs in the background, the LED **flickers on output**, like a NIC activity LED, so the user can see a long job print something without switching to it.

## Guardrails for production

Connections flagged `production` (directly or inherited from a Folder) add friction in exactly three places and nowhere else:

1. **Broadcast.** Including a production Session in Broadcast requires an explicit confirmation that names the production hosts. While Broadcast is active, every receiving Session's rail turns into a pulsing outline. Under reduced motion it becomes a static double outline instead.
2. **Paste.** Pasting more than one line into a production terminal shows a preview with the line count and the first lines, and a "Paste 14 lines" button. This can be turned off per Connection.
3. **Closing with a running process.** Closing a production Tab whose shell has a foreground process other than the shell asks for confirmation.

Production never changes the terminal's own colours. The remote screen belongs to the remote machine.

## What the user can configure

- Environments: name, colour from a curated set of 10 cable colours, and the production flag.
- Per Connection: override environment, label text, and paste guard.
- Users cannot remove the label tape or the session rail. They are the product's safety contract.

## Rationale

MobaXterm and Remmina mark hosts with icons or not at all, and their tabs look identical whether the machine is a laptop VM or a payment database. Colour-coded environments are common in database tools and cloud consoles because they work. Ela makes them structural: the same mark, in the same place, at every level, so the eye learns it once.
