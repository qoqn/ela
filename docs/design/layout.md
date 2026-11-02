# Layout

## Regions

```
┌────┬──────────────────┬─────────────────────────────────────────────┬───────────┐
│    │ Search / connect │ ▌prod  db-01 ×│▌dev  api ×│▌lab  bench ×│ + │           │  tab strip 34px
│ ⌂  ├──────────────────┼═════════════════════════════════════════════┤  Tool     │
│    │ ▸ Production   ▌ │[ db-01  root ]▐▐▐▐▐▐▐ prod ▐▐▐▐▐▐▐▐▐▐▐▐▐▐▐▐│  panel    │  ← session rail
│ ⇅  │   ● db-01      ▌ │                                             │  (Files,  │
│    │   ● db-02      ▌ │                                             │  Tunnels, │
│ ⇄  │ ▸ Staging      ▌ │         remote screen (terminal / RDP)      │  Metrics) │
│    │ ▸ Homelab      ▌ │                                             │           │
│ ✎  │                  │                                             │  280px,   │
│    │                  │                                             │  optional │
│ ⚙  │ Recent           │                                             │           │
├────┴──────────────────┴─────────────────────────────────────────────┴───────────┤
│ ● Connected  ssh  root@db-01.internal:22   SHA256:9f…c2   23 ms   ⇄ 2 tunnels  ∿ │  status bar 24px
└──────────────────────────────────────────────────────────────────────────────────┘
 rail   sidebar 264px       workspace (flex)                           tool panel
 44px   (200–480, resizable)
```

| Region | Content | Notes |
|---|---|---|
| Activity rail | Connections, Files, Vault, Known Hosts, then Development tools (development builds only) and Settings at the bottom. Tunnels and Snippets join after Files once they are built: the rail never shows a control that does nothing | Icons with tooltips. Selecting one swaps the sidebar content, always a `Panel`. Files follows the active Tab. Development tools opens a dialog with the test servers and benchmarks. [Settings](./interaction.md#settings) opens a page over the Tab strip and the Session. |
| Sidebar | The "Connections" header with New connection and New folder, the search and connect field, then the Connection tree, Recent (see [interaction.md](./interaction.md#recent)), and the Trash row at the bottom | Resizable (200 to 480px) by dragging its edge, or with `←`/`→`, `Home` and `End` on the edge, and each section remembers its own width on this machine. Double-clicking the edge restores the default: 264px, and 320px for Files and Known Hosts, which show long paths and fingerprints. Collapsible (`Ctrl+B`), which leaves only the rail. |
| Tab strip | One Tab per Session, in order of opening | The active Tab sits on `chassis` with its title in `label`. Pinned Tabs on the left. Overflow scrolls horizontally and adds a "more" menu with search. |
| Session rail | Label tape, Environment name and the Session's actions, over the environment rail across the full width | Always visible above the remote screen, 26px tall, in a fullscreen window too. The address is in the status bar and the tape's tooltip. See [identity-and-safety.md](./identity-and-safety.md). |
| Remote screen | Terminal or framebuffer | Takes all remaining space. Split Panes divide this region, and each Pane gets its own session rail. |
| Tool panel | Tunnels, host metrics for the active Session | Right side by default, can dock bottom. Hidden until opened, and remembers its state per Protocol. |
| Status bar | Link state, Protocol, address, fingerprint, latency, Tunnels, Broadcast, throughput | Each item is clickable and opens the relevant detail. |

## Alignment

- Text in the chrome is left-aligned. Ela never centres text in lists, forms or menus.
- Centred text is used only in empty states and in the welcome Tab.
- Numbers in the status bar and stats overlays are right-aligned within fixed-width slots so they do not shift as they change.
- Icons sit on the 4px grid and align with the text baseline of their row.

## Window sizes

| Width | Behaviour |
|---|---|
| ≥ 1280px | Full layout, tool panel can be open beside the workspace |
| 1024–1279px | The tool panel overlays the workspace instead of pushing it |
| 900–1023px | The sidebar collapses to the rail by default |
| < 900px | Not supported; this is the minimum window width |

The minimum window size is 900×600.

## Split panes

- Split right (`Ctrl+Shift+D`) and split down (`Ctrl+Shift+E`) divide the active Pane.
- Each Pane has its own session rail, so identity stays per Pane.
- The focused Pane's rail is full strength, and unfocused Panes' rails drop to 60% opacity. The label tape stays fully legible either way.
- Dragging a Tab onto a Pane edge splits there. Dragging it out of the window opens a new window.

## Welcome tab

The welcome Tab shows when no Sessions are open. It says what to do and offers the ways to do it, each a row that runs the command, with its shortcut on the right:

1. [The mark](./README.md#the-mark), 48px tall, then the headline "Connect to a machine" (`text-xl`, `label`), and one `dim` line: "Open a saved connection, or type an address like root@10.0.0.5 in the search field."
2. "Open a connection" (the connect palette, `Ctrl+T`), "Search or connect", "Command palette", "Shortcut sheet".

With nothing saved yet, the line reads "Type an address like root@10.0.0.5 in the search field, or import the hosts in your SSH config.", "Import SSH hosts" comes first, without a shortcut, and "Open a connection" is left out because its palette would be empty.

3. Once something has been opened, "Recent" (`text-sm` semibold, `dim`) and the last 5 Connections as the same rows: environment rail, name, host in mono `dim`, and when on the right. A click connects. It repeats the sidebar's Recent on purpose: the Welcome Tab is also seen with the sidebar collapsed.

Beyond that small mark it has no marketing copy, logo splash or tips carousel.
