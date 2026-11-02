# Config is split from Secrets and stored one file per item

Connections and Folders are plain TOML files, one per item and named by UUID, each with a `schema_version`. Secrets live only in the encrypted Vault. This lets people commit, share or sync (git, Syncthing, cloud folders) their connection tree without leaking credentials, and per-item files make sync conflicts rare and local. We rejected a single JSON document (one edit conflicts with everything) and SQLite (opaque to diff and sync tools). The cost is slower loading for very large trees, which an in-memory index can fix if needed.
