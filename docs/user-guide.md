# User guide

LinuxLink is built around two ideas: **channels** and **mixes**.

- A **channel** is a source of sound: a group of apps (Music, Games, Voice
  Chat…) or an input device such as your microphone.
- A **mix** is a combination of channels, each at its own volume. You might
  hear one mix in your headphones while your stream or recording gets another.

The main screen, **Mixes**, is a matrix: channels are rows, mixes are columns.

![The Mixes page](images/mixes.png)

## Channels

The first column of each row is the channel itself:

- Its **icon** shows the apps in it. Click the icon or name to open the
  channel's settings.
- The **speaker button** mutes the whole channel.
- The **fader** is the channel's master volume. It applies on top of every
  mix. For an input channel (your microphone), it is the device's real volume.

### Putting apps in a channel

1. Click the channel's name.
2. Under **Apps in this channel**, tick the apps you want. Use the **search
   box** to find any installed application, even one that has never played
   sound.
3. Click **Done**.

LinuxLink moves the app there right away, and every time it starts from then
on. Apps that aren't in any channel play through the channel chosen in
**Settings → General → System default output** (System, by default).

An app can only be in one channel; ticking it in another channel moves it.

### Creating, renaming and deleting channels

- **Create channel** (below the last row) makes a new app channel, or an input
  channel for a microphone or other input device.
- In a channel's settings you can rename it, pick an icon, choose the input
  device (for input channels) or **Delete channel**.

## Mixes

Each column is a mix. The header shows its name and icon, its level meter, and
on the right the **🎧 listen toggle**.

- **🎧 on (blue):** you hear this mix on your listening device.
- **🎧 off (crossed out):** you don't hear it, but it keeps working for
  anything recording it.

Any number of mixes can be on at once. Choose your **listening device**
(headphones or speakers) in the top-right corner. You can also just pick an
output in your desktop's volume controls: LinuxLink follows, moving your mixes
there while apps keep playing through your channels. If the listening device
is disconnected, LinuxLink switches to the most recently connected Bluetooth
headset, else your previous output. A Bluetooth headset you connect becomes
your listening device automatically.

### The cells

Each cell is one channel's volume *in that mix*:

- Drag the fader to set the volume. Faders snap to 25%, 50% and 75%.
- Click the speaker to mute the channel in this mix only. Muted cells turn red.
- An empty cell means the channel isn't in that mix. Click **+** to add it;
  right-click a cell and choose **Remove from this mix** to take it out.

### Mix settings

Click a mix's header to rename it, choose an icon, turn listening on or off, or
send it to **extra outputs** (for example a capture card or a second headset).
Each output device plays at most one mix. You can have up to five mixes; add
one with the **+** next to the last header.

## Using a mix in OBS, Discord and other apps

Every mix is also a virtual microphone named **LinuxLink *mix name***, for
example *LinuxLink Stream*.

- **OBS:** add an *Audio Input Capture* source and choose *LinuxLink Stream*.
- **Discord:** *User Settings → Voice & Video → Input Device → LinuxLink Stream*.
- **Recording apps:** pick the mix as the input device.

A common setup: your microphone, game and music in **Stream**, your voice chat
and everything else in **My Mix**, with 🎧 on for My Mix only.

## Your devices

The sidebar lists your input devices. Click one to set its gain, mute it, watch
its level, or **Add as channel**.

## Settings

- **Listening:** your listening device and which mixes you hear.
- **General:** which channel apps play through by default, whether closing the
  window keeps LinuxLink running, and whether it starts with your computer.
- **Input devices / Output devices:** switch off devices you never use to hide
  them from LinuxLink. Other apps can still use them.
- **Audio → Latency:** lower is snappier; raise it if you hear crackling.
- **About:** version, **What's new**, and links to report a problem.

## Closing, the tray and starting with your computer

LinuxLink has to keep running for your channels and mixes to work. Closing the
window therefore **minimizes** it to the taskbar, and a LinuxLink icon appears
in the system tray. Click the tray icon to bring the window back, or right-click
it and choose **Quit**. You can also quit with **Quit LinuxLink** at the bottom
of the sidebar.

When LinuxLink quits, it removes all of its virtual devices and puts your
previous default output back.

To quit when the window is closed instead, turn off **Settings → General →
Keep running when closed**.

## Updates

The button in the lower-right corner shows whether you're **Up to date**.
When a new version is out it changes to **Download new update**:

| Installed as | What happens |
| --- | --- |
| AppImage, tarball | Downloads and installs in the background; click **Restart to update** |
| .deb, .rpm, Flatpak | Downloads the package to ~/Downloads and opens your system installer |
| Arch package | Opens the release page; update with pacman or your AUR helper |

Every download is checked against the release's SHA256SUMS before it's used.
After an update, a **What's new** window lists the changes:

![The What's new window](images/whats-new.png)

## Your settings

Everything you set up is saved automatically in
`~/.config/linuxlink/config.toml` (Flatpak:
`~/.var/app/io.github.Okaza03.LinuxLink/config/linuxlink/config.toml`) and
restored when LinuxLink starts again, including after a reboot.

To start over, quit LinuxLink and delete that file.
