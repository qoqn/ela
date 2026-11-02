# Changelog

Versions follow [Semantic Versioning](https://semver.org). How a release is made: [docs/releasing.md](docs/releasing.md).

## Unreleased

Everything so far, on the way to 0.1.0. Linux only.

- SSH terminal with identity files, ssh-agent, passwords and keyboard-interactive sign-in. Reads `~/.ssh/config` and `known_hosts` the way OpenSSH does.
- Jump Hosts, including chains, for SSH and RDP.
- File browser over SSH: upload, download, rename, delete, drag and drop.
- RDP desktop that follows the window size, with shared clipboard text and pinned certificates. Works with GNOME Remote Desktop.
- A remote desktop in a Tab out of view uses almost no CPU or bandwidth until it is shown again.
- The RDP cursor is drawn locally with the server's shape, so it moves without lag, shows only once, and hides when the server hides it.
- Connection tree with Folders, inherited settings, tags and a Trash. Each Connection is one TOML file.
- Vault for passwords and key passphrases, encrypted with AES-256-GCM and a key from Argon2id.
- Command palette, terminal search, a choice of host key, and the terminal's font and size.
