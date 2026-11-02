# Security

Ela handles passwords, key passphrases and connections to other people's machines, and it hasn't had an outside security review.

## Reporting a vulnerability

Please report it privately through the repository's **Security** tab, under **Report a vulnerability**, rather than in a public issue. Include what you found, how to reproduce it, and which version or commit you used.

Fixes go to the latest release only.

## How secrets are handled

Secrets travel from the UI to Rust once and are never sent back to the webview ([ADR-0002](docs/adr/0002-webview-never-sees-secrets.md)). A bug that lets the webview read a Secret, or lets it reach logs or saved Connections, counts as a vulnerability.
