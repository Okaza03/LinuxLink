# Releasing

Releases are built and published by GitHub Actions when a version tag is
pushed. Each release gets every package format for x86_64 and ARM64, a
`SHA256SUMS` file, and release notes taken from `CHANGELOG.md`.

## Steps

1. **Update the version** in the root `Cargo.toml` (`[workspace.package]
   version`) and run `cargo check` so `Cargo.lock` follows.
2. **Write the release notes.** Move the entries under `## [Unreleased]` in
   `CHANGELOG.md` into a new section, `## [X.Y.Z] - YYYY-MM-DD`, and update the
   links at the bottom. Be detailed: this text appears in the app's **What's
   new** window and as the GitHub release notes. A unit test fails if the
   current version has no section.
3. **Update the metadata:** add a `<release>` entry to
   `packaging/linux/io.github.Okaza03.LinuxLink.metainfo.xml` and bump
   `pkgver` in `packaging/arch/PKGBUILD`.
4. **Optional dry run:** run the *Release* workflow manually (Actions → Release
   → Run workflow). It builds every package and uploads them as workflow
   artifacts without publishing anything.
5. **Tag and push:**

   ```sh
   git commit -am "Release vX.Y.Z"
   git tag -a vX.Y.Z -m "LinuxLink X.Y.Z"
   git push origin main vX.Y.Z
   ```

The workflow then builds everything and publishes the release.

## File names are a contract

The in-app updater finds its download by exact file name, so these must not
change without updating `InstallKind::asset_name` in
`apps/LinuxLink/src/updater.rs`:

| Format | File |
| --- | --- |
| AppImage | `LinuxLink-X.Y.Z-<x86_64\|aarch64>.AppImage` |
| Flatpak | `LinuxLink-X.Y.Z-<x86_64\|aarch64>.flatpak` |
| Debian | `linuxlink_X.Y.Z_<amd64\|arm64>.deb` |
| RPM | `linuxlink-X.Y.Z-1.<x86_64\|aarch64>.rpm` |
| Arch | `linuxlink-X.Y.Z-1-x86_64.pkg.tar.zst` |
| Tarball | `linuxlink-X.Y.Z-<x86_64\|aarch64>-linux.tar.gz` |
| Checksums | `SHA256SUMS` |
