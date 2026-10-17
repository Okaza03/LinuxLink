# Changelog

All notable changes to LinuxLink are listed here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and versions follow
[Semantic Versioning](https://semver.org/spec/v2.0.0.html).

Each section below also appears in the app's **What's new** window after an
update, and as the notes of the matching GitHub release.

## [Unreleased]

### Fixed

- **The AppImage now appears in your app menu** and its window shows the
  LinuxLink icon in the taskbar. It adds a menu entry on start (turn this off
  in **Settings → General → Show in app menu**), keeps it pointing at the
  AppImage if you move it, and the entry disappears if you delete the file.
- Updating an AppImage that still has its download name renames it to the new
  version, instead of leaving the old version number in the file name.

## [0.1.0] - 2026-10-17

The first release of LinuxLink: a Wave Link 3 style audio mixer for Linux,
built on PipeWire.

### Mixing

- **Routing matrix.** Channels are rows and mixes are columns. Every cell is
  that channel's send into that mix, with its own fader and mute button, so
  each mix is completely independent. Muted cells turn red.
- **Route or unroute with one click.** Click **+** on an empty cell to send a
  channel into a mix; right-click a cell and pick **Remove from this mix** to
  take it out again.
- **Channel master faders.** Each channel has a master fader and mute that
  apply on top of all of its sends.
- **Live level meters** inside every fader track, under each mix header and on
  the device pages.
- **Fader tick marks** every 10%, and faders **snap to 25%, 50% and 75%** when
  you drag within 2% of them.

### Channels

- Seven channels out of the box: Microphone, System, Browser, Voice Chat,
  Music, Games and Sound Effects. Add, rename, re-icon and delete channels.
- **App channels** are virtual outputs. Assign any number of apps to one and
  LinuxLink moves them there now and every time they start.
- **Search for apps** when assigning them: the channel window searches apps
  that have played audio *and* every installed application, so you can set up
  Discord before it ever makes a sound. Flatpak apps are matched by their app
  ID.
- Channels show the real icons of the apps inside them, from your icon theme.
- **Input channels** (microphones and other inputs): the channel's master
  fader and mute control the device's real volume, so other apps and the
  **Your devices** page always agree with it.

### Mixes

- Two mixes out of the box, **My Mix** and **Stream**, and up to five in
  total. Rename them and pick an icon.
- **🎧 Listen toggle** on every mix header: turn it on to hear that mix on your
  listening device. Any number of mixes can be on at once.
- **Every mix is a virtual microphone** named "LinuxLink *mix name*", ready to
  pick as the audio source in OBS, Discord or a recorder.
- Send a mix to **extra outputs** as well, for example a capture card or a
  second headset. Each device plays at most one mix.

### Devices

- **Your devices** sidebar with a page per input device: input gain, mute, a
  live meter and **Add as channel**.
- Pick your **listening device** from the top-right corner.
- Bluetooth devices show their real names (for example "AirPods Pro"), even
  when PipeWire doesn't pass the name on.

### Settings

- Collapsible sections for listening, general options, input devices, output
  devices, audio and about.
- **Turn devices off** to hide the ones you never use from LinuxLink. They keep
  working in other apps.
- Choose which channel is the **system default output** while LinuxLink runs.
- **Start with your computer.**
- **Latency** from 10 ms (lowest) to 80 ms (most stable).

### Updates

- The **update button** in the lower-right corner shows **Up to date** or
  **Download new update**.
- **AppImage** and **tarball** installs download, verify and install updates by
  themselves; restart to finish.
- **.deb, .rpm and Flatpak** installs download the verified package to
  ~/Downloads and open it in your system's installer.
- Every download is checked against the release's SHA256SUMS.
- This **What's new** window after each update.

### Reliability

- LinuxLink removes all of its virtual devices when it closes, including on
  Ctrl+C or logout, and cleans up after a crash on the next start.
- Your previous default output comes back when LinuxLink closes.
- Volumes stay in sync even if WirePlumber or another mixer app changes them.
- Only one LinuxLink runs at a time, so two copies can't fight over your audio.

### Packages

- Packages for every major distribution, on x86_64 and ARM64: AppImage,
  Flatpak, .deb (Debian, Ubuntu, Mint, Pop!_OS), .rpm (Fedora, openSUSE), an
  Arch Linux package, and a plain tarball.

[Unreleased]: https://github.com/Okaza03/LinuxLink/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/Okaza03/LinuxLink/releases/tag/v0.1.0
