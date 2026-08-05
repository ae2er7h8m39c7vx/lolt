## Install

Download the archive for your platform, unpack it, and put `lolt` on your PATH:

```sh
tar xzf lolt-<platform>.tar.gz
sudo mv lolt-<platform>/lolt /usr/local/bin/
lolt            # T1 in the LCK
```

Checksums are in the matching `.sha256` files.

macOS builds are universal (Apple silicon + Intel) and unsigned — if Gatekeeper
objects, clear the quarantine flag with `xattr -d com.apple.quarantine lolt`.

The macOS widget is not distributed here; build it from source with Xcode, see
[macos/README.md](macos/README.md).
