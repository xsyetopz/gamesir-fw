# JieLi GIP upgrade protocol and `gsfw` pad commands

A cross-platform (Windows, Linux, macOS) reimplementation of the protocol GameSir Nexus uses to
update JieLi-based pads over GIP (Xbox One transport), read from `JL_Upgrade_Gip.dll`. The goal is
to recover pads that a Nexus update left unusable, without Nexus.

Status: handshake (C0), query (C1) and CRC read (C5) work against a G7 SE on Windows 10 and on
macOS. `flash` implements the DLL's mode 1/2 path for a pad whose
region A (boot area) already holds the image's boot code: region C goes into the bank the pad is
not running, is verified, and only then the running bank's dir head is zeroed. It never writes
region A. On 2026-09-29 it wrote the `6.40` Key region C back into bank 1 of a G7 SE running
6.64 (`--keep-region-a`, verify CRC `0xd820`, C2 status 0).

## Use

Build with `cargo build --release -p gsfw`. The binary is `target/release/gsfw`. The USB
access is `nusb`, so no libusb is needed. `gsfw-gui` runs the same operations in a window.

```bash
gsfw info IMAGE.ufw          # entries, chip key, flash.bin directory
gsfw dry-run IMAGE.ufw       # the GIP messages probe would send
gsfw list                    # 3537 devices and their interfaces
gsfw probe IMAGE.ufw -v      # C0 + C1; prints the device flash directory
gsfw crc IMAGE.ufw 0x0 0x1000
gsfw flash IMAGE.ufw         # checks only: layout, region A, target bank
gsfw flash IMAGE.ufw --yes   # writes
```

The pad answers one C0 per power-up. After the first run, pass the printed session key with
`--session-key HEX`, or replug the pad.

`flash` stops when region A on the pad differs from the image's. `--keep-region-a` writes region C
anyway. For the G7 SE going from `6.64` back to `6.40`, region A differs only in `isd_config`'s `VLVD`
byte and CRCs.

`probe` needs the image made for the pad (the chip key comes from it, and a wrong image gets status -3).
`crc` also prints the CRC of the same `flash.bin` range, so a match shows the pad holds that image.

The pad must enumerate with an Xbox GIP interface (class `ff/47/d0`). `list` marks it.

- **Windows:** the pad boots into GIP there. In Zadig, pick the GIP interface and install WinUSB,
  which replaces the Xbox driver until you roll it back in Device Manager.
- **Linux:** the kernel's `xpad` driver is detached while the tool runs. Run as root or add a
  udev rule for `3537`. The pad may fall back to HID if the host doesn't answer it in time.
  `USB_QUIRK_DELAY_INIT` for the boot PID has helped with that.
- **macOS:** on `6.64` the pad falls back to HID (`3537:1082`), so there is no GIP interface to
  talk to. On `6.40` it enumerates as `3537:1010` with GIP interfaces. On 2026-09-29, `probe`
  and `crc` worked on macOS with a pad on `6.40`. No program may hold the interface: quit
  OpenJoystickDriver first. `gsfw` sets the configuration when macOS left the pad unconfigured.
  `flash` was not run on macOS.

## Protocol

These parts were read from the DLL (RVAs into `JL_Upgrade_Gip.dll`):

- **Packet:** 512 bytes. `JSUD` magic, `crc16(pkt[6:16])` at 4, u16 1 at 6, body CRC at 8, body
  length at 10, and a `u32` tag at 12 that the reply echoes. The body starts at `0x10` with the
  command byte. The whole packet is XORed with the JieLi ENC keystream (`0x24a0`, `0x1ab0`).
- **Handshake C0 (`0x27c0`):** sends `HJX1` and 16 random bytes, scrambled with key 0x1011. From the
  reply, `session key = crc16(host random) ^ crc16(reply[0x1c:0x2c])` (`0x4520`).
- **Commands:**

  | Command | RVA | Purpose |
  | --- | --- | --- |
  | C1 | 0x31f0 | Query with the chip key. The reply holds the device flash header and directory. |
  | C2 | 0x3ad0 | Result. |
  | C3 | 0x3740 | Erase. |
  | C4 | 0x3540 | Write, up to 256 bytes. |
  | C5 | 0x2e80 | CRC read. |
  | CA | 0x2b40 | Reset. |
  | CB | 0x3e10 | Range. |

- **Fragments (0x1f80):** each packet goes out as ten `4A 0A idx len data[52]` fragments, and each
  waits for a `4C 0A idx` ack. Replies arrive as `4A total idx len data`.
- **No flash read:** the protocol has no read command, so a backup is not possible this way.

Seen on the G7 SE:

- **Framing:** `0F 00 seq 3C` + `F0 00 00 00` + fragment works. Replies are GIP command 0x10 with
  `F1 00 00 00` + fragment, and each host fragment is acked with `4C 0A idx len`.
- **Reply key:** the C0 reply is scrambled with `0x5a5a`, not the tool id. The tool finds the key.
- **C5 reply type** is `4`. Like the DLL, the tool reads status and CRC and ignores the type.
- **Chip key:** C1 needs the `_Key` image's key. The `_No_Key` image gets status -3.

Still a guess: the ack the tool sends for pad messages flagged `0x10` (xpad layout).
