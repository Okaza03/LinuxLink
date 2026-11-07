# Releasing

Releases are built and published by GitHub Actions. Each release gets every
package format for x86_64 and ARM64, a `SHA256SUMS` file, and release notes
taken from `CHANGELOG.md`. It also deploys the website and the Flatpak
repository to <https://okaza03.github.io/LinuxLink/>.

## Steps

1. **Make a branch named `release-X.Y.Z`.** The name is what triggers the
   release when the pull request is merged.
2. **Update the version** in the root `Cargo.toml` (`[workspace.package]
   version`) and run `cargo check` so `Cargo.lock` follows.
3. **Write the release notes.** Move the entries under `## [Unreleased]` in
   `CHANGELOG.md` into a new section, `## [X.Y.Z] - YYYY-MM-DD`, and update the
   links at the bottom. Be detailed: this text appears in the app's **What's
   new** window and as the GitHub release notes. A unit test fails if the
   current version has no section.
4. **Update the metadata:** add a `<release>` entry to
   `packaging/linux/io.github.Okaza03.LinuxLink.metainfo.xml` and bump
   `pkgver` in `packaging/arch/PKGBUILD`.
5. **Optional dry run:** run the *Release* workflow manually on the branch
   (Actions → Release → Run workflow). It builds every package and the website
   and uploads them as workflow artifacts without publishing anything. The
   `github-pages` artifact is the website, ready to open.
6. **Open a pull request and merge it.** *Tag release* (`tag-release.yml`)
   then tags the merge `vX.Y.Z` and starts *Release* on that tag. It refuses if
   the branch name doesn't match `Cargo.toml`'s version or the tag exists.

*Release* then builds everything, deploys the website and Flatpak repository
to GitHub Pages, and only then publishes the GitHub release. That order
matters: the in-app updater sees a release as soon as it exists, and by then
`flatpak update` has to find the same version.

Pushing a `vX.Y.Z` tag by hand still works too.

## The Flatpak repository

Each Flatpak runner (x86_64 and aarch64) builds the app and uploads it as a
bundle. The *Flatpak repository and website* job imports both into one
repository, signing every commit, and runs `packaging/flatpak/publish-repo.sh`.
That script signs the summary, writes `linuxlink.flatpakrepo` and
`linuxlink.flatpakref`, builds the release's two `.flatpak` bundles (which
name the repository, so bundle installs get `flatpak update` too), and fills
in `packaging/site/index.html`. The site and the repository are deployed
together, so every deploy contains both.

The repository holds only the latest release. Clients with an older one just
pull the new commit.

To try it locally, build the app into a repository with flatpak-builder (see
[building.md](building.md)), then:

```sh
LINUXLINK_FLATPAK_BASE=http://127.0.0.1:8765 packaging/flatpak/publish-repo.sh target/flatpak-repo target/site target/dist
python3 -m http.server -d target/site 8765
FLATPAK_USER_DIR=/tmp/flatpak-test flatpak install --user http://127.0.0.1:8765/linuxlink.flatpakref
```

## One-time setup

Already done for this repository; kept here in case it has to be redone.

1. **Signing key**, used for every release from now on. Users' flatpak
   installs trust this key, so **keep the backup safe**: losing it means every
   user has to reinstall.

   ```fish
   set -x GNUPGHOME (mktemp -d)
   gpg --batch --passphrase '' --quick-gen-key "LinuxLink Flatpak repository" ed25519 sign never
   gpg --armor --export-secret-keys > ~/linuxlink-flatpak-key.asc
   gh secret set FLATPAK_GPG_KEY --repo Okaza03/LinuxLink < ~/linuxlink-flatpak-key.asc
   rm -rf $GNUPGHOME; set -e GNUPGHOME
   ```

   Without the `FLATPAK_GPG_KEY` secret, a tagged release fails with a message
   saying so. Dry runs build an unsigned repository.
2. **GitHub Pages**, deployed by Actions:
   `gh api -X POST repos/Okaza03/LinuxLink/pages -f build_type=workflow`
3. **Let tags deploy** to the `github-pages` environment:
   `gh api -X POST repos/Okaza03/LinuxLink/environments/github-pages/deployment-branch-policies -f name='v*' -f type=tag`

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
