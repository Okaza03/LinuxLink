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
| `version` | Format version (currently `3`). |
| `default_channel` | Channel id made the system default output while LinuxLink runs; unset = don't change it. |
| `listen_output` | Your listening device (a PipeWire sink name). |
| `listening` | Ids of the mixes with 🎧 on. |
| `hidden_inputs`, `hidden_outputs` | Devices switched off in Settings (hidden from LinuxLink only). |
| `latency_ms` | Buffer size of every connection, in milliseconds (10–80). |
| `close_to_tray` | `true`: closing the window keeps LinuxLink running in the tray. |
| `appimage_menu_entry` | AppImage only: keep LinuxLink in the app menu. |
| `last_seen_version` | Version whose *What's new* notes were shown last. |

## `[[channels]]`

| Key | Meaning |
| --- | --- |
| `id` | Internal id; the channel's device is `linuxlink_ch_<id>`. |
| `name`, `icon` | Shown in the app; `icon` is an emoji. |
| `kind` | `{ type = "virtual" }` for an app channel, or `{ type = "input", source = "<device>" }` for a microphone or other input. |
| `level` | Channel master: `volume` (0–100) and `muted`. For an input channel the device's own volume is used instead. |
| `sends.<mix id>` | The cell for that mix: `volume` and `muted`. A missing entry means the channel isn't in that mix. |

## `[[mixes]]`

| Key | Meaning |
| --- | --- |
| `id` | Internal id; the mix is `linuxlink_mix_<id>` and its virtual microphone `linuxlink_mixsrc_<id>`. |
| `name`, `icon` | Shown in the app; the virtual microphone is called "LinuxLink *name*". |
| `outputs` | Extra devices the mix always plays on (besides your listening device when 🎧 is on). |
| `level` | The mix's master volume and mute. |

## `[app_rules]` and `[known_apps]`

`app_rules` maps an app's key to the channel id it belongs in. `known_apps`
remembers each app's name and icon so channels can list apps that aren't
playing. Keys are `steam:<appid>` for Steam games, the Flatpak id for Flatpak
apps, or the program's name, as LinuxLink identifies them (see the user guide).

## Command line

| Option | Effect |
| --- | --- |
| `--version` | Print the version and exit. |
| `--cleanup` | Remove LinuxLink's virtual devices left by a crash, then exit. |
| `--minimized` | Start in the tray without opening the window (used by *Start with your computer*). |

Running `linuxlink` while it's already running brings the window back.
