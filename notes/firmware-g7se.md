# G7 SE firmware 6.6.4: data-only map (T10)

Image: `private/re/ufw/g7se-v664-nokey.bin`, the decrypted app.bin (0x37570 bytes, CRC 0x743e;
`gsfw app-bin`). Offsets are file offsets in that image. T9 found no disassembler
that decodes the image cleanly, so everything below comes from byte patterns and strings, not code.
The source path in the image (0x1df18) names the SDK as `SL3100_V664 ... apps/dongle/board/br23/board_ac695x_demo.c`.

## USB device descriptors (18 bytes, `12 01`)

| Offset | bcdUSB | Class | VID:PID | bcdDevice | Notes |
| --- | --- | --- | --- | --- | --- |
| 0x1a26d | 0x0111 | 00/00/00 | 3537:1082 | 0x0664 | Default HID mode (T8 hit at 0x1a275). |
| 0x18b85 | 0x0200 | 00/00/00 | 3537:1010 | 0x0664 | HID-class 1010, the mode macOS saw after the combo. |
| 0x1a291 | 0x0200 | ff/ff/ff | 3537:1010 | 0x0664 | Vendor-class 1010, next to the GIP descriptors; which mode uses it is unknown. |
| 0x18bd4 | 0x0200 | 00/00/00 | 3537:10a0 | 0x0664 | XInput PID (Nexus lists 10A0 as XInput). |
| 0x1a2a3 | 0x0200 | ff/ff/ff | 3537:0158 | 0x0664 | Not identified. |
| 0x1a27f | 0x0200 | 00/00/00 | 2c16:c001 | 0x0664 | Not identified. |
| 0x1a37b | 0x0200 | 00/00/00 | 0f0d:01ab | 0x0664 | 0f0d is HORI's VID (guess: a console-compatibility mode). |

PID 1022 is not in the image: searched the little-endian `37 35 22 10`, and every `12 01`
device descriptor is listed above. The XInput mode here enumerates as 10A0, not 1022.

MS OS string descriptors (`12 03` "MSFT100", vendor code 0x90) sit at 0x1a2b5 and 0x1a38d.

## Configuration and interface descriptors

The 73-byte configuration descriptor in `private/descriptors/g7se-1082-config.bin` is not stored
whole. Its `09 02` header and the HID descriptors' report lengths are filled in at run time
(the stored `09 21` records carry length 00 00). The interface and endpoint records are stored as
templates:

| Offset | Content |
| --- | --- |
| 0x1bb7d | if0 HID `09 04 00 00 02 03 00 00 00`, HID `09 21 10 01`, EP 0x82 IN int 1, EP 0x02 OUT int 10 (1082 if0, byte-for-byte). |
| 0x1bbbd | if1 HID `09 04 01 00 02 03 00 00 00`, HID `09 21 10 01`, EP 0x84 IN int 1, EP 0x04 OUT int 5 (1082 if1, byte-for-byte). |
| 0x1bbdd | if1 alt 0/1, class ff/47/d0 (GIP), isochronous EP 0x03 OUT / 0x83 IN (Xbox audio). |
| 0x1bbfd, 0x1bc1d | Single-interface HID variants with HID 1.11 (`09 21 11 01`). |
| 0x1acd8 | GIP interface ff/47/d0, EP 0x02 OUT / 0x82 IN (Xbox One mode). |
| 0x18be6, 0x1c8fb | XInput interface ff/5d/01 with the 0x21 XUSB descriptor, EP 0x82 / 0x02 (10A0 mode). |

HID report descriptors, byte-for-byte with `private/descriptors` (T8): 1082 if0 at 0x1e3d8,
1010 if0 at 0x1e46c, and 1082 if1 at 0x1e5cf.

Report IDs parsed from those descriptors:

