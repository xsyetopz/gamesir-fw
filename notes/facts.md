# Verified facts

Source in brackets. "Guess" marks anything not verified.

## G7 SE on macOS (fw 6.6.4, bcdDevice 0x1636)

- It enumerates as `3537:1082` "GameSir-G7 SE Controller for Xbox", with 2 HID interfaces.
  [ioreg, `descriptors/g7se-1082-config.bin`]
- Interface 0 (EP 0x82 IN, 0x02 OUT, 148-byte descriptor `if0`) is the gamepad, input report 5:
  15 buttons, hat, X Y Z Rz, Simulation Brake then Accelerator, all 8-bit. Output report 5 has
  4 bytes. Writing them did not change the LED. [descriptor, probes]
- Interface 1 (EP 0x84 IN, 0x04 OUT, 214-byte descriptor `if1`) has no macOS driver. It holds
  keyboard report 3, consumer report 2, mouse report 9, and vendor page 0xFFF0: input 0x10 and
  0x12 (63 bytes each), output 0x0F (63 bytes). [descriptor]
- Opening interface 1 produced keyboard report `03 00 00 46 …` (PrintScreen) 4 times, then
  nothing. Guess: Share is bound to PrintScreen. [`probes/probe.swift`]
- On interrupt OUT, report 0x0F got no reply to G7 Pro framing: heartbeat `0f 00 01 02`, info
  `0f 00 seq 01 09`. SET_REPORT over control (0x21/0x09, wValue 0x020F, wIndex 1) stalls with
  `0xE0005000`. [`probes/info.swift`]
