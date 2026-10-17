# How it works

LinuxLink doesn't process audio itself. It builds a graph of virtual devices in
PipeWire and keeps that graph in sync with your settings.

## The audio graph

```text
 apps ──▶ [channel sink] ──send──▶ [mix sink "My Mix"] ──▶ your headphones   (🎧 on)
                         └─send──▶ [mix sink "Stream"] ──▶ "LinuxLink Stream" virtual mic
 mic ────────────────────send──▶ (any mix)
```

| Piece | PipeWire object | Name |
| --- | --- | --- |
| App channel | `module-null-sink` | `linuxlink_ch_<id>` |
| Mix | `module-null-sink` | `linuxlink_mix_<id>` |
| Mix as microphone | `module-remap-source` of the mix's monitor | `linuxlink_mixsrc_<id>` |
| Send (one matrix cell) | `module-loopback` from channel to mix | |
| Mix output (🎧 or extra output) | `module-loopback` from mix to a hardware sink | |

Volumes map onto that graph like this:

- **A cell's fader** is the volume of its send loopback: channel master × cell
  fader. Two mixes can hold the same channel at different volumes because each
  has its own loopback.
- **An input channel's master** is the hardware source's own volume, so other
  apps see the same level.
- **A mix's master** is the volume of the mix sink.

Apps are routed by moving their streams (`pactl move-sink-input`) into a
channel sink. Rules are stored per application and matched against the
stream's binary name, application name or Flatpak app ID.

## Code map

All code lives in `apps/LinuxLink/src/`:

| File | Responsibility |
| --- | --- |
| `main.rs` | Startup, single-instance lock, signal handling, restart after update |
| `engine.rs` | Builds and reconciles the PipeWire graph on a worker thread |
| `pactl.rs` | Thin wrapper around `pactl` (JSON output) |
| `meter.rs` | Peak meters, one low-rate `parec` capture per source |
| `config.rs` | Settings model, load/save (`~/.config/linuxlink/config.toml`) |
| `ui.rs` | The egui interface |
| `updater.rs` | Update checks, downloads, verification and installation |
| `whats_new.rs` | Release notes from `CHANGELOG.md`, compiled in |
| `tray.rs` | System tray icon (StatusNotifierItem via `ksni`) |
| `apps.rs`, `icons.rs` | Installed applications and icon lookup |
| `sys.rs` | Flatpak specifics: running host tools, host paths |
| `variant.rs` | Release build vs. local *LinuxLink Test* build |

## The engine

The UI owns the configuration. Whenever it changes, the UI sends the whole
configuration to the engine thread, which:

1. **Diffs the structure** (channels, mixes, routes, outputs) against the
   modules it has loaded and creates or removes only what changed.
2. **Reconciles levels**: pushes each send's effective volume and mute, and
   re-applies them if something else (for example WirePlumber restoring a
   saved volume) changed them.
3. **Refreshes** every 500 ms: lists devices and app streams for the UI, and
   routes newly started apps according to their rules.
4. **Saves** the configuration shortly after each change, atomically (written
   to a temporary file, then renamed), and again on exit.

On exit it unloads everything and restores the previous default output. If
LinuxLink crashes, the next start removes leftovers (any module whose arguments
contain `linuxlink_`).

## Updates

The updater asks the GitHub API for the latest release and compares versions.
What it downloads depends on how LinuxLink was installed (`InstallKind` in
`updater.rs`):

| Install | Detected by | Action |
| --- | --- | --- |
| AppImage | `$APPIMAGE` | Replace the AppImage file, restart |
| Tarball | binary outside `/usr` | Replace the binary, restart |
| Flatpak | `/.flatpak-info` | Download the `.flatpak`, open it |
| .deb / .rpm | `/etc/os-release` | Download the package, open it |
| Arch | `/etc/os-release` | Open the release page |

Every file is verified against the release's `SHA256SUMS` before use. Release
file names are part of this contract; see [releasing.md](releasing.md).

## Flatpak

LinuxLink's job is to reconfigure the session's audio, so the Flatpak runs
`pactl`, `parec`, `bluetoothctl` and `xdg-open` on the host through
`flatpak-spawn --host`, and reads host icons and app lists under
`/run/host`. That access makes the Flatpak effectively unsandboxed; it is
there as a convenient package format, not as a security boundary.
