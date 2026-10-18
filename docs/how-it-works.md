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

- **A cell's fader** is the volume of its send loopback: channel master + cell
  fader + boost − ducking, in dB. Two mixes can hold the same channel at
  different volumes because each has its own loopback.
- **Faders follow Wave Link 3's law** (`levels.rs`): linear in dB over 60 dB.
  PipeWire's pulse volumes are cubic, so LinuxLink sets raw volumes
  (`65536 × 10^(dB/60)`), which hit every dB value exactly.
- **An input channel's master** is the hardware source's own volume, so other
  apps see the same level.
- **A mix's master** is the volume of the mix sink.

Apps are routed by moving their streams (`pactl move-sink-input`) into a
channel sink. Each stream is first identified (`apps::AppIndex::identify`):
streams often carry a library's name, so LinuxLink matches the stream's Flatpak
id, application name and executable against installed menu entries (desktop
id, `Exec`, `StartupWMClass`, `Name`; Steam games via `steam://rungameid/`).
Rules are stored per identity and also matched against the stream's raw
binary name, application name and Flatpak id.

## Voice ducking

The engine measures each trigger channel with a low-rate `parec` capture and
ramps a reduction towards the chosen amount while the loudest one is above
the sensitivity threshold, and back to zero when it isn't, linearly in dB
over the attack and release times. The reduction (in half-dB steps) is
subtracted from the ducked channels' sends in the chosen mixes. While ducking
is set up, the engine runs every 20 ms instead of every 100 ms.

## Control API

`api.rs` serves JSON-RPC 2.0 over a WebSocket on 127.0.0.1 (see
[configuration.md](configuration.md#control-api)). Changes go into the
settings the window shares, then to the engine as a normal settings update;
the window adopts them on its next frame. A watcher thread compares what
clients see every 150 ms and sends `*Changed` notifications. Origins other
than none, `file://` and `streamdeck://` (that is, web pages) are refused, and
so are Host headers other than loopback, against DNS rebinding.

## Devices coming and going

Every refresh the engine compares the device list with the last one:

- A **Bluetooth output that just appeared** becomes the listening device.
- If the **listening device disappeared**, it falls back to the most recently
  connected Bluetooth output, the previous system output, then any output.
- Loopbacks that PipeWire removed with their device are rebuilt.
- Bluetooth outputs LinuxLink feeds are checked every two seconds
  (`pw-cli info`); one stuck in the `error` state is suspended and reconnected.
- After any device change (and after LinuxLink's own changes) input-channel
  volumes are guarded for three seconds, because apps such as Discord reset
  the system microphone volume when devices change.
- If you pick another output in the desktop's volume controls, LinuxLink takes
  that as your new listening device and makes its System channel the default
  again.

## Code map

All code lives in `apps/LinuxLink/src/`:

| File | Responsibility |
| --- | --- |
| `main.rs` | Startup, single-instance lock, signal handling, restart after update |
| `engine.rs` | Builds and reconciles the PipeWire graph on a worker thread |
| `pactl.rs` | Thin wrapper around `pactl` (JSON output) |
| `meter.rs` | Peak meters, one low-rate `parec` capture per source |
| `levels.rs` | Wave Link's fader law, dB maths and ducking presets |
| `api.rs` | The local WebSocket control API |
| `config.rs` | Settings model, load/save (`~/.config/linuxlink/config.toml`) |
| `ui.rs` | The egui interface |
| `updater.rs` | Update checks, downloads, verification and installation |
| `whats_new.rs` | Release notes from `CHANGELOG.md`, compiled in |
| `tray.rs` | System tray icon (StatusNotifierItem via `ksni`) |
| `apps.rs` | Installed applications, and identifying the app behind each audio stream |
| `icons.rs` | Icon lookup in the icon themes |
| `instance.rs` | One instance per session; a second launch shows the running window |
| `integrate.rs` | The AppImage's own app-menu entry and icons |
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
