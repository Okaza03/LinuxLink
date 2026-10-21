# Changelog

All notable changes to LinuxLink are listed here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and versions follow
[Semantic Versioning](https://semver.org/spec/v2.0.0.html).

Each section below also appears in the app's **What's new** window after an
update, and as the notes of the matching GitHub release.

## [Unreleased]

## [0.3.1] - 2026-10-21

Bluetooth headsets stay in stereo when you turn devices off.

### Fixed

- **Bluetooth headsets no longer drop to mono.** Turning devices off for
  the whole computer restarts the system's audio service, and a connected
  Bluetooth headset came back in its call mode: mono, much quieter, and on
  some headsets with weaker noise cancelling. LinuxLink now puts headsets
  back in high-quality stereo afterwards.

## [0.3.0] - 2026-10-21

Bluetooth headsets now play the moment they connect, and devices you never
use can be turned off for your whole computer.

### Added

- **Turn devices off for the whole computer.** In **Settings**, turn on
  **Turn them off on this computer too** and the devices you switch off
  disappear from your system's sound settings and other apps, not just from
  LinuxLink. Switch them back on in the same place. Applying a change
  restarts the system's audio service, so sound drops out for a moment.
  Only sound-card devices can be turned off this way; Bluetooth devices and
  devices LinuxLink is using stay hidden from LinuxLink only. Needs
  WirePlumber 0.5 or newer. `linuxlink --restore-devices` turns them all
  back on, and uninstalling does it for you.

### Fixed

- **Bluetooth headsets play right away.** After connecting a headset, or
  starting LinuxLink with one already connected, it often stayed silent
  until you picked another output and back, or even reconnected the
  headset. LinuxLink now restarts the headset once its stream is set up,
  so it plays first time, and recovers properly if it still fails.
- **Quieter quit.** LinuxLink mutes its mixes before taking them apart, so
  quitting doesn't send a burst through your headphones.

## [0.2.0] - 2026-10-18

LinuxLink now behaves like Wave Link 3 down to its numbers: faders, voice
ducking, channel boost, a compact view and a control API.

### Added

- **Voice ducking.** While your microphone (or any channel you choose) is
  loud, music and games go down in the mixes you pick, so people hear you.
  Sensitivity, amount (6, 12 or 24 dB) and speed use Wave Link's presets.
  Find it in **Settings → Voice ducking**.
- **Channel boost** of up to +12 dB in a channel's settings, for quiet apps
  and microphones.
- **Mix volume** row under the mix headers.
- **Reorder and hide.** Right-click a channel to move or hide it, or a mix
  header to move it.
- **Compact view:** one mix's faders in a small window.
- A **⚠** on a microphone's cell when you'd hear yourself in that mix.
- **Control API:** a local WebSocket API in the style of Wave Link's, for
  Stream Deck style controllers and scripts (Settings → Control API).
- Channel settings suggest the apps people usually put in a channel, and list
  apps playing now first, then ones played before.

### Changed

- **Looks like Wave Link 3:** its colours, fader, mix headers and spacing; a
  light theme (**Settings → General → Color theme**, which follows your
  desktop by default); Settings with tabs; a device page with the device on
  the left and its settings on the right; and a **Create channel** list of
  your inputs and apps to pick from. The update button only appears when
  there's an update; **Settings → Updates** always shows the status.
- **Faders work like Wave Link's:** linear in decibels over 60 dB, so the
  middle is −30 dB. Hover a fader to see its value. Your existing levels are
  converted and sound exactly as before.

### Fixed

- **Bluetooth headsets no longer stay silent after LinuxLink starts.** A
  headset that failed while LinuxLink was connecting to it ("Failure in
  Bluetooth audio transport") could end up stuck until you switched to another
  output and back. LinuxLink now also spots a headset that has quietly stopped
  playing, waits for it to restart before reconnecting, and retries with
  increasing pauses.

## [0.1.3] - 2026-10-17

Apps show their real names and icons, and the documentation grew a lot.

### Fixed

- **Apps show their real name and icon.** LinuxLink now matches every app
  playing audio to its menu entry, instead of trusting the audio stream's own
  name: Discord no longer shows as "WEBRTC VoiceEngine", Steam as "Chromium",
  or your desktop's notification sounds as "libcanberra" or a blank entry
  (they're now **System sounds**).
- **Games appear once, with their Steam name and icon**: "deadlock.exe"
  becomes **Deadlock**, and a game no longer shows up twice (with and without
  ".exe") when both Steam and Proton/Wine play its audio.
- The list of apps tidies itself once: names and icons are corrected, and
  leftovers from other audio tools are removed. Apps you assigned to a
  channel stay assigned.

### Documentation

- New guides: [installing, updating and removing](https://github.com/Okaza03/LinuxLink/blob/main/docs/installing.md)
  for every package format, [setups](https://github.com/Okaza03/LinuxLink/blob/main/docs/setups.md)
  with step-by-step recipes for OBS, Discord, recording and more, an
  [FAQ](https://github.com/Okaza03/LinuxLink/blob/main/docs/faq.md) and a
  [configuration reference](https://github.com/Okaza03/LinuxLink/blob/main/docs/configuration.md).
- The user guide explains how apps are recognized, Bluetooth headsets, and
  every mouse and keyboard action.

## [0.1.2] - 2026-10-17

Bluetooth headsets now just work, and the microphone volume stays put.

### Fixed

- **Bluetooth headsets just work.** A headset you connect becomes your
  listening device automatically, like on a phone. If it disconnects,
  LinuxLink carries on with the most recently connected headset, else your
  previous output, else any other output, instead of going silent.
- **No more silence after a Bluetooth hiccup.** Bluetooth outputs sometimes
  fail when audio starts ("Failure in Bluetooth audio transport") and then
  stay silent; LinuxLink now notices and reconnects them by itself.
- An automatic switch (for example to a headset you just connected) could be
  undone by a settings change made at the same moment; it now sticks.
- Your microphone volume is also protected when a headset or other device
  connects or disconnects, not only when LinuxLink itself changes devices.
- LinuxLink now rebuilds its connection to a device that went away and came
  back, instead of staying silent until you picked another device and back.
- LinuxLink no longer switches your listening device at startup just because
  the system's default output was different (this picked the wrong device,
  e.g. the headphone jack instead of your AirPods, after logging in).

## [0.1.1] - 2026-10-17

Fixes for sound, the microphone volume and the AppImage.

### Fixed

- **No more silence after switching outputs.** Your listening device now
  follows your choices: pick another output in your desktop's volume
  controls while LinuxLink runs and your mixes move there; switch outputs
  while LinuxLink is closed and it listens there on the next start. If your
  listening device is disconnected, LinuxLink plays on the system's output
  until it's back, and when LinuxLink quits it leaves the system playing on
  the device you were listening on.
- **Your microphone volume no longer jumps** when LinuxLink starts or quits.
  Some apps (Discord's automatic gain control, for one) reset the system
  microphone volume whenever audio devices appear or disappear; LinuxLink now
  puts your input devices back to their previous volume right after its own
  device changes, while leaving your own changes alone.
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

[Unreleased]: https://github.com/Okaza03/LinuxLink/compare/v0.3.0...HEAD
[0.3.0]: https://github.com/Okaza03/LinuxLink/compare/v0.2.0...v0.3.0
[0.2.0]: https://github.com/Okaza03/LinuxLink/compare/v0.1.3...v0.2.0
[0.1.3]: https://github.com/Okaza03/LinuxLink/compare/v0.1.2...v0.1.3
[0.1.2]: https://github.com/Okaza03/LinuxLink/compare/v0.1.1...v0.1.2
[0.1.1]: https://github.com/Okaza03/LinuxLink/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/Okaza03/LinuxLink/releases/tag/v0.1.0
