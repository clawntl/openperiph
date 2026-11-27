# Linux

OpenPeriph runs on x86-64 Linux with glibc 2.35 or newer, under X11 or Wayland.

## Arch Linux and CachyOS

Install the package from a [release](https://github.com/clawntl/openperiph/releases/latest),
after a full system upgrade:

```
sudo pacman -Syu
sudo pacman -U openperiph-*-x86_64.pkg.tar.zst
```

Or build it from this repository. This uses only official packages:

```
sudo pacman -Syu
sudo pacman -S --needed base-devel git rust
git clone https://github.com/clawntl/openperiph
cd openperiph/packaging/arch
makepkg -si
```

The package installs `openperiph`, a menu entry, and udev rules for every supported device
(including the SK620's bootloader), so nothing needs root. OpenPeriph updates the package itself
when a new release comes out. Remove it with `sudo pacman -R openperiph`.

## Other distributions

Download `openperiph-linux-x64` from a release, make it executable (`chmod +x`) and run it. If
you keep it in a folder you can write to, such as `~/.local/bin`, OpenPeriph can update itself.

## Device access (udev)

Linux only lets root open HID devices until a udev rule allows it. When a device needs a rule,
OpenPeriph shows **Allow access**. This adds a rule for that device to
`/etc/udev/rules.d/61-openperiph-local.rules`, asking for your password through `pkexec`. Unplug
the device and plug it back in afterwards.

To do it by hand instead, add a line like this for each device, then run
`sudo udevadm control --reload && sudo udevadm trigger`:

```
KERNEL=="hidraw*", ATTRS{idVendor}=="feed", ATTRS{idProduct}=="0620", MODE="0660", TAG+="uaccess"
```

The USB ids are on each device's page under [devices](devices/).

## File dialogs

Importing definitions, backups and pictures goes through the desktop's XDG portal. KDE, GNOME and
most desktops provide it (`xdg-desktop-portal`).
