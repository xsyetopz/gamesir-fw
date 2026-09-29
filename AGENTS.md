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
- The tool is the Rust workspace in `crates/` (`gsfw-core`, the `gsfw` CLI, the `gsfw-gui`
  window). `just check` is the gate; it must pass. All lints are `forbid`: fix the
  code, never add `#[allow]`. `lib.rs`, `main.rs` and `mod.rs` hold only declarations; unit tests
  go in `<module>/tests.rs`. A new operation goes into `gsfw_core::ops` so the CLI and the GUI
  offer it both.
- Code comments, doc comments, and user-facing docs (`README.md`, `ARCHITECTURE.md`, `docs/`) follow ASD-STE100.
- `CLAUDE.md` and `GEMINI.md` are symlinks to this file. Edit `AGENTS.md`.

## Commands

Run from the repository root. `rust-toolchain.toml` pins Rust 1.98.1. `just check` also needs
`cargo-deny` 0.20.2 on `PATH`.

| Command | What |
| --- | --- |
| `just setup` | Install the pinned toolchain. |
| `just fmt` | Format. The gate fails on a format diff. |
| `just lint` | Clippy on the code, then on the tests with `.clippy-test/clippy.toml`. Fast lane during edits. |
| `just host` | Format check, clippy, tests, docs, release build. |
| `just check` | Format, then `host`, `no-firmware` and cargo-deny: the full gate. |
| `just no-firmware` | No firmware image and no file over 1 MB in the tree (CI runs it). |
| `cargo run -p gsfw -- --help` | CLI commands. `list`, `probe`, `crc` and `flash` touch a pad. |
| `cargo run -p gsfw-gui` | The window. |

The `justfile` holds the cargo commands of each recipe (`just --list`).

Done means `just check` passes. CI (`.github/workflows/check.yml`) runs the same cargo
commands: all of them on Linux, all except cargo-deny on macOS and Windows.

## Layout

| Path | What |
| --- | --- |
| `crates/gsfw-core/src/formats/` | UFW, `.fw`, Nexus DLL and Flash Tool decoders. |
| `crates/gsfw-core/src/jieli/`, `src/usb.rs` | GIP upgrade protocol, and its nusb link. |
| `crates/gsfw-core/src/app/`, `src/ops` | Use cases (fetch, session, flash) and the operations the CLI and GUI share. |
| `crates/gsfw-core/catalog/catalog.json` | Downloads that `fetch` knows, with SHA-256 sums. No firmware bytes. |
| `ARCHITECTURE.md` | Components, layers, invariant commands, decisions. |
| `docs/` | User-facing docs (GIP protocol, recovery). |
| `notes/` | Research notes. `facts.md` holds verified facts. |

## `private/` layout

| Path | What |
| --- | --- |
| `nexus-app/` | Nexus 2.5.8 x64 package, unpacked (`.sys` removed). |
| `nexus-app/HJC.GameSir.Nexus2_0.dll` | Main app code, .NET Native (no CLR metadata), 67.6 MB. Holds the embedded resources. |
| `nexus-app/FirmwarePackages/JL_Upgrade_Gip.dll` | JieLi GIP upgrade library (native). |
| `nexus-app/D4XGaming.Devices.dll` / `.winmd` | GIP device access layer. |
| `nexus-app/Web/js/app.*.js` | Vue UI (3.2 MB, minified). Talks to native code over `chrome.webview.postMessage({action, payload})`. |
| `nexus-localstate/` | Nexus log from the 2026-09-26 update session, plus `Settings/`. |
| `firmware/Core/FirmwarePackages/<Model>/*.ufw` | 33 images carved by `gsfw nexus-extract`. |
| `firmware/Core/Devices/Unsupported/unsupported_devices.json` | Nexus's list of unsupported devices and PIDs. |
| `descriptors/` | G7 SE HID report descriptors (1082 if0 and if1, 1010 if0) and the 1082 configuration descriptor, binary. |
| `captures/` | Windows USB capture of the G7 SE without Nexus (`pcap.py` parses it), plus macOS G7 SE input traces (`g7*.raw`, `g7*.log`). |
| `reference/` | Copies of g7ctl's G7 Pro protocol notes and code, the GLT research notes, and gamesir-wiki's controller table. |
| `probes/` | macOS IOUSBHost/IOHID probes used on the G7 SE (read the source before running). |

Rebuild `firmware/` with `cargo run -p gsfw -- nexus-extract private/nexus-app/HJC.GameSir.Nexus2_0.dll private/firmware`.

## Open tasks, most useful first

Done (see the notes): Nexus command layouts for the G7 SE (`notes/protocol-g7se.md`); the UFW
container, cipher and chip keys (`crates/gsfw-core/src/formats/ufw.rs`, `notes/facts.md` "UFW
decryption"); the data-only firmware map (`notes/firmware-g7se.md`); the Nexus log sequence
(`notes/facts.md`); the G7 SE recovery by flash (`docs/jieli-gip-protocol.md`).

1. **Talk to the vendor collection on macOS.** Output report 0x0F gets no reply on 0x10/0x12,
   either with g7ctl framing or with the bare Nexus payload (`0f 09`, T12, pad in 1010). Blocks
   profile read/write (T13, T14, T17). Leads: the 1010 feature reports 3 and 0xE0 (a GET stalls);
   a Windows USBPcap capture of Nexus reading a profile, to see the wire framing; raw EP 0x02
   writes, which need the interface taken from Apple's HID driver (DriverKit or Linux).
2. **Mode switch without the combo (T15, T16).** No interface-1 command was found in the
   firmware data. The Nexus route is the XInput rumble spelling `gamesirapp`; on 1082 the
   only rumble-like output is if0 report 5 (4 bytes). T15 sent the pattern in both byte orders
   through the HID API with A pressed, and the motors never buzzed, so report 5 does not reach
   the motors that way. The firmware decoder may listen only in XInput mode (10A0), which
   macOS does not trigger. Needs an XInput host (Windows or Linux `xpad`).
3. **Disassemble the firmware.** T9: pi32v2, pi32 and q32s all fail the 256-instruction check
   (`notes/jieli-tooling.md`). A correct AC695X module would unlock the command dispatch,
   combo table and LED driver (anchors in `notes/firmware-g7se.md`).
