# Building from source

## Requirements

- Rust 1.95 or newer (`rustup update stable`)
- A C toolchain and the usual graphics/windowing headers
- PipeWire with `pipewire-pulse`, plus `pactl` and `parec` to run it

| Distribution | Build dependencies |
| --- | --- |
| Debian, Ubuntu | `sudo apt install build-essential pkg-config libxkbcommon-dev libwayland-dev libx11-dev libgl1-mesa-dev pulseaudio-utils` |
| Fedora | `sudo dnf install gcc pkgconf-pkg-config libxkbcommon-devel wayland-devel libX11-devel mesa-libGL-devel pulseaudio-utils` |
| Arch | `sudo pacman -S --needed base-devel libxkbcommon wayland libx11 libpulse` |

## Build and run

```sh
git clone https://github.com/Okaza03/LinuxLink.git
cd LinuxLink
cargo run --release -p linuxlink
```

## LinuxLink Test

To use your own build day to day without mixing it up with an installed
release, install it as **LinuxLink Test**:

```sh
cargo xtask install              # build and install LinuxLink Test for your user
cargo xtask install --autostart  # ... and start it when you log in
cargo xtask uninstall            # remove it again
```

LinuxLink Test has its own menu entry and icon (with a TEST badge), its own
command (`linuxlink-test`) and its own settings folder
(`~/.config/linuxlink-test`, seeded once from your real settings). It never
updates itself. Only one of LinuxLink and LinuxLink Test can run at a time.

## Checks

These are what CI runs on every push and pull request:

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## Packages

`cargo xtask dist` builds the release binary, the tarball and an AppImage
folder in `target/dist/`. The other formats use standard tools:

```sh
cargo xtask dist
appimagetool target/dist/AppDir LinuxLink.AppImage            # AppImage
cargo deb -p linuxlink --no-build                              # .deb  (cargo install cargo-deb)
cargo generate-rpm -p apps/LinuxLink                           # .rpm  (cargo install cargo-generate-rpm)
flatpak-builder --user --install-deps-from=flathub --force-clean \
  --repo=target/flatpak-repo target/flatpak-build \
  packaging/flatpak/io.github.Okaza03.LinuxLink.yml            # Flatpak
cd packaging/arch && makepkg -si                               # Arch (builds the tagged release)
```

The release workflow (`.github/workflows/release.yml`) does all of this for
x86_64 and ARM64.
