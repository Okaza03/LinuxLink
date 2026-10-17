# Security policy

## Supported versions

Security fixes go into the latest release. LinuxLink updates itself (AppImage,
tarball) or offers the update in the app (other formats), so please stay on
the newest version.

| Version | Supported |
| --- | --- |
| Latest release | ✅ |
| Older releases | ❌ |

## Reporting a vulnerability

Please **don't** open a public issue. Report it privately through GitHub:
**[Report a vulnerability](https://github.com/Okaza03/LinuxLink/security/advisories/new)**.

Include what you found, how to reproduce it, and its impact. You should get a
reply within a week. Once a fix is released, the advisory is published with
credit to you, unless you prefer otherwise.

## Scope notes

Areas where security matters most:

- **The updater** downloads release files from GitHub and installs them. Every
  file is checked against the release's `SHA256SUMS` before use.
- **PipeWire control:** LinuxLink loads modules and moves audio streams in your
  session through `pactl`.
- **The Flatpak** can run host commands (`flatpak-spawn --host`) because it
  manages the whole session's audio; it is not a security sandbox.
