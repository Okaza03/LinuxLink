# FAQ

**Is this Elgato Wave Link?**
No. LinuxLink is an independent, open-source app modelled on Wave Link 3's
routing matrix. It isn't affiliated with Elgato or Corsair, and you don't need
Elgato hardware.

**Does it work with plain PulseAudio?**
No, it needs PipeWire with pipewire-pulse, which current distributions use by
default. See [installing.md](installing.md#requirements).

**Does it process my audio (effects, noise suppression)?**
Not yet. LinuxLink routes and mixes; PipeWire does the work. Effects are a
possible future feature.

**How much delay does it add?**
About 20 ms per step by default (an app → its channel → a mix → your
headphones is two steps). Change it in *Settings → Audio → Latency*; lower is
snappier, higher is more robust against crackling.

**Will it mess up my audio settings?**
While running, LinuxLink makes its *System* channel your default output so
apps go through it, and adds virtual devices named "LinuxLink …". When it
quits, it removes them and leaves your system playing on your listening
device. If it was killed, the next start (or `linuxlink --cleanup`) removes
leftovers.

**Why do I have to keep it running?**
The channels and mixes only exist while LinuxLink runs. Closing the window
keeps it running in the system tray; quit from the tray icon or *Quit
LinuxLink* in the sidebar.

**Why does my microphone volume change by itself?**
Some apps reset the system microphone volume when audio devices come and go;
Discord's *Automatic Gain Control* is the usual one. LinuxLink restores your
input volume right after device changes, but turning off the app's automatic
gain stops it entirely.

**An app is in the wrong channel or has a strange name.**
Open the channel, search for the app and tick it. LinuxLink matches apps to
their menu entries; apps without one show the name their audio stream
reports. See [troubleshooting](troubleshooting.md).

**Can I use LinuxLink with EasyEffects or other audio tools?**
They should coexist, since both use ordinary PipeWire devices, but this isn't
tested. If you try it, an issue with your findings is welcome.

**Does LinuxLink send data anywhere?**
Only to check for updates: it asks GitHub for the latest release at startup
and every six hours, and downloads the update when you (or, for the AppImage
and tarball, it) install one. There is no telemetry.

**What's LinuxLink Test?**
A build of your own source checkout, installed with `cargo xtask install`. It
has its own menu entry, icon badge and settings, and never updates itself.
Only one of LinuxLink and LinuxLink Test can run at a time.

**Where are my settings?**
`~/.config/linuxlink/config.toml`; see [configuration.md](configuration.md).
