<div align="center">

<img src="packaging/linux/io.github.Okaza03.LinuxLink.svg" width="96" alt="LinuxLink icon">

# LinuxLink

**Mix and route your audio like Elgato Wave Link, on Linux.**

[![CI](https://github.com/Okaza03/LinuxLink/actions/workflows/ci.yml/badge.svg)](https://github.com/Okaza03/LinuxLink/actions/workflows/ci.yml)
[![Latest release](https://img.shields.io/github/v/release/Okaza03/LinuxLink?sort=semver)](https://github.com/Okaza03/LinuxLink/releases/latest)
[![License: MIT OR Apache-2.0](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue)](#license)

<img src="docs/images/mixes.png" alt="LinuxLink's routing matrix: channels as rows, mixes as columns" width="900">

</div>

LinuxLink splits your audio into **channels** (Music, Games, Voice Chat, your
microphone…) and lets you build independent **mixes** from them: one for your
headphones, another for your stream or recording, each with its own volume for
every channel. It is modelled on Elgato Wave Link 3 and built on PipeWire.

## Features

- **Routing matrix.** Channels are rows, mixes are columns, and every cell has
  its own fader and mute. Click **+** to route a channel into a mix.
- **App channels.** Put any apps in a channel (search through every installed
  app) and LinuxLink keeps them there, every time they start.
- **Up to five mixes.** Toggle 🎧 on any mix to hear it; turn on as many as you
  like. Every mix is also a **virtual microphone** for OBS, Discord or a recorder.
- **Microphones and inputs** as channels, with real device gain, mute and live
  meters.
- **Faders that feel right:** level meters in every track, ticks every 10%,
  snapping to 25/50/75%.
- **Stays out of the way.** Closing the window keeps LinuxLink running in the
  system tray; optionally start with your computer.
- **Bluetooth headsets just work:** connect one and your mixes move to it.
- **Apps by their real name and icon**, including Steam and Proton games.
- **Remembers everything** across restarts and reboots.
- **Updates itself** (AppImage, tarball) or hands verified packages to your
  system installer (.deb, .rpm, Flatpak), with a **What's new** window after
  every update.

## Install

Download the file for your system from the
[**latest release**](https://github.com/Okaza03/LinuxLink/releases/latest).
Every release has packages for **x86_64** and **ARM64 (aarch64)**.

| System | File | How to install |
| --- | --- | --- |
| **Any distribution** | `LinuxLink-<version>-x86_64.AppImage` | `chmod +x LinuxLink-*.AppImage` and run it. Updates itself. |
| **Any distribution** | `LinuxLink-<version>-x86_64.flatpak` | `flatpak install --user LinuxLink-*.flatpak` (or open it in Discover / GNOME Software) |
| **Debian, Ubuntu, Mint, Pop!_OS** | `linuxlink_<version>_amd64.deb` | `sudo apt install ./linuxlink_*.deb` |
| **Fedora, openSUSE, RHEL** | `linuxlink-<version>-1.x86_64.rpm` | `sudo dnf install ./linuxlink-*.rpm` (openSUSE: `sudo zypper install ./linuxlink-*.rpm`) |
| **Arch, CachyOS, EndeavourOS, Manjaro** | `linuxlink-<version>-1-x86_64.pkg.tar.zst` | `sudo pacman -U linuxlink-*.pkg.tar.zst`, or build [`packaging/arch/PKGBUILD`](packaging/arch/PKGBUILD) |
| **Anything else** | `linuxlink-<version>-x86_64-linux.tar.gz` | Unpack it and run `./install.sh`. Updates itself. |

On ARM64, pick the `aarch64` / `arm64` file instead. Details, updating and
uninstalling for each format: [docs/installing.md](docs/installing.md). Every release also has a
`SHA256SUMS` file: `sha256sum -c SHA256SUMS --ignore-missing`.

### Requirements

- **PipeWire** with **pipewire-pulse** (the default on Fedora, Ubuntu 22.10+,
  Debian 12+, Arch and most current distributions).
- `pactl` and `parec`: package `pulseaudio-utils` on Debian, Ubuntu, Fedora and
  openSUSE, `libpulse` on Arch. The .deb, .rpm and Arch packages install them
  for you.
- Optional: `bluetoothctl` (BlueZ) to show Bluetooth devices by name.
- The tray icon needs a desktop with StatusNotifierItem support (KDE Plasma,
  Cinnamon, XFCE, Budgie…; on GNOME install the *AppIndicator* extension).

## Getting started

1. Start **LinuxLink** from your app menu.
2. Click a **channel** (for example *Music*) and tick or search for the apps
   that belong in it.
3. In **My Mix**, set how loud each channel is in your headphones. Pick your
   headphones in the top-right corner.
4. For streaming or Discord, use the **Stream** mix and select
   **LinuxLink Stream** as the microphone / audio source in that app.

The [user guide](docs/user-guide.md) explains everything in detail.

## Documentation

- [User guide](docs/user-guide.md): channels, mixes, devices, settings, the tray
- [Setups](docs/setups.md): step-by-step recipes for streaming with OBS, Discord
  calls, recording and Bluetooth headsets
- [Installing, updating and removing](docs/installing.md), for every package format
- [FAQ](docs/faq.md) and [troubleshooting](docs/troubleshooting.md)
- [Configuration reference](docs/configuration.md): every setting and command-line option
- [How it works](docs/how-it-works.md): the PipeWire graph behind LinuxLink
- [Building from source](docs/building.md)
- [Releasing](docs/releasing.md)
- [Changelog](CHANGELOG.md)

## Contributing

Bug reports, ideas and pull requests are welcome. Please read
[CONTRIBUTING.md](CONTRIBUTING.md) and our [Code of Conduct](CODE_OF_CONDUCT.md).
For security problems, see [SECURITY.md](SECURITY.md). Need help? See
[SUPPORT.md](SUPPORT.md).

LinuxLink is an independent project and is not affiliated with or endorsed by
Elgato or Corsair.

## License

LinuxLink is licensed under either of

- [Apache License, Version 2.0](LICENSE-APACHE)
- [MIT License](LICENSE-MIT)

at your option. Unless you explicitly state otherwise, any contribution you
intentionally submit for inclusion in LinuxLink shall be dual licensed as above,
without any additional terms or conditions.
