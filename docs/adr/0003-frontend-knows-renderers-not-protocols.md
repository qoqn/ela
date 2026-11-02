# The frontend knows Renderers, not Protocols

The UI never branches on Protocol IDs. Each Protocol declares a `RendererKind` (Terminal, Framebuffer, FileBrowser, Web, External) and a JSON settings schema. The UI picks the view from the Renderer and builds the Connection editor form from the schema. New Protocols keep arriving (SPICE, X2Go, S3, kubectl exec…), but new kinds of view almost never do, so adding a Protocol becomes a Rust-only change. The trade-off is that the generated forms are less hand-tuned than bespoke per-Protocol editors.
