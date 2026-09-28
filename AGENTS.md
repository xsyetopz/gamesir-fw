# Agent brief

Goal: learn enough from GameSir firmware and the Nexus app to switch a GameSir pad's mode and
drive its LED and settings from macOS without Nexus. First target: G7 SE (`3537:1082`, fw 6.6.4).
Second goal: decrypt the `.ufw` images well enough to read the USB and GIP command handlers.

Verified facts live in `notes/facts.md`. Read it first. Add what you verify there, with its
source (a file offset, a capture frame or a probe output). Mark guesses as guesses.

## Rules

- No firmware writes, DFU entry or flashing to any device. No output or feature report to a pad
  unless the owner authorizes that exact probe. Read-only queries need the owner's approval too.
- Never commit, upload or publish anything under `private/`, or any firmware, DLL, capture or
  derived plaintext. The repo stays local; a remote needs the owner's approval.
- Do not open or copy `.sys` files. Do not copy the Nexus WebView2 profile (cookies, history).
- Standalone Swift probes build with
  `env -u TOOLCHAINS DEVELOPER_DIR=/Applications/Xcode-26.6.0.app/Contents/Developer xcrun swiftc -O x.swift -o x`.
- Python tools: stdlib first; `uv run` with PEP 723 headers when a dependency is needed.

## `private/` layout

| Path | What |
| --- | --- |
| `nexus-app/` | Nexus 2.5.8 x64 package, unpacked (`.sys` removed). |
| `nexus-app/HJC.GameSir.Nexus2_0.dll` | Main app code, .NET Native (no CLR metadata), 67.6 MB. Holds the embedded resources. |
| `nexus-app/FirmwarePackages/JL_Upgrade_Gip.dll` | JieLi GIP upgrade library (native). |
| `nexus-app/D4XGaming.Devices.dll` / `.winmd` | GIP device access layer. |
| `nexus-app/Web/js/app.*.js` | Vue UI (3.2 MB, minified). Talks to native code over `chrome.webview.postMessage({action, payload})`. |
| `nexus-localstate/` | Nexus log from the 2026-09-26 update session, plus `Settings/`. |
| `firmware/Core/FirmwarePackages/<Model>/*.ufw` | 33 images carved by `tools/extract_nexus_resources.py`. |
| `firmware/Core/Devices/Unsupported/unsupported_devices.json` | Nexus's list of unsupported devices and PIDs. |
| `descriptors/` | G7 SE 1082 HID report descriptors (if0, if1) and the configuration descriptor, binary. |
| `captures/` | Windows USB capture of the G7 SE without Nexus, plus `pcap.py`. |
| `probes/` | macOS IOUSBHost/IOHID probes used on the G7 SE (read the source before running). |

Rebuild `firmware/` with `python3 tools/extract_nexus_resources.py`.

## Open tasks, most useful first

1. **Nexus command builders.** The UI sends `setConfig`, `saveProfile` and `commonReq` JSON (see
   the UI bundle). Native code turns it into GIP message 0x0F vendor commands. Find, for the
   G7 SE: the LED on/off (0D), status (E0), profile read/write (04/05, 0B/0C, 07) and the
   APP-mode switch (to PID 1010). Output: the exact byte layouts. Lead: the .NET Native DLL keeps
   strings and type names; search near the command-name strings, then follow the xrefs in
   Ghidra (x64, PE).
2. **Mode switching on macOS.** The pad enumerates as 1082 (HID) with a vendor interface 1
   (reports 0x0F out, 0x10/0x12 in) that did not answer G7 Pro framing. Find in firmware or
   Nexus what 1082's interface 1 accepts, and how the pad enters XInput 1022 / APP 1010.
3. **UFW container.** Parse the cleartext JieLi header (`PB01_00_0`, `UPDATE_JUMP`, file table)
   of `G7SE/JS_SL3101_V664_*.ufw`. Map the sections and their lengths and CRCs. Search for public
   JieLi AC69xx firmware tooling that documents the format and the CRC16-keyed cipher (none
   checked yet). Verify any tool claim against these files.
4. **Decrypt.** The bodies are ciphertext (entropy about 8.0 bits/byte). Key and No_Key differ
   after 0x1E27. Try the known JieLi cipher (CRC16-based keystream, keyed by chip key or none).
   `No_Key` is the likeliest to open with a fixed or zero key. `JL_Upgrade_Gip.dll` exports
   `JL_getFirmwareDataCRC`, `JL_getFirmwareOrigCRC`, `JL_getFirmwarePidVid`,
   `JL_loadFirmwareData` and `JL_upgradeDevice*`; reverse `JL_loadFirmwareData` and the
   "firmware KEY does not match the device KEY" check to learn how it reads the header and key.
5. **Firmware handlers.** Once plaintext exists: find the USB descriptors (PID 1082, 1022, 1010),
   the interface-1 report handler, the LED driver and the combo-button table. The AC695X CPU is
   believed to be JieLi's own ISA (unverified), so find a Ghidra or IDA processor module first.
6. **Nexus log.** `nexus-localstate/.../Log - 20260926.log` records a full G7 SE update (PID 1010,
   `SingleUBoot`, `Key` variant). Extract the command and response sequence if it is logged.
