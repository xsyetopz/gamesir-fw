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
- These combos did nothing, and the pad stayed 1082 with its LED off: Xbox+Share 3 s,
  Xbox+M 3 s plus replug, M+Y. [owner test, 2026-09-28]
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
- The G7 SE PIDs in the app's table are 1082 1069 1071 1073 1075 1077 108F 106D. [earlier
  analysis]

## Embedded resources in `HJC.GameSir.Nexus2_0.dll`

- The DLL is .NET Native: it has no CLR header, so dnfile gives `pe.net = None`.
- The name table starts near file offset 5069318. Each entry is a UTF-8 name, then two packed
  varints (offset, length). The low bits of the first byte set the size: `x0`=1, `01`=2,
  `011`=3, `0111`=4 bytes. [`tools/extract_nexus_resources.py`]
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
