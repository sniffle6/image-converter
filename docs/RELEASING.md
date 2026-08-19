# Releasing Convertalot

Releases are cut by pushing an annotated `vX.Y.Z` git tag. The `Release` GitHub
Actions workflow (`.github/workflows/release.yml`) builds, tests, packages, and
publishes the release automatically — nothing is built or uploaded by hand.

## Tag conventions

- Tags are `v` + the exact `version` in `Cargo.toml` (`v0.1.0`, `v1.2.3`).
- The workflow fails if the tag and `Cargo.toml` disagree, so versions cannot drift.
- A tag containing a hyphen (`v0.2.0-rc.1`) is published as a **pre-release**;
  the installer's "latest" lookup skips pre-releases.
- Use annotated tags (`git tag -a`) so the tag records who cut the release and when.

## Cutting a release

1. Bump `version` in `Cargo.toml`, then run `cargo check` so `Cargo.lock`
   picks up the new version. Commit both files and merge to `main` through a PR.
2. Tag the release commit on `main` and push the tag:

   ```powershell
   git switch main
   git pull origin main
   git tag -a v0.2.0 -m "Convertalot v0.2.0"
   git push origin v0.2.0
   ```

3. The `Release` workflow then:
   - verifies the tag matches `Cargo.toml`,
   - runs `cargo test --locked`,
   - builds `image-converter.exe` and `image-converter-gui.exe` with `cargo build --release --locked`,
   - smoke-tests the built CLI (`image-converter.exe --version`),
   - zips both executables (plus `README.md`) into `convertalot-vX.Y.Z-x86_64-pc-windows-msvc.zip`,
   - writes a `SHA256SUMS.txt` checksum file,
   - creates the GitHub release with auto-generated notes and both assets attached.

   Watch it with `gh run watch` or on the Actions tab.

4. Verify the release installs:

   ```powershell
   irm https://raw.githubusercontent.com/sniffle6/image-converter/main/install.ps1 | iex
   ```

## Fixing a bad release

- **Workflow failed, no release created:** fix the problem on `main`, then move the
  tag: `git tag -fa vX.Y.Z -m "Convertalot vX.Y.Z"` and
  `git push --force origin vX.Y.Z`. The workflow re-runs on the forced push.
- **Release published but broken:** prefer shipping a fixed `vX.Y.(Z+1)` over
  mutating a published release — installers may have already downloaded it.
  Delete the bad release/tag only if it never worked at all:
  `gh release delete vX.Y.Z --cleanup-tag`.
