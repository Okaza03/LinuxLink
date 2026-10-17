#!/usr/bin/env python3
"""Writes a release's notes: the version's CHANGELOG.md section, then a
download guide for every file in the release folder.

Usage: release_notes.py VERSION DIST_DIR
"""

import os
import re
import sys

REPO = "Okaza03/LinuxLink"


def changelog_section(version: str) -> str:
    text = open("CHANGELOG.md", encoding="utf-8").read()
    match = re.search(rf"^## \[{re.escape(version)}\][^\n]*\n(.*?)(?=^## \[|^\[[^\]]+\]: |\Z)", text, re.S | re.M)
    if not match:
        sys.exit(f"CHANGELOG.md has no section for {version}")
    return match.group(1).strip()


def previous_version(version: str) -> str | None:
    text = open("CHANGELOG.md", encoding="utf-8").read()
    versions = re.findall(r"^## \[(\d+\.\d+\.\d+[^\]]*)\]", text, re.M)
    later = versions[versions.index(version) + 1 :] if version in versions else []
    return later[0] if later else None


def main() -> None:
    version, dist = sys.argv[1], sys.argv[2]
    files = set(os.listdir(dist))
    base = f"https://github.com/{REPO}/releases/download/v{version}"

    def link(name: str) -> str:
        return f"[`{name}`]({base}/{name})" if name in files else "—"

    rows = [
        ("**AppImage** (any distribution, updates itself)", "LinuxLink-{v}-x86_64.AppImage", "LinuxLink-{v}-aarch64.AppImage"),
        ("**Flatpak** (any distribution)", "LinuxLink-{v}-x86_64.flatpak", "LinuxLink-{v}-aarch64.flatpak"),
        ("**Debian, Ubuntu, Mint, Pop!_OS**", "linuxlink_{v}_amd64.deb", "linuxlink_{v}_arm64.deb"),
        ("**Fedora, openSUSE, RHEL**", "linuxlink-{v}-1.x86_64.rpm", "linuxlink-{v}-1.aarch64.rpm"),
        ("**Arch, CachyOS, EndeavourOS, Manjaro**", "linuxlink-{v}-1-x86_64.pkg.tar.zst", None),
        ("**Tarball** (any distribution, updates itself)", "linuxlink-{v}-x86_64-linux.tar.gz", "linuxlink-{v}-aarch64-linux.tar.gz"),
    ]
    out = [f"# LinuxLink {version}", "", changelog_section(version), "", "## Downloads", ""]
    out += ["| System | x86_64 (most PCs) | ARM64 |", "| --- | --- | --- |"]
    for label, x86, arm in rows:
        out.append(f"| {label} | {link(x86.format(v=version))} | {link(arm.format(v=version)) if arm else '—'} |")
    out += [
        "",
        "### How to install",
        "",
        "- **AppImage:** `chmod +x LinuxLink-*.AppImage`, then run it.",
        "- **Flatpak:** `flatpak install --user LinuxLink-*.flatpak`, or open the file in Discover / GNOME Software.",
        "- **Debian / Ubuntu:** `sudo apt install ./linuxlink_*.deb`",
        "- **Fedora:** `sudo dnf install ./linuxlink-*.rpm` (openSUSE: `sudo zypper install ./linuxlink-*.rpm`)",
        "- **Arch:** `sudo pacman -U linuxlink-*.pkg.tar.zst`",
        "- **Tarball:** unpack it and run `./install.sh`.",
        "",
        "Already have LinuxLink? Use the update button in the lower-right corner of the app.",
        "",
        "LinuxLink needs PipeWire with pipewire-pulse. See the "
        f"[user guide](https://github.com/{REPO}/blob/v{version}/docs/user-guide.md) to get started.",
        "",
        "### Verify your download",
        "",
        "```sh",
        "sha256sum -c SHA256SUMS --ignore-missing",
        "```",
    ]
    previous = previous_version(version)
    if previous:
        out += ["", f"**Full changelog:** https://github.com/{REPO}/compare/v{previous}...v{version}"]
    print("\n".join(out))


if __name__ == "__main__":
    main()
