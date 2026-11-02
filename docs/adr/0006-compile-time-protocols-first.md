# Compile-time Protocols first, runtime plugins later

Protocols, Secret Stores, Transports and Importers are Rust crates behind traits, enabled through Cargo features. Simple external tools are added as declarative TOML Launchers. We are deliberately not building a runtime plugin system (WASM or JSON-RPC subprocess) yet: it would freeze a public API, need sandboxing and need plugin distribution before anyone outside the project writes plugins. The traits are kept narrow so that a WASM or RPC adapter can wrap them later without changing the core.
