# gamesir-fw

A community tool for GameSir controller firmware (JieLi AC695X chips). It is public domain.

With `gsfw` you can:

- decode GameSir firmware images (`.ufw` from GameSir Nexus, `.fw` from the per-model Flash Tools): entries, CRCs, chip key, decrypted app image.
- get a firmware download from the catalog, check its SHA-256, and write its images.
- carve the firmware images out of the GameSir Nexus app or a GameSir Flash Tool `.exe`.
- flash an image to a pad over the JieLi GIP upgrade protocol, without Nexus.

The flasher recovered a GameSir G7 SE that a Nexus update moved from 6.4.0 to 6.6.4, after which macOS no longer saw it as an Xbox pad. `gsfw` wrote 6.4.0 back, and the pad works on macOS again. `docs/jieli-gip-protocol.md` tells how.

This repository does not contain firmware. Get the images from GameSir's own downloads. The catalog (`crates/gsfw-core/catalog/catalog.json`) gives the URL and the SHA-256 of each GameSir Flash Tool that it knows. `gsfw catalog` lists them. The 6.40 G7 SE tool has no direct URL, so for that tool `fetch` needs `--from FILE`.

## Install

Rust 1.98 or later (`rust-toolchain.toml` pins it). On Linux the window needs the usual X11 or Wayland libraries.

```bash
cargo run --release -p gsfw -- --help   # command line
cargo run --release -p gsfw-gui         # window with the same operations
```

## Commands

```bash
gsfw catalog                 # the firmware downloads that fetch knows
gsfw fetch ARTIFACT OUT      # download, check the SHA-256, write the images
gsfw fetch ARTIFACT OUT --from FILE   # the same, with a file you downloaded
gsfw info IMAGE              # entries, CRCs, chip key, flash layout
gsfw extract IMAGE OUT       # all entries, decrypted
gsfw app-bin IMAGE OUT       # the decrypted app image
gsfw find-usb IMAGE DESCDIR  # the VID/PID and descriptor files inside the image
gsfw nexus-extract DLL OUT        # images from HJC.GameSir.Nexus2_0.dll
gsfw flash-tool-extract EXE OUT   # images from a GameSir Flash Tool .exe
gsfw dry-run IMAGE           # the GIP messages probe would send
gsfw list                    # connected GameSir pads and their USB interfaces
gsfw probe IMAGE             # read-only handshake with the pad
gsfw crc IMAGE ADDR LEN      # read-only CRC of a flash range
gsfw flash IMAGE             # checks only
gsfw flash IMAGE --yes       # writes the image
```

`gsfw-gui` offers the same operations. Before it writes flash, it asks for a confirmation.

The pad commands need the pad in Xbox (GIP) mode. On Windows, bind its GIP interface to WinUSB with Zadig first. See `docs/jieli-gip-protocol.md` for the details.

**Flashing can make a pad unusable. Only the G7 SE has been flashed so far.**

## Repository

- `crates/gsfw-core/`: formats, the JieLi GIP protocol, the flash use cases, the USB link, and the operations both front ends run.
- `crates/gsfw/`: the command line. `crates/gsfw-gui/`: the window (egui).
- `ARCHITECTURE.md`: the components, their layers and the design decisions.
- `docs/`: the upgrade protocol.
- `justfile`: the gate. Run `just check` before a change is done. It formats, then runs clippy, the tests, the docs, a release build, `tools/check_no_firmware.py` and cargo-deny.
- `tools/check_no_firmware.py`: fails when git would commit a firmware image or a file over 1 MB.
