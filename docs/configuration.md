# Configuration reference

LinuxLink saves everything automatically; you never need to edit this file.
It's documented for troubleshooting, backups and scripting.

| Build | File |
| --- | --- |
| LinuxLink | `~/.config/linuxlink/config.toml` |
| Flatpak | `~/.var/app/io.github.Okaza03.LinuxLink/config/linuxlink/config.toml` |
| LinuxLink Test | `~/.config/linuxlink-test/config.toml` |

Quit LinuxLink before editing it by hand, or it will overwrite your changes.
Saves are atomic (written to a temporary file, then renamed), so a crash can't
leave a half-written file. If the file can't be read, LinuxLink starts with
defaults and keeps the old file as `config.toml.bak`.

## Top level

| Key | Meaning |
| --- | --- |
| `version` | Format version (currently `4`). Version 3 files are converted on start, keeping every level sounding the same; the original is kept as `config.toml.v3.bak`. |
| `default_channel` | Channel id made the system default output while LinuxLink runs; unset = don't change it. |
| `listen_output` | Your listening device (a PipeWire sink name). |
| `listening` | Ids of the mixes with 🎧 on. |
| `hidden_inputs`, `hidden_outputs` | Devices switched off in Settings (hidden from LinuxLink only). |
| `latency_ms` | Buffer size of every connection, in milliseconds (10–80). |
| `close_to_tray` | `true`: closing the window keeps LinuxLink running in the tray. |
| `appimage_menu_entry` | AppImage only: keep LinuxLink in the app menu. |
| `last_seen_version` | Version whose *What's new* notes were shown last. |
| `window_mode` | `"full"` or `"compact"`. |
| `theme` | `"system"` (follow the desktop), `"light"` or `"dark"`. |
| `compact_mix` | Id of the mix the compact view shows. |
| `api_enabled` | `true`: serve the [control API](#control-api). |

## `[[channels]]`

| Key | Meaning |
| --- | --- |
| `id` | Internal id; the channel's device is `linuxlink_ch_<id>`. |
| `name`, `icon` | Shown in the app; `icon` is an emoji. |
| `kind` | `{ type = "virtual" }` for an app channel, or `{ type = "input", source = "<device>" }` for a microphone or other input. |
| `level` | Channel master: `volume` and `muted`. For an input channel the device's own volume is used instead. |
| `sends.<mix id>` | The cell for that mix: `volume` and `muted`. A missing entry means the channel isn't in that mix. |
| `boost` | `enabled` and `db` (0–12): extra gain on every send. |
| `hidden` | `true`: hidden from the Mixes page (its audio keeps flowing). |

Every `volume` is a fader position from 0 to 100, linear in decibels like
Wave Link's: `dB = 0.6 × volume − 60`, so 100 is 0 dB, 50 is −30 dB and 0 is
silence.

## `[[mixes]]`

| Key | Meaning |
| --- | --- |
| `id` | Internal id; the mix is `linuxlink_mix_<id>` and its virtual microphone `linuxlink_mixsrc_<id>`. |
| `name`, `icon` | Shown in the app; the virtual microphone is called "LinuxLink *name*". |
| `outputs` | Extra devices the mix always plays on (besides your listening device when 🎧 is on). |
| `level` | The mix's master volume and mute. |

## `[ducking]`

| Key | Meaning |
| --- | --- |
| `keys` | Ids of the channels that trigger ducking. |
| `ducked.<channel id>` | Ids of the mixes that channel is lowered in. |
| `sensitivity` | `"low"`, `"medium"` or `"high"`: trigger level −3, −12 or −24 dBFS. |
| `amount` | `"low"`, `"medium"` or `"high"`: lowered by 6, 12 or 24 dB. |
| `speed` | `"slow"`, `"medium"` or `"fast"`: attack/release 100/2000, 80/1200 or 30/200 ms. |

## `[app_rules]` and `[known_apps]`

`app_rules` maps an app's key to the channel id it belongs in. `known_apps`
remembers each app's name and icon so channels can list apps that aren't
playing. Keys are `steam:<appid>` for Steam games, the Flatpak id for Flatpak
apps, or the program's name, as LinuxLink identifies them (see the user guide).

## Control API

While `api_enabled` is on, LinuxLink serves JSON-RPC 2.0 over a WebSocket on
`127.0.0.1`, on a port the system picks, and writes it to `ws-info.json` next
to `config.toml` (`{"port": 41234}`). The protocol follows Wave Link 3's, so
its method names will be familiar:

| Method | Params | Does |
| --- | --- | --- |
| `getApplicationInfo` | | Name, version, `interfaceRevision` |
| `getChannels` | | Channels with `level`, `isMuted`, `type`, their `mixes` and `apps` |
| `getMixes` | | Mixes with `level` and `isMuted` |
| `getInputDevices`, `getOutputDevices` | | Devices; `mainOutput` is your listening device |
| `setChannel` | `id`, `level`?, `isMuted`?, `mixes`?: `[{id, level?, isMuted?}]` | Sets a channel's master or its cells (adding it to a mix if needed) |
| `setMix` | `id`, `level`?, `isMuted`? | Sets a mix's volume or mute |
| `setInputDevice` | `id`, `isMuted`?, `gain`?: `{value}` | Sets a device's mute or volume (percent) |
| `setOutputDevice` | `mainOutput`: `{outputDeviceId}` or `outputDevice`: `{id, outputs: [{mixId}]}` | Picks your listening device, or plays a mix on an output |
| `addToChannel` | `appId`, `channelId` | Always plays that app in that channel |
| `getWindowMode`, `setWindowMode` | `windowMode`: `fullSized` / `compacted`, `settings`?: `{mixId}` | The window's view |
| `setSubscription` | `levelMeterChanged`: `{type, id, isEnabled}` | Live meters for a `channel`, `mix`, `input` or `output` |

Levels are fader positions from 0 to 1. After any change LinuxLink sends
`channelsChanged`, `mixesChanged`, `inputDevicesChanged`,
`outputDevicesChanged` or `windowModeChanged` with the new state, and
`levelMeterChanged` about 20 times a second for subscribed meters. Only
programs on this computer can connect; connections from web pages are
refused.

## Command line

| Option | Effect |
| --- | --- |
| `--version` | Print the version and exit. |
| `--cleanup` | Remove LinuxLink's virtual devices left by a crash, then exit. |
| `--minimized` | Start in the tray without opening the window (used by *Start with your computer*). |

Running `linuxlink` while it's already running brings the window back.
