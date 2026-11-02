# Releasing

The version lives in one place: `version` under `[workspace.package]` in the root `Cargo.toml`. The crates inherit it and Tauri reads it from there.

Versions follow SemVer. Before 1.0, a minor bump (0.1 → 0.2) can break things and a patch bump (0.1.0 → 0.1.1) only fixes them. Saved files (Connections, Vault, settings) are migrated, not broken, by any version.

1. Set the version and refresh the lockfile: `cargo check --workspace`.
2. Regenerate the licences that packages carry: `cd app && bun run licenses`, which needs `cargo install --locked --features cli cargo-about`. It fails when a dependency has a licence `app/scripts/licenses/about.toml` doesn't accept; check that one against the GPL before adding it. Commit the result.
3. In `CHANGELOG.md`, move what's under "Unreleased" to a new heading with the version and date.
4. Run the checks in `CLAUDE.md`, including the fixture tests.
5. Commit as `Release x.y.z` and tag it: `git tag -a vx.y.z -m "Ela x.y.z"`.
6. Build the packages: `cd app && bun tauri build`. They land in `target/release/bundle/`.
7. Push the commit and tag, then publish: `gh release create vx.y.z --notes "…" target/release/bundle/deb/*.deb target/release/bundle/rpm/*.rpm target/release/bundle/appimage/*.AppImage`. The notes are that version's changelog section.

Hold back the AppImage for now. It also carries system libraries such as GTK and WebKitGTK, and their LGPL notices aren't in `THIRD-PARTY-LICENSES`. Leave it out of the upload in the last step.