- 2026-09-28: holding View+Xbox+Menu while plugging in, then releasing once the LED
  blinks rapidly, gives `3537:1010` "GameSir-G7 SE Controller for Xbox", bcdDevice 0x1636.
  [owner's description of the combo; ioreg idProduct 4112]
- 2026-09-28 22:49: after the owner pressed A in that blinking mode and reported the pad back
  at 1082, ioreg still showed one GameSir device at `3537:1010` (idProduct 4112, sessionID
  15383095120964), so pressing A did not re-enumerate the pad. After the owner replugged it
  (plain plug-in, no combo), it enumerated as `3537:1082` [ioreg; OpenJoystickDriver
  screenshot]. So 1010 lasts only until the next plug-in; 1082 is the default.
- Owner, 2026-09-28: the pad did not enumerate as 1010 by default until the Nexus update from
  fw 6.4.0 to 6.6.4. [owner report, not verified here]
- These combos did nothing, and the pad stayed 1082 with its LED off: Xbox+Share 3 s,
  Xbox+M 3 s plus replug, M+Y. [owner test, 2026-09-28]
- A reset on a borrowed Windows PC did not bring the LED back. [owner, 2026-09-28]
- Holding View+Xbox+Menu while plugging in USB gives a fast white LED blink, and the pad
  enumerates as `3537:1010` (bcdDevice 0x0664, ioreg prints it as decimal 1636; USB 2.0). So the LED hardware works. [owner, ioreg]
- In 1010 it has one HID interface (2 endpoints, Apple's HID driver owns it exclusively) with a
  177-byte descriptor (`descriptors/g7se-1010-if0.desc`):
  - input report 1: gamepad, 64 bytes, streams at about 250 Hz. Idle frame:
    `01 80 80 80 80 0f 00 … ff ff ff ff 00 00 00 80 … 01 08 00 cc`;
  - output report 5: 31 bytes, vendor 0xFF00 usage 0x22;
  - feature 3: 47 bytes, usage 0x2721; feature 0xE0: 2 bytes, page 0xFF80 usage 0x57. A GET of
    either stalls with `0xE0005000`;
  - the same 0xFFF0 vendor collection as 1082's interface 1: out 0x0F, in 0x10 and 0x12.
  [ioreg, `probes/feat1010.swift`]
- The 6.64 1010 descriptor (177 bytes) is the 6.40 one (app.bin 0x16e44, 130 bytes) with 18
  buttons instead of 14, 2 padding bits instead of 6, and the 0xFFF0 vendor collection (out 0x0F,
  in 0x10/0x12) appended. Reports 1, 5 (31-byte out), 3 and 0xE0 have the same sizes. The owner
  calls this state calibration mode (View+Home+Menu, blinking white LED). It has no mass-storage
  or vendor-class interface, so it does not give the JieLi download tool a `BR23 UDISK`.
  [item parse of both descriptors, ioreg 2026-09-29]
- On 1010, output report 0x0F with g7ctl framing is accepted (`kIOReturnSuccess`) but gets no
  reply on 0x10 or 0x12: heartbeat `0f 00 seq 02 f2 00`, info `0f 00 seq 01 09` and
  `0f 00 seq 01 0b`. The g7ctl `gamesirapp` handshake (raw 8-byte writes with no report ID on
  EP 0x02) was not tried. It needs the interface taken from Apple's HID driver.
  [`probes/info1010.swift`]
- T12, 2026-09-29, pad in 1010 (ioreg idProduct 4112): `IOHIDDeviceSetReport` output report
  0x0F with the Nexus framing, 64 bytes `0f 09` + 62 × `00` (version request, no GIP header).
  The write returned `kIOReturnSuccess`, and there was no input report 0x10 or 0x12 in 2 s. So
  neither the g7ctl framing nor the bare Nexus payload gets a reply over HID report 0x0F on
  macOS. The bytes reaching the wire unchanged is untested (PLAN risk). [`private/re/g7se --pid 1010 version`]
- Owner, 2026-09-29: Nexus on Windows falls back to "press any button to go into APP mode" the
  moment its window loses focus. So Nexus does not hold APP mode (1010) in the background.
  Guess: it stops the heartbeat or sends an exit, and the unlock needs a button press while
  the rumble repeats. [owner report]
- T15, 2026-09-29, pad in 1082 (idProduct 4226): Nexus's `gamesirapp` pattern sent to
  interface 0 as output report 5. Each pair was held 15 ms, then `05 00 00 00 00` for 5 ms,
  with the 5-pair sequence restarting every 500 ms, for 8 s per order (16 cycles, 0 failed writes):
  - order lo: `05 67 61 00 00`, `05 6d 65 00 00`, `05 73 69 00 00`, `05 72 61 00 00`, `05 70 70 00 00`;
  - order hi: `05 00 00 67 61` … `05 00 00 70 70`.
  The owner pressed A during each run. The pad was still 1082 after both, and neither run made
  the motors buzz. So report 5 written through the HID API does not drive the rumble motors on
  1082, either because it is not the rumble channel or because the bytes do not reach the pad
  as sent, and the unlock pattern never reached the motors. [`private/probes/rumble1082.swift`,
  ioreg, owner]
- 2026-09-29, Linux (OrbStack 2.2.3 machine `g7`, Ubuntu 26.04, kernel 7.0.14-orbstack, USB
  passthrough `orb usb attach 01140000`): the pad enumerates as `3537:1082` bcdDevice 6.64
  full-speed with the same two HID interfaces (EP 0x82/0x02, 0x84/0x04). `hid-generic` binds
  if0 as `hidraw0` (gamepad + consumer) and if1 as `hidraw1` (keyboard + mouse; "ignoring
  exceeding usage max"). So a Linux host does not get XInput either. Guess: the pad picks
  XInput from a Windows-only request such as the MS OS string descriptor 0xEE (`MSFT100` at
  app.bin 0x1a2b5). `xpad` is a loadable module in that kernel. [guest dmesg, lsusb -v]
- OrbStack passthrough does not survive re-enumeration (orbstack#2642, open). After any mode
  switch, run `orb usb detach 01140000 && orb usb attach 01140000` (the ID may change).
- 2026-09-29, Linux probes L1-L4 on 1082 in OrbStack `g7` (owner-approved; the pad stayed at
  `3537:1082` after every one). [`private/probes/linux_g7.py`]
  - L1, `hidraw0`: `05 80 80 80 80` for 1 s, then `05 00 00 00 00`. Both writes returned 5.
    Buzz: none (owner).
  - L2, `hidraw1`: 64-byte `0f 09`+zeros, then `0f 00 01 01 09`+zeros. Both writes returned
    64. No input report of any ID came back on `hidraw1` within 2 s after either packet.
  - L4, raw EP 0x02 with usbhid detached from if0: `00 08 00 LL RR 00 00 00` for the
    `gamesirapp` pairs (67 61, 6d 65, 73 69, 72 61, 70 70). Each pair held 15 ms, then
    `00 08 00 00 00 00 00 00` for 5 ms; the sequence repeated every 500 ms for 8 s. 16 cycles,
    0 write errors. So EP 0x02 accepts the 8-byte frames. Buzz: none (owner), so they are not
    rumble in `1082`.
  - L3, `GET_DESCRIPTOR` string 0xEE (`80 06 ee 03 00 00 12 00`) returned
    `12 03 4d 00 53 00 46 00 54 00 31 00 30 00 30 00 90 00`, which is `MSFT100`, vendor code
    `0x90`, flags `0x00`. Presence polls every 0.5 s for 5 s showed no re-enumeration, so
    reading 0xEE does not switch the mode by itself. Guess: Windows' next request, the vendor
    request `C0 90 00 00 04 00 ..` (MS OS 1.0 extended compat ID), is the trigger. That request
    is read-only but was not approved.
- 2026-09-29, macOS, `private/probes/seize10a0.swift` (IOKit first-match notification, then
  `IOUSBHostDevice` with `.deviceSeize`), owner replug with no buttons:
  - `10a0` appeared at +27.375 s and was opened with seize 4 ms later, unconfigured.
  - `GET_DESCRIPTOR` string 0xEE (`80 06 ee 03 00 00 12 00`) and vendor `C0 90 00 00 04 00 28 00`,
    sent at +13 ms, both failed with `0xE0005000` (pipe stalled).
  - `10a0` detached at +27.648 s (0.27 s after arrival, same as without the probe). `1082`
    arrived at +34.3 s, 6.6 s later instead of the usual 0.5 s. Guess: our open handle delayed
    the port teardown.
  - So in `10a0` the pad stalls the MS OS request that it answers in `1082` (L3). Guess: it has
    decided on `1082` before a user-space client can reach it, from what the macOS kernel did
    during enumeration. The OrbStack attach of `10a0` hit the same stall (`status -32`).
- `3537:1082` is GameSir's shared HID fallback PID, not a G7 SE-only ID. paroj/xpad#366 reports
  the K1 Kaleid as `3537:1082` (cold-boot PID 1096, GIP PID 1067 per a comment there), and
  dlundqvist/xone#209 shows the G7 HE falling back to it too. Each model's product string
  (`GameSir-G7 SE Controller for Xbox`) tells them apart. [GitHub issues, read 2026-09-29]
- dlundqvist/xone#209 (G7 HE, fw bcdDevice 6.64; a comment reports the same for the G7 SE): at
  power-on the pad enumerates as `3537:10a0` (the XInput/GIP boot PID, cf. app.bin 0x18bd4).
  Linux's init then fails with `-71`, and the pad re-enumerates as `1082` HID.
  `usbcore.quirks=3537:10a0:gk` (`g` = `USB_QUIRK_DELAY_INIT` is the flag that matters) makes it
  continue to GIP (`1065` on the HE), where interface 0 is FF/47/D0 and xone or xpad binds with
  rumble. Pressing Xbox is needed before the GIP announce. The G7 SE's GIP PID is unverified
  (guess: 1010).
- 2026-09-29, OrbStack `g7`: neither `orb usb detach`/`attach` nor a USB reset from the guest
  (`USBDEVFS_RESET`) restarts the pad's boot phase. It came back as `1082` each time, so `10a0`
  appears only at power-on. The guest's `xpad` has alias `usb:v3537p*...icFFisc47ipD0in*`.
  [guest dmesg, lsusb]
- 2026-09-29, macOS, owner replug with the `private/probes/watch_attach.py` log running: the pad
  appeared as `3537:10a0` (manufacturer string "Guangzhou Chicken Run Network Technology Co.,
  Ltd.", absent on 1082) for about 0.25 s. It then disconnected and returned as `1082` about
  0.5 s later. OrbStack listed the 10a0 device as "attached" but the guest saw only the
  disconnect. A second `orb usb attach` at 10a0 failed with "USB control transfer failed with
  status -32". So macOS enumeration alone drives the pad from 10a0 to 1082.
- app.bin 0x18bd4 (10a0): device descriptor `12 01 00 02 00 00 00 40 37 35 a0 10 64 06 01 02 03 01`,
  then interface 0 FF/5D/01 (Xbox 360 XInput) with EP 0x82/0x02 interrupt, 32 bytes.
- Owner: the pad ran 6.40 and worked on this Mac until Nexus forced 6.6.4 (2026-09-26).
  Community guides (asgardscripts.net "GameSir G7 HE/SE Setup", guide.cronus.support "GameSir
  Fix") report that after the newer firmware the G7 SE/HE no longer works on direct-connect
  hosts such as the Cronus Zen. Their fix is a downgrade to 6.4.0 with the Windows tool
  "G7 6.4.0.exe". Guess: the 10a0 host probe is new since 6.40 and is what macOS trips.
- `G7 SE-HE 6.40 Flash Tool - PID Protected.exe` (owner's Downloads, sha256 4c7e0658…2a5152,
  UPX-packed PE32, not Authenticode-signed): JieLi's Qt download tool. Its overlay inflates to
  an archive holding `config.ini`, `JS_SL3101_V640_Key.fw`, `JS_SL3101_V640_No_Key.fw` (413,024
  bytes each) and `Window.png`. Layout in "Flash Tool overlay".
  config.ini: PID match on, `[PCMode] name_list = br25udisk,br23udisk`, reset after success on,
  erase-all off. The tool flashes over USB mass storage (`jl_usb_scsi_windows`), so the pad must
  be in JieLi download mode (G7 Pro guides: hold Xbox+Menu while connecting; unverified on the
  SE). The `.fw` header opens with ENC key 0xFFFF: size 0x64d60, `AC695X`; the rest of its
  layout is decoded in "v6.40 .fw format and descriptors". [`private/flash-tool/`]
- The flash tool imports only SetupDi/CM_/DeviceIoControl/CreateFile, with no HID API, and
  enumerates USB storage volumes. It can reach the pad only when the pad shows up as a JieLi disk.
  v6.40 app.bin carries that disk's SCSI inquiry at 0x157be (`00 80 02 02 20 00 00 00` +
  `BR23    UDISK           1.00`), matching the tool's `br23udisk`. v6.64 app.bin has no SCSI
  inquiry block and no `UDISK`/`BR23` string; `usb_msd`/`MSDOS5.0` remain. So 6.64 probably
  cannot present the disk the 6.40 tool looks for from its app firmware. Unverified on hardware;
  the ROM "UBOOT" mode (`4c4a:2342`, per kagaimiq/jielie usb-ids) would be the other way in.
- v6.64 upgrade and mode strings: `xbox_gip_upgrade_enter` 0x1af86, `xbox_upgrade_mode=%d` 0x1ac6f,
  `set usb xinput boot mode` 0x1b26a, `boot_from_uboot_idx` 0x1b2ba, `factory all reset enter auto
  usb mode!` 0x1ca9d, and power-on combo logs (`power on Cross/Circule/Options/Share press`).
  MS OS compatible-ID strings `XGIP10` and `XUSB10` sit just before 0x1ca9d. So 6.64 enters
  upgrade over GIP, the Nexus path.
- icdd log, 2026-09-29 07:40-08:40: the only USB arrivals were GameSir `10a0`/`1082` pairs.
  No JieLi UBOOT/UDISK disk appeared during the owner's Xbox+Menu attempts. gamecontrollerd
  keeps a driver connection for 1082 (4226). For 1010 (4112) at 08:12 the connection was
  invalidated right after the check. [`log show`]
- On 1082 interface 0, through the HID API (Apple's HID driver in the path): the g7ctl
  `gamesirapp` handshake as report ID 0 (`00 08 00 c1 c2 00 00 00`, with a flush packet between
  chunks), then 10 heartbeats and info `01 09` and `01 0b` as report 0x0F. Every write returned
  `kIOReturnSuccess`. There was no re-enumeration (still 1082 after 4 s) and no input other than
  gamepad report 5. Success does not prove the bytes reached the wire unchanged: neither report
  ID is declared in if0's descriptor. [`probes/handshake1082.swift`,
  `probes/session1082-if0.swift`]
- Seizing 1082 interface 0 from Apple's HID driver (`IOUSBHostInterface` with `.deviceSeize`)
  fails at init with `0xE00002C9` (`kIOReturnInternalError`), even with OJD's client closed.
  WindowServer and Chrome still held it open. So raw EP 0x02 writes are not reachable from an
  app on macOS 27; that needs a USBDriverKit extension with a matching entitlement, or Linux.
  [`probes/seize1082-if0.swift`]
- On Windows, Nexus sees 1010 as GIP "Xbox One Game Controller". On macOS the same PID presents
  as plain HID, so the firmware may pick its protocol from how the host enumerates it. Guess.
- The first 1082 heartbeat probes lacked the fixed `f2 00` payload. Repeated correctly on
  interface 1 (EP 0x04 OUT): 14 heartbeats `0f 00 seq 02 f2 00` at 0.316 s, plus info
  `01 09` and `01 0b`, all accepted. No reply on EP 0x84 over 4 s; the only input was one idle
  keyboard (3), consumer (2) and mouse (9) report at open. [`probes/heartbeat1082.swift`]
- The GameSir FAQ (via gamesir-wiki) describes: Xbox+M 3 s then replug restores the original
  PID; Xbox+Share 3 s toggles XInput/GIP; Xbox+Menu 5 s does a function reset. Unverified on
  this unit.

## Nexus 2.5.8 app

- Package `GuangzhouChickenRunNetwor.16460D2A87234_2.5.8.0_x64__0zmrycy25r4k8`. [log]
- It reaches pads only over GIP in APP mode, PID 1010. The update session saw `0x3537/0x1010`
  "Xbox One Game Controller", then `ShouldBeAddedXInputPid: 0x10A0` after removal. [log]
- Update: `KeyType: Key, UpdateType: SingleUBoot`, image
  `embedded://FirmwarePackages\G7SE\JS_SL3101_V664_Key.ufw`, `JL_setDualUbootEnabled(0)`,
  `JL_loadFirmwareData ret: 0`. It sends through `Core.Dfu.FirmwareUpgrade_Gip.writeGipDevice`
  → `SendToDevice(Byte[])`. [log]
- Stack traces carry method names with offsets, so .NET Native kept stack-trace metadata.
  Guess: method names can be mapped to code addresses. [log]
- Vendor commands travel in GIP message 0x0F. 09/0A firmware version, 04/05 profile read,
  0B/0C current profile, 07 profile switch/save, 0D light on/off, E0 status, F2 test/stream,
  FE calibration, F0 firmware chunk. Rumble is GIP 0x09. [static strings, from an earlier
  analysis; byte layouts not recovered]
- The UI (`Web/js/app.*.js`, Vue) calls native code with
  `chrome.webview.postMessage({action, payload})`. Actions include `setConfig` (7 call sites),
  `saveProfile`, `commonReq`, `switchDeveloperMode` and `openDevTool`. [UI bundle]
- The G7 SE PIDs in the app's table are 1010 1069 1071 1073 1075 1077 108F 106D (G7 SE) and
  1064 1065 1079 (G7 HE), each with XInput PID 10A0; 1082 is not in it. [`ProxyG7SE..cctor`
  0x38eb700 calls to `DefaultModeCofnig`; an earlier note listed 1082 first, which was wrong]

## Flash Tool overlay (`crates/gsfw-core/src/formats/flashtool.rs`)

Read from `private/flash-tool/flashtool-640.exe` (0x7909a0 bytes) on 2026-09-29.

- The overlay starts at the end of the last PE section's raw data (UPX0, UPX1, `.rsrc`): file
  offset 0x6dee00, 0xb1ba0 bytes.
- The overlay is the byte `0xF8`, then Qt `qCompress` data: the inflated length as a big-endian
  `u32` (`00 0d 48 00`, 0xD4800), then a zlib stream (`78 da`). 64 bytes follow the stream.
- Archive header, 0x20 bytes: 4 bytes `2d 43 ed cc`, `u32` entry count (5), `u32` archive length
  (0xD4800), zeros.
- Records of 0x210 bytes from 0x20: `u8` flag (0, and 0xFE on the last record), `u8` index,
  `u16` CRC-16/XMODEM of the file, `u32` offset, `u32` size, `u32` 0, then the name (0x200
  bytes, NUL-terminated). All five CRCs match.
- Files: `data\config.ini` (0x1000, 8060 bytes), `data\JS_SL3101_V640_Key.fw` (0x3000),
  `data\JS_SL3101_V640_No_Key.fw` (0x67e00, both 413,024 bytes), `data\Window.png` (0xccc00,
  30,698 bytes), `tail.bin` (0xd4400, 64 bytes).
- `gsfw flash-tool-extract` writes files that are byte-identical to the hand-carved copies in
  `private/flash-tool/bundle/` (test `flash_tool_extract_matches_the_hand_carved_files`).
- `gsfw nexus-extract` on `private/nexus-app/HJC.GameSir.Nexus2_0.dll` wrote 34 files with the
  same sha256 as the earlier Python carve in `private/firmware/` (2026-09-29, 0.24 s).
- The catalog sums for this `.exe` and its two `.fw` images come from Python `hashlib`
  (2026-09-29). `gsfw fetch g7se-6.40-flash-tool OUT --from private/flash-tool/flashtool-640.exe`
  passes all three sums (test `fetch_from_a_file_passes_the_catalog_sums`).
- Eight more tools from GameSir (downloaded 2026-09-29 into `private/flash-tool/cdn/`) have the
  same archive, but a different overlay prefix and different first 4 archive bytes:

  | Tool | Prefix before the length | First 4 archive bytes | Files |
  | --- | --- | --- | --- |
  | 6.40 G7 SE (above) | 1 byte, `f8` | `2d43edcc` | 5 |
  | G7 SE/HE 6.55 | 514 bytes, from `a0 3d a4 3d fc 3d` | `582dfd44` | 5 |
  | Cyclone 2 3.52 | none | `7ca0908d` | 5 |
  | Cyclone 2 dongle 1.19 | 520 bytes, from `a0 3d a4 3d fc 3d` | `870e147a` | 5 |
  | Cyclone 2 dongle 1.21 | 5 bytes | `4129518e` | 5 |
  | K1 Flux 1.26 | none | `5b565b74` | 5 |
  | Dongle 231101a (Cyclone Pro) | 9 bytes | `75764cb9` | 4 |
  | Dongle 250319a (universal) | 14 bytes | `2ea8d61d` | 4 |
  | Tarantula Pro dongle 240930a | 5536 bytes | `3fd0c444` | 9 |

  The first 4 archive bytes are not the CRC-32 or the Adler-32 of `archive[4:]`, `archive[8:]`
  or `archive[0x20:]` (Python `zlib`). Their meaning is unknown. The Tarantula Pro tool has no
  64 bytes after the stream. All other tools have them. All record CRCs match in all nine
  tools. So `flashtool::inflate` uses the first zlib stream that inflates to the length in the
  4 bytes before it, and `entries` does not check the first 4 bytes.
- Sources of the tools: the 6.40 G7 SE tool is a OneDrive share that
  https://gamesir.com/pages/tutorial-how-to-use-gamesir-g7-se links. The Tarantula Pro dongle
  tool is on `download.gamesir.hk`. The other seven are on GameSir's Shopify CDN
  (`cdn.shopify.com/s/files/1/2241/8433/files/`). The exact URLs are in the catalog. The
  controller tools' `config.ini` `[Title]` names the model and the version. The dongle tools
  have no title, so their model names come from the GameSir download pages.
- `gsfw fetch ID OUT` downloaded each of the eight tools that have a URL (2026-09-29). Each
  `.exe` and each image passed its catalog sum. Python `hashlib` computed those sums from the
  downloads and from the `gsfw flash-tool-extract` output (test
  `each_downloaded_tool_passes_the_catalog_sums`).
- ureq 3.4.2 `BodyWithConfig::limit(n)` refuses any read after `n` bytes, also the read that
  finds the end of the body. So a body of exactly `n` bytes fails. `net.rs` passes `n + 1`
  (test `a_body_up_to_the_limit_is_returned` failed with `n`).

## Embedded resources in `HJC.GameSir.Nexus2_0.dll`

- The DLL is .NET Native: it has no CLR header, so dnfile gives `pe.net = None`.
- The name table starts near file offset 5069318. Each entry is a UTF-8 name, then two packed
  varints (offset, length). The low bits of the first byte set the size: `x0`=1, `01`=2,
  `011`=3, `0111`=4 bytes. [`crates/gsfw-core/src/formats/nexus.rs`]
- The blob section is contiguous at file offset 5084735. The first resource is
  `unsupported_devices.json` (1643 bytes); the rest are 33 `.ufw` images, 50,174,059 bytes in
  total.
- Images: G7Pro (PCWireless, WFF, ZZZ; DG and DV), G7SEPlus V118, G7SE `JS_SL3101_V664`, G7
  `JS_SL3100_V330`, K1Flux V126, K1 `1000hz_V163`, K2X V106, T3Pro V195, T7ProW V157, T7Pro
  DG V111 and DV V123. Each has `_Key` and `_No_Key` variants; one G7Pro image also has `915Key`.

## UFW format (G7SE `JS_SL3101_V664`, 1,166,336 bytes)

- Cleartext strings: `JS_SL3101` at 0x410; `2_3_0_0` at 0x1E3B; `PB01_00_0` at 0x1E49;
  `UPDATE_JUMP` at 0x1E53; `EOFFSET` at 0x1E79. Guess: a JieLi file table around 0x1E00.
- Key and No_Key are identical up to 0x1E27, except bytes 0x0–0x3, the 2-byte fields at 0x44,
  0x1D4, 0x224, 0x274, 0x2C4 and 0x314 (0x50 stride), and 4 bytes at 0x440. Guess: CRCs over
  differently encrypted bodies.
- After 0x1E27 nearly every byte differs, except 0x40000–0x5FFFF, which is identical (and
  0x60000–0x6FFFF, which differs in only 36 bytes).
  Entropy is about 7.99 bits/byte in both, so both are ciphertext. Guess: a padding or unused
  region encrypted with the same keystream.
- `JL_Upgrade_Gip.dll` (PDB `D:\git_109\hjc_ac695x_xbox_firmware_update_gip\…\JL_Upgrade.pdb`)
  exports `JL_getFirmwareDataCRC`, `JL_getFirmwareOrigCRC`, `JL_getFirmwarePidVid`,
  `JL_loadFirmwareData`, `JL_upgradeDevice` and `JL_upgradeDeviceByHandle`. Error string: "The
  firmware KEY does not macth the device KEY". Chip family: JieLi AC695X.

## Nexus log sequence

Source: `private/nexus-localstate/LocalState/Logs/Log - 20260926.log`, 644 lines, three app
starts (lines 2, 493, 609; 19:03, 19:40, 19:44). Line numbers below are file lines.

- 31-36: `D4XDeviceAdded(0)` `0x3537/0x1010`, GIP "Xbox One Game Controller"; state reports
  `firmwareVersion` 6.4.0 before the update. Every attach also logs "T7ProW proxy
  initialization" (33, 99, 167), so the G7 SE shares the T7ProW code path. Guess from the name.
- 39-48: `commonReq {"requestType":"requestToUpdate"}` → `FirmwareUpgrade_Gip` handle for PID
  0x1010 → attempt 1/3, `KeyType: Key`, `UpdateType: SingleUBoot`,
  `JS_SL3101_V664_Key.ufw` → `JL_setDualUbootEnabled(0)` → `JL_loadFirmwareData ret: 0` →
  `openGipDevice` → progress 0%.
- 49-64: progress 3, 5, 7, 31, then 32% "reset device"; the pad drops off (59) and XInput
  PID 0x10A0 "Xbox 360 Controller for Windows" is expected (65).
- 67-158: `closeGipDevice`/`openGipDevice` retries; six `writeGipDevice` failures with HRESULT
  0x80070006 (handle invalid) while the pad is away. Each stack is
  `ComCallHelpers.Call +0xc7` → `ID4XDevice__Impl.Stubs.SendMessage(__ComObject, Byte[], Byte)
  +0x8c` → `FirmwareUpgrade_Gip.SendToDevice(Byte[]) +0x5b` → `SendToDevice(Byte[]) +0xda` →
  `writeGipDevice(IntPtr, Byte*, UInt32) +0xcd` (e.g. lines 72-76). `SendMessage` takes a
  byte array plus one byte (guess: the GIP message type).
- 78-97, 239-258: unobserved `KeyNotFoundException` "No handler registered for event
  updateProgressChanged/0" from `EventBusMediator.<SendAsync>d__11 +0x35f`. UI plumbing only.
- 160-201: the pad returns as 1010 (instance 1), "re-open device" 35%, progress to 99%, 100%
  "upgrade success" (190), `Result: JL_ERROR_SUCCESS` (201). Total 52249 ms (207).
- 225-237: after the update the pad shows once as 0x10A0, then as 1010 (instance 2); Nexus
  reads the five profiles (`ReadProfile: Profile1..4, ProfileShift`, 233-237, 0.3 s apart),
  then `Current profile index: 1` (238). `firmwareVersion` is now 6.6.4 (261).
- 267-336: `commonReq calibrate` twice (UI steps logged, no bytes).
- 344-360: `commonReq saveProfile` ×5 and `switchProfile` ×2 (each followed by
  `reqSyncProfile`), then `setTestingInfo` (369) for the button test page (371-460, key names).
- 463-644: at 19:06:55 and in both later sessions, whenever the pad shows as 0x10A0 Nexus says
  "GameSir device detected, please press any key to switch to APP mode" (475, 525, 527, 557, 593, 639).
  In the second session the pad goes 0x10A0 removed (528) → 1010 added (531) 2.6 s later.
  The log does not say whether Nexus sent anything to 0x10A0 first. Guess: a key press on the
  pad while Nexus holds the XInput device triggers the switch.
- Raw bytes: the log holds no command frames (no `SendMessage` payloads, no 0x0F headers).
  It does hold the decoded 480-byte profile payloads as `-`-separated hex on lines 233-237,
  536-540 and 572-576 (checked with a regex for runs of 5+ hex pairs over all 644 lines;
  these 15 lines are the only hits). After the update all five profiles are identical; in
  the later sessions Profile1 differs from Profile2-4 and ProfileShift (which are identical),
  at 120 byte offsets from 0x30 to 0x1DF, reflecting the `saveProfile` edits. The
  `Profile Loaded` JSON (261, 542, 578; about 22.6 KB each) gives the UI field names
  (`motor.leftMotor`, `axisRatio`, `mouseDpi`, …) but is not valid JSON as logged.

## Nexus method map (read by a script removed on 2026-09-29)

- `HJC.GameSir.Nexus2_0.dll` keeps CoreRT reflection blobs in `.rdata`, each framed
  `[id u32][size u32][data][4 bytes]`, chain from file offset 0x2f32bc: ids 1, 2, 9, 7, 6, 23,
  21, 22, 14, 15, 10, 5, 19, 18, 8, 26 (0x336beb), 27 (0x3fe44e), 13 (0x44e50a), 24, 25
  (resources; matches `crates/gsfw-core/src/formats/nexus.rs`), 30, 32, 34. Image base 0x180000000.
  [file offsets from the tool]
- Blob 27: 40982 `(rva u32, token u32)` pairs; tokens are type 0x28 (39342, method reference
  `(parent, name, signature)` in blob 26) or 0x2b (1640, generic instantiation of a 0x28).
  Polymorphic handles in records are `offset<<8 | type` (0x3f type ref, 0x32 namespace ref).
- Methods with reflection metadata are not in blob 27. Their names come from blob 6
  (InvokeMap, NativeHashtable, entries `flags, method, type index, entrypoint index`; flags
  0x24), blob 8 (u32 RVAs by index), blob 1 (TypeMap: type index → TypeDefinition handle
  0x3cXXXXXX) and blob 13 (TypeDefinition: flags, base, namespace, name, size, packing,
  enclosing; NamespaceDefinition: parent, name). 6312 such methods; 356 have no TypeMap entry
  and print as `?type N`.
- Stack-trace offsets are return address − 1 − method start. Checked on all three log frames:
  `FirmwareUpgrade_Gip.SendToDevice` RVA 0x3b70ec0 (invoke map): +0x5b+1 follows
  `call rel32` → 0x3acffe0 = `ID4XDevice__Impl.Stubs.SendMessage` (stack map, the log's next
  frame); +0xda+1 follows `call [rip+disp32]`. `writeGipDevice` RVA 0x3b70da0: +0xcd+1
  follows `call rel32` → 0x3b70ec0 = `SendToDevice`. Without the −1 neither SendToDevice
  offset follows a call. `.pdata` starts agree (0x3b70da0, 0x3b70ec0; funclet at 0x3b70f31).

## G7 SE commands in Nexus (T4; details and RVAs in `notes/protocol-g7se.md`)

- The G7 SE proxy (`ProxyG7SE` ctor 0x3f9a940) calls `T7ProW.Proxy..ctor`. Its payloads are the
  Data of GIP message 0x0F; the GIP header is added by the Windows GIP driver below D4XGaming (see "GIP framing of upgrade payloads"), and the payloads carry no checksum.
- Requests and replies:
  - `09` version gets reply type 0A.
  - `0B` current profile gets reply 0C, with byte 1 = index.
  - `07 NN` switches profile; Shift = 5 and Light = 0x20.
  - `F2 BB` is the heartbeat.
- Profile read and write:
  - Read chunks are `04 II OH OL NN` with NN ≤ 55; the reply is type 05, 60 bytes.
  - Write chunks are `03 II OH OL NN data`, zero-padded to 60 bytes.
  - Light profile offsets start at 0xEAB.
- E0 is not a request. It is byte 0 of the pad's 60-byte state input (0x3c031e0).
- 0D light is built only by `K1.Proxy.setLightStatus` 0x3f83d00. The G7 SE inherits the empty
  base method, so Nexus sends no light command to it.
- The XInput → APP mode switch is not a GIP command. `XInputDeviceService` (tick 0x3c62bd0) rumbles
  the XInput pad (PID 10A0) with five (left, right) motor pairs that spell `gamesirapp`
  (L/255.0, 15 ms on, 5 ms off). It repeats every 500 ms, and the UI asks the user to press a
  key. On-wire rumble bytes: unverified.
- The profile length for idx 1–5 is 480 bytes (`K*7 + 0x154`). K = 20 is the length of the
  button-name array built in `C2.BasicProfileInfo..cctor` 0x38e72e0 (static RVA 0x3711048).
- The light profile (idx 0x20) is 21 bytes (`(M+1)*3`, `T7ProW.Commander.GetProfilePacketLength`
  0x3f70640). M = 6, the zone array Animation/A/B/X/Y/DPad built in
  `T7ProWRgbProfileInfo..cctor` 0x38e5640 (static RVA 0x3711060). `Parse` 0x3b9b860 reads 3
  single header bytes, then 3 bytes per zone. [Ghidra decompile, frozen strings in .data]
- Reading it takes one packet: `04 20 0e ab 15`.
- 1082 has one rumble-like output: interface 0 report 5, 4 bytes, LED page usages 0x43–0x46
  (`descriptors/g7se-1082-if0.desc`). The motor byte order is unknown. g7ctl's handshake
  `00 08 00 LL RR 00 00 00` is the Xbox 360 XInput rumble frame, so Nexus's `gamesirapp`
  rumble reaches the pad in that form when the pad runs as XInput 10A0. Guess: the firmware's
  `gamesirappenc` decoder (string at app.bin 0x199fb, next to `xinput app mode` at 0x1a119)
  listens only in XInput mode, not in 1082.

## `JL_Upgrade_Gip.dll` firmware loader (T6)

Sources:

- Ghidra decompiles of the exports: `JL_getFirmwarePidVid` 0x11b80, `JL_loadFirmwareData`
  0x11b00, `JL_getFirmwareDataCRC` 0x11c20, `JL_getFirmwareOrigCRC` 0x11c80.
- Loader `FUN_18000a530`, used when the global at 0x1e38c is 0 (the other loader, 0xf220,
  is not read).
- Helpers 0x18f0, 0x19c0, 0x1750, 0x56d0.
- Checked by decoding both G7SE `.ufw` files in Python.

What the loader does:

1. Header: it ENC-descrambles `file[0:0x40]` with the 16-bit key at global 0x1dee2. The
   keystream is `out ^= key&0xFF; key = key<<1 ^ (0x1021 if bit15)`. The global is BSS, set at
   runtime; key 0xFFFF makes both files' header CRCs match.
   - Layout `<HHIHHI` + name: hdr_crc, list_crc, image_size, entry_count, 0x0004 (must be < 5),
     0x200, chip name "AC695X".
   - hdr_crc = CRC16 over the descrambled `header[2:0x40]`, with poly 0x1021, init 0, not
     reflected (nibble table at 0xa530).
   - The loader also requires `image_size <= file size`.
   - Both files: image_size 1,166,336, which is the file size; 10 entries.
2. Entry table: `entry_count` entries of 0x50 bytes at 0x40.
   - list_crc = CRC16 over the raw (still scrambled) table; it matches in both files.
   - Each entry is ENC-descrambled on its own with the same key.
   - Entry layout `<HHHHIII`: type, index, data_crc, 0, offset, size, padded size; name at +0x40.
3. Entries in both files (offset, size, and whether data_crc = CRC16 of the stored bytes):

   | Type | Offset | Size | Name | Stored CRC |
   | --- | --- | --- | --- | --- |
   | 0 | 0x400 | 0x39700 | flash.bin | ok |
   | 2 | 0x39b00 | 0 | info.log | ok |
   | 0x34 | 0x39b00 | 0x10bb | isd_config.ini | mismatch |
   | 0x64 | 0x3abc0 | 0x34836 | ota.bin | ok; same in both files |
   | 0xFB | 0x6f400 | 0x1b | script.ver | mismatch |
   | 0x20 | 0x6f420 | 0x3a000 | flash2.bin | ok |
   | 0x21 | 0xa9420 | 0x3a000 | flash3.bin | ok |
   | 0x22 | 0xe3420 | 0x39700 | flash4.bin | ok |
   | 0xA1 | 0x11cb20 | 0x90 | blimit.bin | mismatch |
   | 0xFF | 0x11cbc0 | 0x40 | tail.bin | ok |

   The UPDATE_JUMP / PB01_00_0 strings sit inside the descrambled region around 0x1E00; they
   are not entry names.
   - Guess: the three mismatching entries are stored encrypted, with the CRC taken over the
     plaintext. ENC 0xFFFF, whole or in 32-byte or 512-byte blocks, does not give the stored
     CRC.
4. Entries by type:
   - Types 0 and 0x20-0x24 (flash images) go to `FUN_1800056d0`. It parses the 32-byte JLFS
     headers inside the image with the same ENC key and a CRC16 over 0x1E bytes (0x16b0).
     It re-encrypts them with `0x1870` and does not call the flash cipher. `flash.bin`'s
     first 32 bytes, ENC-descrambled with 0xFFFF, pass that CRC in both files.
   - Type 0xA1 (blimit.bin) is copied, then decrypted with `FUN_1800018f0(key@0x1dee0, …)`,
     CRC-checked, and passed to `FUN_180005520`.
     - `0x18f0` is JieLi's SFC cipher: 32-byte blocks, each ENC with
       `key ^ (file_offset >> 2)`.
     - key@0x1dee0 = `FUN_1800019c0(32 bytes)`. That is the chipkey.bin decode: byte-sum of
       bytes 0-15, clamped to 0x55 if < 0x11 or 0xAA if > 0xDF; bit i is set when a byte
       pair XOR is below that sum.
     - It is fed from an entry whose 0x20-byte CRC matches; which entry is not identified.
5. After the entries:
   - DataCRC (0x1da12) = CRC16 (same parameters) over the whole file.
   - The 32-bit field at 0x1da18 = `FUN_180001750(file)`, a CRC32 with reflected poly
     0xEDB88320 (init not read).
   - `JL_getFirmwareOrigCRC` returns 0x1da14 / 0x1da1c; their writer is not read.
   - `JL_getFirmwarePidVid` copies strings from state +0x2f8 (PID, ≤16 chars) and +0x309
     (VID, ≤4 chars).
6. Decryption on the host: headers and entry table (ENC 0xFFFF), JLFS headers (same key),
   and blimit.bin (SFC with a derived chip key). The flash image bodies are not decrypted on
   the host (inference: 0x56d0 never calls 0x18f0); the chip does that with its own key.
7. "The firmware KEY does not macth the device KEY" (string RVA 0x19518) is referenced from
   0x14b09 and 0x168b2. Those upgrade-path functions are not read.

These layouts match kagaimiq's `fwunpack_newfw.py` (`notes/jieli-tooling.md` §1).

## UFW decryption (T7-T8; `crates/gsfw-core/src/formats/ufw.rs`)

- One chip key for all images:
  - Every `_Key` image of the 10 models (G7, G7SE, G7SE+, K1, K1 Flux, K2X, T3 Pro, T7 Pro
    DG/DV, T7 Pro W) uses chip key 0x4317.
  - Every `_No_Key` image uses 0xFFFF.
  - Found by known plaintext: `isd_config.ini`'s first 32 bytes are '#'.
- The UFW's encrypted entries are SFC-encrypted:
  - Affected entries: isd_config.ini, script.ver, blimit.bin.
  - Block key: `chipkey ^ (absolute file offset >> 2)`.
  - The stored CRC16 is over the plaintext.
  - With this, `ufw.py info` reports every header, table and entry CRC as ok on all 20 images.
- Values from the decrypted G7SE `isd_config.ini`:
  - `PB01_00_0`, `UPDATE_JUMP`, `EOFFSET` and `VLVD` are its settings, not UFW sections:
    RESET=PB01_00_0 (reset pin PB01, long-press off), UPDATE_JUMP=0, EOFFSET=1 (n*4k), VLVD=4.
  - `PSRAM` does not occur.
  - It also has PID=JS_SL3101, VID=6.64, FLASH_SIZE=512K, FLASH_BIN_CNT=6, and
    `SDK_TYPE=SOUNDBOX`.
  - script.ver = "AC695X-v0.01-cfg_tool-v0.07".
- flash.bin (UFW entry 0) is a JLFS image:
  - 32-byte header (ENC 0xFFFF, CRC ok), flash_size 0x7ff00.
  - Top table at 0x20 (each entry ENC 0xFFFF): uboot.boot 0xc0, isd_config.ini 0x1a14,
    app_dir_head 0x1b20, app_dir_head2 0x3ed20, key_mac 0x7ff00.
- `app_dir_head`'s table and files are SFC with the chip key; offsets are relative to 0x1b20.
  - Entries: app_area_head (maps at 0x1e000c0), app.bin +0xc0 size 0x37570, cfg_tool.bin
    +0x37630, VM, PRCT, BTIF.
  - The decrypted app.bin matches its stored CRC16 (0x743e for G7SE) and is identical from
    `_Key` and `_No_Key`.
  - cfg_tool.bin does not match with the same rule (not investigated).
- The G7SE v6.6.4 app.bin plaintext (`private/re/ufw/g7se-v664-nokey.bin`) contains:
  - `37 35 82 10` at 0x1a275 and `37 35 10 10` at 0x18b8d;
  - `g7se-1082-if0.desc` at 0x1e3d8, `g7se-1010-if0.desc` at 0x1e46c and
    `g7se-1082-if1.desc` at 0x1e5cf, byte for byte.
  - No `37 35 22 10` (PID 1022).

## v6.40 .fw format and descriptors (`crates/gsfw-core/src/formats/ufw.rs`, `.fw` suffix)

Source: `private/flash-tool/bundle/JS_SL3101_V640_Key.fw` and `_No_Key.fw` (413,024 bytes each,
from the "G7 SE/HE 6.40 Flash Tool - PID Protected" bundle), decoded in Python; `ufw.py info`
reports every CRC as ok on both.

- Same container as the UFW: 0x40-byte header `<HHIHHI` + chip name, entry table of 0x50-byte
  entries at 0x40, header CRC16 over the descrambled `header[2:0x40]`, table CRC16 over the raw
  table. Differences from the UFW:
  - Header and table are ENC-keyed with the chip key itself: 0x4317 (`_Key`), 0xFFFF
    (`_No_Key`). Of the 8 keys that put "AC695X" at 0x10, only that one passes the header CRC.
  - Each entry CRC16 covers the stored bytes up to the padded size (checked on all 6 entries of
    both files; e.g. ota.bin: CRC over size 0x34836 = 0x3c15, over padded 0x34840 = 0x290f, the
    stored value).
  - isd_config.ini is SFC with the chip key and offsets relative to the entry (block key
    `chipkey ^ (rel_off >> 2)`), not absolute file offsets. script.ver and ota.bin are stored
    in clear (ota.bin is byte-identical to the v6.64 UFW's ota.bin); tail.bin is not decoded.
- Header: image_size 0x64d60 (= file size), 6 entries, 0x0004, 0x200, "AC695X".
- Entries (both files): flash.bin 0x400 size 0x2f000; info.log 0x2f400 size 0; isd_config.ini
  0x2f400 size 0x10a4; ota.bin 0x304c0 size 0x34836; script.ver 0x64d00 size 0x1b; tail.bin
  0x64d20 size 0x40.
- The files differ only in 0x0-0x21f (header, table), 0x440-0x443, 0x1e27-0x1e35,
  0x2400-0x2ec86 (the SFC-keyed app directory), 0x2f400-0x304bf (isd_config.ini) and
  0x64d33-0x64d41 (tail.bin).
- isd_config.ini decodes to the same text from both: PID=JS_SL3101, VID=6.40, FLASH_SIZE=512K,
  SDK_TYPE=SOUNDBOX. script.ver = "AC695X-v0.01-cfg_tool-v0.07".
- flash.bin is JLFS like the UFW's (top table ENC 0xFFFF in both files): uboot.boot 0xc0,
  isd_config.ini 0x1a14, app_dir_head 0x2020, app_dir_head2 0x3e020, key_mac 0x7f000.
  app_dir_head (SFC, chip key, offsets relative to 0x2020): app.bin +0xc0 size 0x2c258, stored
  CRC 0x8fd9, which the decrypted body matches; identical from both files
  (`private/re/fw640/app.bin`).

Descriptors in the v6.40 app.bin compared with v6.64 (`private/re/ufw/g7se-v664-nokey.bin`):

- v6.40 has a table of six 18-byte device descriptors at 0x142a5 (all bcdUSB 0x0200, bMaxPacket
  64, one configuration):

  | Offset | Class | VID:PID | bcdDevice | iSerial |
  | --- | --- | --- | --- | --- |
  | 0x142a5 | 00 | 20d6:c001 | 0x0640 | 0 |
  | 0x142b7 | 00 | 3537:1010 | 0x0640 | 3 |
  | 0x142c9 | 00 | 3537:10a0 | 0x0101 | 3 |
  | 0x142db | 00 | 3537:012c | 0x0100 | 0 |
  | 0x142ed | ff/ff/ff | 3537:1010 | 0x0640 | 3 |
  | 0x142ff | ff/ff/ff | 3537:0158 | 0x0640 | 3 |

- v6.64's device descriptors: 3537:1010 (0x18b85), 3537:10a0 (0x18bd4), 3537:1082 (0x1a26d),
  2c16:c001 (0x1a27f), vendor-class 3537:1010 (0x1a291) and 3537:0158 (0x1a2a3), 0f0d:01ab
  (0x1a37b), all bcdDevice 0x0664.
- So v6.40 has 3537:10a0 (bcdDevice 0x0101, not 0x0640) and 3537:1010, but no 3537:1082
  anywhere (no `37 35 82 10`). v6.40 alone has 3537:012c and 20d6:c001; v6.64 alone has
  3537:1082, 2c16:c001 and 0f0d:01ab.
- The "MSFT100" string descriptor (`12 03` + UTF-16 "MSFT100" + vendor code 0x90) is at 0x14311
  in v6.40, right after the table; v6.64 has it at 0x1a2b5 and 0x1a38d.
- Interface descriptor templates found in v6.40: MSC 08/06/50 (0x12d84), audio 01/01 and 01/02,
  HID 03/00/00 twice as if0 (0x15279, 0x15299), XInput ff/5d/01 (0x15b37), Xbox GIP ff/47/d0 as
  if0 (0x149a7) and if1/if2 with alt settings (0x16888-0x168b1). v6.64 adds an HID if1 (0x1bbbd)
  and more HID if0 copies, and a second XInput template; it has GIP ff/47/d0 only as if0 and if1.
- None of the macOS G7 SE HID descriptor files appear whole in v6.40. The first 16 bytes of
  `g7se-1010-if0.desc` match at 0x16e44; there the v6.40 descriptor is 130 bytes (v6.64: 177) and
  declares 14 buttons (usage max and report count 0x0e) where v6.64 declares 18 (0x12).
- v6.40 has no "gamesir" string (v6.64: `gamesirappenc NULL` at 0x199fb) and none of v6.64's
  xinput or mode-switch log strings (`xinput app mode`, `force switch set wire ...`).
- Guess: 1082 (and the two-interface HID layout) arrived after 6.40; in 6.40 the pad enumerates
  as 1010 or 10a0.

## GIP upgrade protocol (JL_Upgrade_Gip.dll)

Source: Ghidra decompile of `JL_Upgrade_Gip.dll` (`private/re/gip-upgrade/all.c`), 2026-09-29, by a
subagent. Spot-checked here: the `0xa4a` / `0x34` fragment header in 0x1f80, the `JSUD` magic
`0x4455534a`, and the `HJX1` and export strings. The rest is taken from the decompile and not yet
re-checked.

- Transport: the DLL does no USB I/O itself. The caller registers open, close and
  `write(handle, buf, 0x38)` with `JL_registerGipOperations` (0x1ef0), and feeds each received
  GIP payload to `JL_handleGipData` (0x2120), which reads the last 0x38 bytes. The GIP message
  type Nexus wraps these payloads in is unknown (Nexus `SendToDevice` RVA 0x3b70ec0, not read).
- Fragments (0x1f80): one 512-byte packet goes out as ten 56-byte payloads,
  `4A 0A idx len data[52]`, with idx 1..10 and len 0x34 (0x2C for the last). The pad acks each
  with `4C 0A idx`. The host resends after 1000 ms without an ack. Replies use the same `4A`
  framing.
- Packet: `JSUD` + CRC16/XMODEM(bytes 6..15) + `01 00` + body CRC16 + body length + 4-byte tag
  (echoed in the reply); the body starts at 0x10. The whole 512 bytes are ENC-scrambled with a
  session key.
- Commands:
  - C0 handshake: `HJX1` at 0x14, 16 random bytes at 0x18, scrambled with 0x1011. The session
    key comes from both sides' random bytes; the exact mix is a guess, see 0x4520.
  - C1 query: the chip key from `tail.bin` at 0x14. The reply carries the pad's JLFS header;
    status -3 means "KEY does not match".
  - C3 erase (address, unit 1/2/3 = 256 B / 4 KB / 64 KB).
  - C4 write (address, CRC16 at 0x1a, length, up to 0x100 data bytes at 0x20).
  - C5 CRC read (address, length → status, CRC16).
  - CB range (start, end); its purpose is a guess.
  - CA reset.
  - C2 result (text such as "upgrade success").
- Sequence (0x14a20, 0x56d0, 0xb3f0):
  - The host sends one flash image, stored bytes unchanged. It picks `flash.bin` or one of
    `flash2`-`flash4` from the pad's erase unit and flash mode.
  - The image is split at the `app_dir_head` offset into A (the part before), B (A with the
    `app_dir_head` flag changed) and C (the rest).
  - The host writes C at `app_dir_head2`, then 32 zero bytes at `app_dir_head`, then resets and
    handshakes again. It then writes B at 0x10000 and A at 0, and sends C2.
  - Each region: erase, check the CRC is all-0xFF, write from 0x100 up with the first chunk last,
    check the CRC, up to 5 tries.
  - `ota.bin`, `isd_config.ini` and `script.ver` are not sent. `tail.bin` (chip key) and
    `blimit.bin` (`JLDGAUTH`, original CRCs) are used on the host only.
- Guess: a v6.40 `.fw` (same container, has `flash.bin` and `tail.bin`) could be sent the same
  way. The only pad-side check seen is the chip key in C1.

### GIP framing of upgrade payloads

Source: `private/re/gip-upgrade/nexus-disasm.txt`, `nexus-sendtodevice.c` (Nexus, image base
0x180000000) and `d4x-all.c`, `d4x-onmessage.txt` (Ghidra with analysis on
`D4XGaming.Devices.dll`), 2026-09-29. RVAs of the three Nexus methods confirmed against
`nexus-methods.tsv` (invoke map) and the disassembly.

- Host to pad, verified. `writeGipDevice` (0x3b70da0) copies the 0x38-byte native buffer into a
  `byte[]` and calls `SendToDevice` (0x3b70ec0). That allocates `byte[0x3C]`, stores 0xF0 at
  index 0 (`movb $-0x10, 0x10(%rax)` at 0x3b70ee4), copies the payload to index 4 (indirect call
  at 0x3b70efd with args src, 0, dst, 4, src.Length; Array.Copy by its argument shape, the
  callee cell is lazily bound and not resolved), and calls `ID4XDevice.SendMessage(data, 0x0F)`
  (0x3acffe0, `movb $0xf, %r8b` at 0x3b70f11). GIP data is therefore 60 bytes:
  `F0 00 00 00` + `4A 0A idx len data[52]`.
- The same message id 0x0F carries the profile commands (`notes/protocol-g7se.md`); the upgrade
  traffic differs only by the first data byte 0xF0.
- `D4XGaming.Devices.dll` does not build a GIP header, verified. Its `ID4XDevice` vtable is at
  0x180008868; slot 13 (0x1800088d0) is `SendMessage` = 0x180001be0, which calls the
  `IGipGameControllerProvider` (IID at 0x180008ad8) vtable +0x30 with `(0, messageId, len,
  data)`: `GipMessageClass.Command` (0). Slot 14 `SendMessageA` (0x180001bb0) is the same with id
  9. Windows (`Windows.Gaming.Input.Custom`, then the OS GIP driver) adds the header. This
  corrects "the GIP header is added in D4XGaming" in the T4 section above.
- Wire header, guess. Windows should send `0F flags seq 3C` + the 60 bytes, one 64-byte USB
  packet, so no chunking. The provider API takes no sequence or flag argument, so the OS driver
  picks the sequence number and whether the ack flag (0x10) is set; the system flag (0x20)
  should be clear for a Command-class vendor message. None of this is in the two DLLs; a
  Windows USBPcap capture of an update would settle it.
- Pad to host, verified. The `IGipGameControllerInputSink.OnMessageReceived` handler
  (0x180001870, vtable slot 7 at 0x180008930) reads only the length and buffer arguments and
  drops the message class, message id and sequence, then raises `MessageBReceived(sender, len,
  data)` (0x1800019d0). Nexus `Instance_OnReceiveMessage` (0x3b71d20, reached from
  `Device_MessageBReceived` 0x3b71d10) checks the sender against the upgrade device, then
  requires `data[0] == 0xF1` (`cmpb $-0xf` at 0x3b71d6a) and length 0x3C (0x3b71d70).
  `DownloadACK` (0x3b71d90) copies the 60 bytes to index 4 of a `byte[0x40]` and calls
  `JL_handleGipData(buf, 0x40)` (0x3af26a0), which takes the last 0x38 bytes, that is data 4..59.
  So acks and replies arrive as `F1 ?? ?? ??` + 56-byte payload; bytes 1..3 are ignored.
- The GIP message id of replies is not visible to Nexus, since D4XGaming drops it. Guess: 0x0F,
  like the requests.

### Upgrade transport over HID (1082), 2026-09-29

- **Setup.** Interface 1 of `3537:1082` had no driver attached (ioreg: no `AppleUserHIDDevice`
  child), so `private/probes/raw1082if1.swift` opened it and wrote raw interrupt OUT packets on
  EP 0x04, zero-padded to 64 bytes. All writes returned `st 0`, n 64.
- **Packets sent.** Fragment 1 of an upgrade packet (`4a 0a 01 34` + zeros) in six framings:
  `0f f0 00 00 00 4a…`, `0f 00 01 3c f0…`, `0f 20 02 3c f0…`, `0f 10 03 3c f0…`, `f0 00 00 00 4a…`,
  `0f 4a 0a 01 34`.
- **Result.** EP 0x84 returned only the idle keyboard (`03`), consumer (`02`) and mouse (`09`)
  reports within 1.2-1.5 s. There was no `4c 0a 01` fragment ack. The DLL waits 1000 ms for a
  per-fragment ack (0x1f80), so a pad that took the fragment would have answered.
- **Conclusion.** In 1082 the pad does not route upgrade fragments from interface 1. Together
  with the silent profile commands (`info.swift`, T12), the vendor 0x0F path looks
  GIP-only.
- **What the Nexus log adds.** The 2026-09-26 session (log lines 41-138) upgraded a pad that
  enumerated as GIP `3537:1010` ("Xbox One Game Controller") on Windows. It reset at 32%, came
  back as GIP 1010, and ended with `upgrade success` / `JL_ERROR_SUCCESS`. So 6.64 still enters
  GIP 1010 on a Windows host, and the flash of 6.64 completed; it was not a partial write.

### C1 reply and flash directory (read 2026-09-29; client in `crates/gsfw-core/`)

- **Status reply (0x4da0).** u32 2 at 0x10, i32 status at 0x14. On status 0 it copies a u32 at
  0x1c and 16 bytes at 0x28. The exchange (all.c ~11690) also checks that the reply's tag at 0x0c
  equals the sent tag.
- **C1 reply (all.c ~12560-12760).** Bytes 0x18..0x38 hold a 32-byte flash header, then seven
  32-byte directory entries up to 0x118. Each is ENC-scrambled with 0xFFFF. The header CRC is
  `crc16(h[2:32]) == u16 h[0]`. Each entry has its name at +16 and u32 address at +8. The code
  takes the address of `app_dir_head` and `app_dir_head2`, and adds 0x1000 to both when byte 0x11b
  (EOFFSET) is 1, or 0x10000 when it is 0x10. Byte 0x11a is the mode. u32 header +8 and byte
  header +13 go to outputs of unknown meaning.
- **Same layout in `flash.bin`.** The first 0x100 bytes of `flash.bin` in `JS_SL3101_V664_*.ufw`
  decode this way. The header CRC passes, and header +8 is 0x7ff00. The entries are `uboot.boot`
  0x1954, `isd_config.ini` 0x71, `app_dir_head` 0x1b00, `app_dir_head2` 0x3ed00 and `key_mac`
  0x100 (`crates/gsfw-core/tests/g7se_images.rs`). A pad's C1 reply can therefore be compared with the image's head.
- **C4 body length.** The body is data length + 0x18 (0x3540), so 8 zero bytes follow the data
  inside the body CRC. The fixed commands also have 0x18-byte bodies, which end at 0x28.
- **macOS gets no GIP interface.** It sees only 1082 (`jlgip.py list`: two HID interfaces, no
  `ff/47/d0`). The live runs below used Windows 10 with WinUSB bound to the 1010 device by Zadig.
  On 6.40 this changes: after the flash-back, `gsfw list` on macOS shows `3537:1010` bcdDevice
  0x0640 with three `ff/47/d0` interfaces (if0 interrupt 0x02/0x82, 64 bytes; if1 alt1
  isochronous 0x03/0x83, 228 bytes; if2 alt1 bulk 0x01/0x81, 64 bytes), so libusb can reach GIP on macOS for a pad on 6.40 (2026-09-29, `gsfw list`).
  No upgrade command was sent to it on macOS.

### Live runs on the G7 SE (2026-09-29, Windows 10, pad on 6.64, `private/captures/jlgip-*`)

- **GIP framing works as guessed.** Host sends `0F 00 seq 3C` + `F0 00 00 00` + fragment. The pad
  acks each fragment with `4C 0A idx len` + echoed data. Its replies are GIP command 0x10, own
  seq, `F1 00 00 00` + `4A 0A idx len data`. Power-on `05 20 00 01 00` is sent first.
- **C0.** The pad accepts C0 scrambled with 0x1011 but scrambles its reply with 0x5a5a
  (`jlgip-c0-reply1.bin`). It answers one C0 per power-up. Later C0s get fragment acks and no
  reply, and a libusb reset does not help, but a replug does. The session key formula is right:
  run 4 derived 0x1896, and C1 replies descramble with it.
- **C1.** The `_No_Key` image's chip key gets status -3. The `JS_SL3101_V664_Key.ufw` key works
  (`jlgip-c1-reply-key.bin`): mode 2, EOFFSET 1, flash size 0x7f000, header CRC ok. Directory:
  `uboot.boot` 0x1954, `isd_config.ini` 0x71, `app_dir_head` 0x2000, `app_dir_head2` 0x3e000,
  `key_mac` 0x1000, so the app dirs are 0x3000 and 0x3f000. The pad's head equals the head of
  `flash3.bin` in the 6.64 Key image (0x1000 erase unit). `JS_SL3101_V640_Key.fw` `flash.bin` has
  the same directory, and both `_Key` images have the same chip key.
- **C5 replies have type 4**, not 2. The status is at 0x14 and the CRC at 0x18.
- **Flash map (C5 CRCs).** Device address = `flash3.bin` offset + 0x1000:

  | Range | CRC | Matches |
  | --- | --- | --- |
  | 0x0+0x100 | 0x1ac7 | 0xFF bytes |
  | 0x0+0x3000 | 0xc5cc | 0xFF×0x1000 + 6.64 `flash3[:0x2000]` (region A) |
  | 0x1000+0x100 | 0x85be | `flash3[:0x100]` |
  | 0x2000+0x1000 | 0x15ee | `flash3[0x1000:0x2000]` |
  | 0x3000+0x1000 | 0xb226 | 32 zero bytes + 6.40 Key `flash.bin[0x2020:0x3000]` |
  | 0x3f000+0x1000 | 0x01ab | `flash3[0x2000:0x3000]` (6.64 app) |

- **What the Nexus update did.** The pad ran 6.40 from bank 1, so it was in mode 1. Nexus wrote the
  6.64 region C to bank 2 (0x3f000) and zeroed the first 32 bytes of bank 1 (0x3000). This is the
  DLL's mode-1 path. The rest of the 6.40 app is still in bank 1. The pad now runs bank 2 and
  reports mode 2.
- **Region A, 6.40 vs 6.64.** 9 bytes differ in `flash[:0x2000]`: header CRC (0-1), header +6
  (2 bytes), the `isd_config.ini` entry CRCs (0x40-0x43), and `isd_config` byte 0x1a74:
  `VLVD` 7 in 6.40, 4 in 6.64. Boot code and directory are identical. (That `VLVD` sets the
  low-voltage detect level is a guess from the name.)
- **Mode 2 path (0x14a20, all.c ~13010-13080).** If region A matches, region C is written at the
  device `app_dir_head` (0x3000), then 32 zero bytes at `app_dir_head2`, then C2. A region write
  (0x140c0, up to 5 tries) does C3 erase over the whole region (0x12900: 64K blocks where
  64K-aligned, else 4K, else the unit, no reply read), a C5 blank check, C4 in 0x100 chunks with
  the first chunk last (0x12c10, no reply read), then a C5 verify.
- **Flashed 6.40 back (`private/captures/jlgip-flash640.log`).** `jlgip.py flash
  JS_SL3101_V640_Key.fw --keep-region-a --yes` wrote the 6.40 region C (0x2d000 bytes) to 0x3000.
  Erase took 15 blocks, the blank check passed, 720 C4 chunks were written, and the verify CRC was
  0xd820 on the first try. It then zeroed 0x3f000+0x20 (CRC 0x0000) and C2 returned status 0.
  Region A kept 6.64's `VLVD` 4. After a replug on Windows, `jlgip.py list` shows `3537:1010`
  bcdDevice 0x0640, so the pad boots 6.40. The 6.40 descriptor has a third `ff/47/d0` interface
  (if2 alt1, endpoints 0x01/0x81) that 6.64 did not list. On macOS the pad enumerates as
  `3537:1010` again. OpenJoystickDriver manages it as Xbox One protocol (input endpoint 0x82,
  output endpoint 0x02), and the LED works (owner's screenshot and report, 2026-09-29). OJD's
  Input Test shows "Input active" over `xbox.gip:usb` 3537:1010, and the owner reports inputs
  working. Rumble works from OJD's Input Test (owner's test). The LED is a single white LED that
  cannot be configured, so OJD's missing lighting controls are correct for this pad (owner).

### Live run on the G7 SE (2026-09-29, macOS, pad on 6.40, owner-approved)

Logs: `private/captures/macos-probe-1.log`, `private/captures/macos-crc-1.log`.

- While OpenJoystickDriver runs, it holds if0 of `3537:1010` exclusively (IORegistry
  `UsbExclusiveOwner` "OpenJoystickDriv"). The `gsfw` claim then fails with 0xe00002c5
  (`kIOReturnExclusiveAccess`).
- After the owner quit OJD, the pad re-enumerated (new IORegistry id) with no active
  configuration. No macOS driver matches class `ff/47/d0`, so nothing configures it. `gsfw` now
  sends SET_CONFIGURATION with the first configuration value when no configuration is active
  (`usb::configure`).
- `gsfw probe JS_SL3101_V640_Key.fw --pid 1010 -v`: the pad answered the xpad power-on with its
  GIP announce. C0 gave session key 0x25cc (reply scrambled with 0x5a5a, as on Windows). C1:
  mode 1, EOFFSET 1, flash size 0x7f000, head CRC ok. The directory is the same as on 6.64
  (`uboot.boot` 0x1954, `isd_config.ini` 0x71, `app_dir_head` 0x2000, `app_dir_head2` 0x3e000,
  `key_mac` 0x1000). On 6.64 (Windows) C1 gave mode 2. The meaning of the mode change is not
  known.
- `gsfw crc ... 0x3000 0x2d000 --session-key 25cc`: C5 CRC 0xd820. This is the CRC that the
  Windows flash verified for the 6.40 region C, so the pad still holds that image.
- After both runs, `gsfw list` shows `3537:1010` with its GIP interfaces.