| Descriptor | Reports |
| --- | --- |
| 1082 if0 | in 5 (9 B, gamepad), out 5 (4 B, rumble), in 2 (consumer) |
| 1082 if1 | in 3 / out 3 (keyboard), in 2, in 9 (7 B), in 0x10 and 0x12 (63 B vendor), out 0x0F (63 B vendor) |
| 1010 if0 | in 1 (63 B), out 5 (31 B vendor), feature 3 (47 B), feature 0xE0 (2 B, page 0xff80), in 0x10/0x12, out 0x0F |

1010 has a 2-byte feature report 0xE0. Guess: this is the "E0" in the Nexus UI, and it is a
HID feature report, not a GIP 0x0F command. T4 found only an incoming E0 state report in Nexus.

## Interface-1 report 0x0F command bytes

The table cannot be built from data alone. No window of up to 24 bytes in the image contains all
of the T4 command bytes 03, 04, 07, 09, 0B and F2, so there is no byte table to read. The dispatch
is compiled compare-and-branch code, which T9 could not disassemble. Related log strings:

| Offset | String |
| --- | --- |
| 0x1a735 | `setreportcmd_ID_13` |
| 0x1a749 | `setreportcmd_ID_14` |
| 0x1a721 | `SendPidReadBufBack` |
| 0x1a2d9 | `Usb Read PID out` |
| 0x1a49b | `Usb Write PID out` |
| 0x1a7e9 | `AppGetCurrrentMode` |
| 0x1bda1 | `rx usb sensor calibrate command` |

Guess: `ID_13` and `ID_14` are SET_REPORT handlers for report IDs 13 and 14 (0x0D/0x0E, if
decimal). Neither ID appears in any of the three report descriptors.

## LED path (0D)

Not found as code. The anchors are the strings `set rgb ctrl %d %d` (0x1a8b4), `pwm led init ...`
(0x1a441), `[PWM_LED] clock = ...` (0x1d878), `[BTSTACK]led port init` (0x1be88) and
`UserProfile.light_profile.brightness=%d` (0x18036). The last one ties the LED to the stored
profile's light block, which fits T4's light profile at offset 0xEAB.

## Mode-switch combo (View+Xbox+Menu gives 1010)

Not found. Searched for the log strings and for key-mask tables. The candidates are strings
only: `power on read key` (0x1a5a5), `FunkeyIn++++=0x%x` (0x1a546),
`f_PowerOnPressShareKey = %d` (0x1d47d), `f_PowerOnPressShareKey5 = %d` (0x1d61a),
`switch mode change=%x` (0x1adec) and `power on switchModeSet=%x` (0x1b5ac). The names fit a
key state that is read at power-on and selects the mode, which matches the plug-in-while-held
combo (guess).

The app-mode entry point confirms T4's rumble unlock:

- `gamesirappenc NULL` (0x199fb) is the only "gamesir" string in the image.
- `xinput app mode` (0x1a119) and `Exit App Enter Xinput` (0x17d67) name the app mode.
- Guess: the XInput rumble "gamesirapp" sequence is decoded by a `gamesirappenc` state machine.

Other mode names in the log strings: `force switch set wire {xinput,android,steam} mode`
(0x1c630, 0x1c3ea, 0x1c40e), `auto check set wire {xbox,xinput,android} mode` (0x1bc9d,
0x1be67, 0x1bff8), and `wire ps4 set:USB_PS4` / `USB_VENDOR_DEFINE` (0x1abd0, 0x1bc5d).

## Where the mode is stored

Not found as an address. isd_config.ini reserves a JieLi VM area: `VM_ADR=0; VM_LEN=12K; VM_OPT=1`.
Guess: VM_ADR 0 means the tool places it automatically. The firmware logs VM and "eeprom"
access (`[VM]vm_info:addr:0x%x, len:0x%x, mode:0x%x` 0x18418, `read vm ok0` 0x19c74,
`eeprom_page_num = %d profile_size = %d` 0x1caad, `frist power on reset eeprom!` 0x1ba2b).
Guess: the mode byte and the profiles live in that 12 KiB VM region of the pad's flash, and
the region is not part of the upgrade image.
