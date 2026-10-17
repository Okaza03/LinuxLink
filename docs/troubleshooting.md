# Troubleshooting

Running `linuxlink` from a terminal prints its errors there, which helps with
everything below. Errors also appear as a red banner in the app.

## LinuxLink says "pactl: not found" or shows no devices

LinuxLink talks to PipeWire through `pactl` and `parec`:

| Distribution | Install |
| --- | --- |
| Debian, Ubuntu, Mint | `sudo apt install pulseaudio-utils` |
| Fedora | `sudo dnf install pulseaudio-utils` |
| openSUSE | `sudo zypper install pulseaudio-utils` |
| Arch and derivatives | `sudo pacman -S libpulse` |

Also check that PipeWire's PulseAudio layer runs: `pactl info` should print
`Server Name: PulseAudio (on PipeWire …)`. If it doesn't, install
`pipewire-pulse` and log out and back in.

## I hear nothing

- Check the **🎧 toggle**: at least one mix must be on for you to hear it.
- Check the **listening device** in the top-right corner.
- Check the channel's mute button and the mix's cell (red means muted).
- Is the app in the channel you think? Its icon appears on the channel; the
  channel's level meter moves when it plays.

## An app doesn't go to its channel

Some apps report a different name than their launcher. Open the channel,
search for the app while it is playing and tick the entry marked
**● playing**. If an app plays before LinuxLink starts, LinuxLink moves it as
soon as it starts.

## Crackling or dropouts

Raise **Settings → Audio → Latency** to 40 or 80 ms.

## LinuxLink devices are left over after a crash

LinuxLink cleans up on the next start. To clean up without starting it, run:

```sh
linuxlink --cleanup
```

## "LinuxLink is already running"

Only one LinuxLink (or LinuxLink Test) can run at a time, because they control
the same devices. Look for it in the taskbar or tray, or quit it there first.

## No tray icon

Your desktop needs StatusNotifierItem support. KDE Plasma, Cinnamon, XFCE and
Budgie have it; on GNOME install the
[AppIndicator extension](https://extensions.gnome.org/extension/615/appindicator-support/).
Without a tray, LinuxLink still minimizes to the taskbar.

## Bluetooth devices show an address instead of their name

Install BlueZ's command-line tools (`bluez` on Debian/Ubuntu/Fedora,
`bluez-utils` on Arch). LinuxLink asks `bluetoothctl` for the name.

## The Flatpak can't find my apps' icons

The Flatpak reads icons from your system through its filesystem permissions.
If you removed them (for example with Flatseal), icons fall back to the
channel's own icon.

## Updates fail

The update button shows the reason when you hover it. Common causes:

- No internet connection, or GitHub is unreachable: click the button to retry.
- An AppImage in a folder you can't write to (for example `/opt`): move the
  AppImage to your home folder, or update it by hand.

## Reset everything

Quit LinuxLink and delete `~/.config/linuxlink/` (Flatpak:
`~/.var/app/io.github.Okaza03.LinuxLink/`).

Still stuck? [Open an issue](https://github.com/Okaza03/LinuxLink/issues/new/choose).
