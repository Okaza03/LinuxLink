# Installing, updating and removing

Every [release](https://github.com/Okaza03/LinuxLink/releases/latest) has
packages for **x86_64** (most PCs) and **ARM64** (aarch64). The Arch package is
x86_64 only. Not sure which to pick? Use the **AppImage**: it works on every
distribution and updates itself.

## Requirements

LinuxLink controls PipeWire through its PulseAudio layer, so it needs:

- **PipeWire** with **pipewire-pulse** and **WirePlumber**. That's the
  default on Fedora, Ubuntu 22.10+, Debian 12+, Arch, openSUSE Tumbleweed and
  most current distributions. Check with `pactl info`: it should say
  `Server Name: PulseAudio (on PipeWire …)`. Plain PulseAudio isn't supported.
- **`pactl` and `parec`**: package `pulseaudio-utils` (Debian, Ubuntu, Fedora,
  openSUSE) or `libpulse` (Arch). The .deb, .rpm and Arch packages pull these
  in for you.
- Optional: **`bluetoothctl`** (BlueZ) to show Bluetooth devices by name, and a
  desktop with a **system tray** (KDE Plasma, Cinnamon, XFCE, Budgie; GNOME with
  the AppIndicator extension).
- The packages are built on Ubuntu 22.04, so they need **glibc 2.35** or newer.

## AppImage (any distribution)

1. Download `LinuxLink-<version>-x86_64.AppImage`.
2. Make it executable: `chmod +x LinuxLink-*.AppImage` (or *Properties →
   Permissions → Allow executing* in your file manager).
3. Run it. LinuxLink adds itself to your app menu with its icon, so next time
   you can start it from there.

**Updates** install themselves: when the button in the lower-right corner says
*Restart to update*, click it. The AppImage file is replaced in place (and
renamed to the new version if it still has its original name).

**Remove it:** turn off *Settings → General → Show in app menu*, quit
LinuxLink, and delete the AppImage file. (Deleting the file alone also hides
the menu entry.)

> Ran an older AppImage before? If the taskbar shows LinuxLink without an
> icon, log out and back in once; the desktop had remembered "no icon".

## Flatpak (any distribution)

```sh
flatpak install --user LinuxLink-<version>-x86_64.flatpak
flatpak run io.github.Okaza03.LinuxLink
```

You can also open the file in Discover or GNOME Software. The bundle fetches
the Freedesktop runtime from Flathub if you don't have it yet.

**Updates:** the update button downloads the new `.flatpak` to ~/Downloads and
opens it in your software center. **Remove:**
`flatpak uninstall io.github.Okaza03.LinuxLink`.

LinuxLink's job is to reconfigure your whole session's audio, so the Flatpak
runs `pactl`/`parec` on your system through `flatpak-spawn`. It is a
convenient package, not a security sandbox.

## Debian, Ubuntu, Linux Mint, Pop!_OS

```sh
sudo apt install ./linuxlink_<version>_amd64.deb
```

**Updates:** the update button downloads the new .deb to ~/Downloads and opens
it in your software center (or install it with the same command).
**Remove:** `sudo apt remove linuxlink`.

## Fedora, openSUSE, RHEL

```sh
sudo dnf install ./linuxlink-<version>-1.x86_64.rpm      # Fedora, RHEL
sudo zypper install ./linuxlink-<version>-1.x86_64.rpm   # openSUSE
```

**Updates:** like the .deb. **Remove:** `sudo dnf remove linuxlink` /
`sudo zypper remove linuxlink`.

## Arch Linux, CachyOS, EndeavourOS, Manjaro

```sh
sudo pacman -U linuxlink-<version>-1-x86_64.pkg.tar.zst
```

Or build it from source with the [PKGBUILD](../packaging/arch/PKGBUILD)
(`makepkg -si` in that folder). **Updates:** the update button opens the
release page; install the new package the same way. **Remove:**
`sudo pacman -R linuxlink`.

## Tarball (anything else)

```sh
tar -xzf linuxlink-<version>-x86_64-linux.tar.gz
cd linuxlink-<version>-x86_64-linux
./install.sh                          # for your user, in ~/.local
sudo PREFIX=/usr/local ./install.sh   # or for everyone
```

A per-user install **updates itself** like the AppImage. **Remove:** run
`./uninstall.sh` (with the same `PREFIX`).

## From source

See [building.md](building.md). `cargo xtask install` installs your build as
**LinuxLink Test**, next to any released LinuxLink.

## Your settings

Uninstalling keeps your settings. To remove them too, delete
`~/.config/linuxlink/` (Flatpak: `~/.var/app/io.github.Okaza03.LinuxLink/`).
If LinuxLink was killed rather than quit, `linuxlink --cleanup` removes any
virtual devices it left behind.

## Verifying downloads

Each release has a `SHA256SUMS` file. In the folder with your download:

```sh
sha256sum -c SHA256SUMS --ignore-missing
```

The in-app updater does this check for you.
