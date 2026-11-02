# Svelte 5 for the frontend

The frontend is Svelte 5 (runes) + TypeScript built with plain Vite as an SPA, without SvelteKit, because a desktop app needs neither routing nor SSR. Svelte compiles to small, fast code and runes handle high-frequency state (terminal and Session status updates) without a store library. Components are hand-built on native elements; the shadcn-svelte/bits-ui we first planned turned out not to be needed.

## Considered options

- **React**: excluded by project preference.
- **SolidJS**: comparable performance, but a thinner component ecosystem.
- **Leptos / Dioxus (Rust in WASM)**: would give one language, but xterm.js and canvas interop through wasm-bindgen is awkward, the UI component ecosystem is tiny, and compile times slow down UI iteration.
