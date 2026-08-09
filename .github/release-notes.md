## macOS app and widget

`Lolt-<version>-macos-universal.dmg` contains the app and its Notification
Center widget, universal for Apple silicon and Intel. Open the disk image, drag
**Lolt** to Applications, and launch it once — then add the widget from
Notification Center → **Edit Widgets**.

If the build is unsigned, macOS will refuse the first launch. Right-click the
app → **Open**, or clear the quarantine flag:

```sh
xattr -dr com.apple.quarantine /Applications/Lolt.app
```

## Command line

Download the archive for your platform, unpack it, and put `lolt` on your PATH:

```sh
tar xzf lolt-<platform>.tar.gz
sudo mv lolt-<platform>/lolt /usr/local/bin/
lolt            # T1 in the LCK
```

Checksums are in the matching `.sha256` files.

macOS builds are universal (Apple silicon + Intel) and unsigned — if Gatekeeper
objects, clear the quarantine flag with `xattr -d com.apple.quarantine lolt`.

`--league` and `--team` complete against the live league and team lists; enable
it with `source <(COMPLETE=zsh lolt)` in your shell rc (`bash` and `fish` work
the same way).
