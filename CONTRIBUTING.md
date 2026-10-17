# Contributing to LinuxLink

Thanks for helping! Bug reports, ideas, documentation and code are all welcome.
By taking part you agree to follow our [Code of Conduct](CODE_OF_CONDUCT.md).

## Reporting bugs and asking for features

- Search [existing issues](https://github.com/Okaza03/LinuxLink/issues) first.
- Use the **Bug report** or **Feature request** template. For bugs, the
  version, how you installed LinuxLink, your distribution and the output of
  running `linuxlink` in a terminal help a lot.
- Security problems: please don't open an issue; see [SECURITY.md](SECURITY.md).

## Development setup

See [docs/building.md](docs/building.md) for dependencies. Then:

```sh
git clone https://github.com/Okaza03/LinuxLink.git
cd LinuxLink
cargo run -p linuxlink
```

To use your build every day, `cargo xtask install` installs it as
**LinuxLink Test**, separate from any released LinuxLink you have installed.
[docs/how-it-works.md](docs/how-it-works.md) explains the architecture.

## Making a change

1. Fork the repository and create a branch from `main`.
2. Keep changes focused: one fix or feature per pull request.
3. Match the surrounding code's style. Comments explain *why*, not *what*.
4. Add or update tests where it makes sense (`cargo test --workspace`).
5. Add a line under `## [Unreleased]` in [CHANGELOG.md](CHANGELOG.md)
   describing the change for users. It becomes the release notes and the
   in-app **What's new** text, so write it for them.
6. Make sure the checks pass:

   ```sh
   cargo fmt --all
   cargo clippy --workspace --all-targets -- -D warnings
   cargo test --workspace
   ```

7. Open a pull request and fill in the template.

### Things to keep in mind

- **Settings compatibility.** Users' settings live in
  `~/.config/linuxlink/config.toml`. New fields need defaults so old files
  keep loading; never make a change that resets someone's setup.
- **Cleanup.** Every PipeWire module LinuxLink loads must be named with the
  `linuxlink_` prefix so it can be cleaned up after a crash.
- **Release file names** are relied on by the updater; see
  [docs/releasing.md](docs/releasing.md).
- **Flatpak.** Host tools must run through `sys::host_command`, and host paths
  through `sys::host_path`.

## License

Unless you explicitly state otherwise, any contribution you intentionally
submit for inclusion in LinuxLink is dual licensed under the
[MIT](LICENSE-MIT) and [Apache-2.0](LICENSE-APACHE) licenses, without any
additional terms or conditions.
