# Community GameSir firmware tool: architecture

Draft, 2026-09-29. The tool is the Rust workspace in `crates/` of this repository, released under
The Unlicense. `python3 tools/check.py` is the gate.

## Goal

Anyone with a GameSir pad on macOS, Linux (including Steam Deck) or Windows can:

1. Get the firmware their pad needs, including older versions GameSir does not offer.
2. Flash it to recover a pad, as done for the G7 SE (6.64 back to 6.40, `notes/facts.md`, "Live
   runs").
3. Decode firmware and chip files for research: containers, entries, `isd_config`, the app image.

## What exists (source of truth today)

| Piece | Where | State |
| --- | --- | --- |
| UFW/`.fw` container decode, chip key, app decrypt | `gsfw-core` `formats` | Tested on the Nexus 6.64 and the 6.40 G7 SE images. |
| Nexus resource carving | `gsfw-core` `formats::nexus` | Output from Nexus 2.5.8 x64 equals the earlier Python carve (34 files). |
| JieLi GIP upgrade protocol (pure) | `gsfw-core` `jieli` | Tested. Planning covers mode 1/2 with region A kept. |
| Upgrade session and flash use cases | `gsfw-core` `app` | Ported from the Python tool that flashed a G7 SE on Windows 10 (WinUSB). Simulator tests cover the 6.40-over-6.64 flash. |
| USB transport (`nusb`) | `gsfw-core` `usb` | `list` checked on macOS. No upgrade command sent from the Rust port yet. |
| Operations shared by both front ends | `gsfw-core` `ops` | |
| CLI | `gsfw` | Hand-written argument parser. |
| GUI | `gsfw-gui` (egui) | Builds and opens on macOS. |
| Flash Tool `.exe` carving | `gsfw-core` `formats::flashtool` | Output of the 6.40 G7 SE tool equals the hand-carved files. Eight more GameSir tools carve, and their files match their CRC-16 values. |
| Firmware catalog | `crates/gsfw-core/catalog/catalog.json`, `gsfw-core` `catalog` | Nine GameSir Flash Tools. Eight have a GameSir URL. The 6.40 G7 SE tool has none. |
| Fetch use case, HTTP adapter (`ureq`, `rustls`) | `gsfw-core` `app::fetch`, `net` | `fetch --from` checked with the 6.40 G7 SE tool. Tested against a local HTTP server. On 2026-09-29, `fetch` downloaded the eight tools that have a URL, and each passed its catalog sums. |

Firmware comes from two kinds of artifacts GameSir publishes:

- **Nexus package:** only the current image of each model, embedded in
  `HJC.GameSir.Nexus2_0.dll`.
- **Per-model "Flash Tool" `.exe`:** older versions, in a UPX-packed overlay (for example "G7 SE/HE
  6.40 Flash Tool").

## Quality scenarios

Each scenario has a test or a measurement. A pad simulator (the transport test double) makes the
flash scenarios testable without hardware.

| # | Scenario | Measure | Check |
| --- | --- | --- | --- |
| Q1 | The USB link drops at any packet during `flash`. | The pad still has one bank whose dir head is intact: the old bank until region C is verified, the new one after. | Simulator: a guard checks the pad before every C3 and C4 of every simulated flash. A drop stops all later packets, so this covers a drop at every packet. `a_link_drop_is_reported` drops the link at spaced points and at the last 8 calls, and expects an error each time. |
| Q2 | The user selects an image whose layout, chip key or region A does not fit the pad. | Zero C3/C4 packets are sent. | Simulator test for each refusal: layout, chip key status -3, region A, mode 0. |
| Q3 | A downloaded or carved artifact differs from the catalog. | It is never written or used: the sha256 must match. | Unit tests: a tampered download, a tampered `--from` file, and an image with another sha256. |
| Q4 | Someone adds a firmware image to the repository. | CI fails. | `tools/check_no_firmware.py`, in CI on each OS. It looks at the files that git tracks or does not ignore. It fails on `*.ufw`, `*.fw`, `*.bin`, and any file over 1 MB. |
| Q5 | The same core runs on macOS, Linux and Windows. | The unit and simulator tests pass on all three. | CI matrix in `.github/workflows/check.yml`: `ubuntu-latest` runs the full gate, `macos-latest` and `windows-latest` run `tools/check.py host`. Not run yet: the repository has no remote. |
| Q6 | A known-good recovery is repeated. | The plan (addresses, lengths, CRCs) equals the recorded G7 SE run: C at 0x3000, 0x2d000 bytes, CRC 0xd820, kill at 0x3f000. | Fixture test. It skips when the user has no local image, and the hashes are in the catalog. |
| Q7 | A new model is added. | Only a catalog entry and a test change. The core code changes only when the protocol differs. | Review rule. The catalog tests (`catalog/tests.rs`) check the schema of the built-in catalog. |

The flash duration is not a requirement yet. Record it from the next run (the 6.40 log has no
timings).

## Decisions

### D1. Rust workspace: a core library, a CLI and an egui window over the same operations

- **Context:** the first port was Python. The owner chose Rust with a strict lint policy
  (`xsyetopz/rust-template`), one binary per OS, and a CLI and a GUI that offer the same things.
- **Decision:**
  - `gsfw-core` holds everything: formats, protocol, use cases, the USB link (`nusb`, pure Rust,
    no libusb) and `ops`. `ops::Op` names each operation and `ops::run` runs it, writing lines
    to a sink. The CLI and the GUI only build an `Op` and show the lines, so they cannot drift.
  - `gsfw` parses arguments by hand. `clap`'s derive emits `#[allow]`, which the `forbid` lints
    reject, and its builder exceeds the stack-frame limit.
  - `gsfw-gui` uses `eframe` (glow backend). It is the only package with the egui profile's
    exceptions. The core and the CLI keep the full policy.
- **Alternatives:**
  - Python with `pyusb` and tkinter. Replaced: it needs a Python install and libusb on every
    host.
  - `rusb`. Rejected: it links libusb. `nusb` reaches the same endpoints without a C library.
- **Consequences:**
  - Windows users still bind WinUSB with Zadig. Linux users need a udev rule.
  - The window needs X11 or Wayland libraries on Linux. Steam Deck desktop mode has them
    (guess, not tried).

### D2. One process, modules plus two ports, no service and no queue

- **Context:** a user runs one command against one pad. Nothing outlives the process.
- **Decision:**
  - **Modules:**
    - `formats` (containers, chip key, `isd_config`, app decrypt)
    - `jieli` (packets, fragments, flash planning)
    - `catalog` (device and artifact table)
    - `app` (fetch, decode, flash use cases)
    - `ui` (CLI, GUI)
  - **Ports:**
    - `GipLink`: `send(packet)`, `reply()`, `drain()`. It has two implementations today: the
      `nusb` link and the pad simulator in `crates/gsfw-core/tests/sim_flash.rs`.
    - `Source`: `get(url, limit) -> bytes`. It has two implementations today: the `ureq`
      adapter in `net` and a fixed table in the fetch tests. A file that the user gives
      (`--from`) does not go through the port.
    - Nexus and Flash Tool carving are `formats` functions applied to the fetched file, not
      sources.
- **Alternatives:**
  - A firmware web service. Rejected: there is no independent deployment need, and it would host
    the images (see D4).
  - A port per model. Rejected: every model seen so far uses the same JieLi protocol. A
    difference goes in the catalog until a second protocol exists.
- **Consequences:** every `GipLink` implementation passes the same contract tests: fragment acks,
  reply reassembly, stale-reply skip by tag.

### D3. The catalog is data, reviewed by pull request

- **Context:** the community adds models and versions. Code review is the only authority.
- **Decision:**
  - `crates/gsfw-core/catalog/catalog.json` is the single source of truth. Only pull requests
    write it. The build puts it into the program.
  - Each artifact has an id, the model, the version, the file name, the length, the sha256, the
    publisher URLs, the mirror URLs, and the path and sha256 of each image in it.
  - All artifacts are Flash Tool `.exe` files. A Nexus artifact needs a `kind` field and the
    Nexus carve step.
- **Consequences:**
  - `fetch` writes the artifact and its files into the output directory that the user gives.
    There is no cache.
  - Not done: VID/PID per mode, chip, and `verified` runs in the catalog, and the `flash`
    warning for a model and version with no `verified` run.

### D4. Fetch from GameSir first, with a community mirror as the fallback

- **Context:**
  - The images are GameSir's copyrighted binaries. The Unlicense covers only the project's own
    code, docs and catalog.
  - GameSir offers only the current image of each model in Nexus. Older images, which recovery
    needs, exist only in per-model Flash Tools that can disappear.
- **Decision (owner, 2026-09-29):**
  - The tool downloads the publisher's artifact (Nexus package, Flash Tool `.exe`) first, checks
    its sha256 against the catalog, and carves the images locally.
  - When the publisher's copy is gone, the tool falls back to a community mirror. The catalog
    sha256 proves that a mirrored copy is the original.
  - The mirror lives in a separate repository or store, not in the tool repository, so a takedown
    of the mirror does not take the tool down.
  - If GameSir asks for the removal of a mirrored file, the project removes it.
  - Users can also give a file they already have.
- **Alternatives:**
  - Only the publisher's artifacts. Rejected: recovery fails when GameSir removes an old tool.
  - Images in the tool repository or its releases. Rejected: a takedown hits the tool itself.
- **Consequences:**
  - `fetch` tries each publisher URL, then each mirror URL, and uses the first body with the
    catalog sha256. `--from FILE` uses the file and no URL.
  - A download stops after the catalog length.
  - The catalog has GameSir URLs for eight tools and no mirror URLs. The owner adds the mirror
    URLs.
  - Q4 keeps images out of the tool repository.

### D5. Flash safety rules live in `jieli` planning, not in the UI

- **Decision:**
  - `plan_flash` refuses when the layout, EOFFSET or mode does not fit.
  - Region C goes to the idle bank and is verified before the running bank's head is zeroed.
  - Region A is never written until the DLL's B/A path is implemented and passes Q1 on the
    simulator.
  - Checks-only is the default. Writing needs an explicit confirmation. In the CLI, it is
    `--yes`. In the GUI, it is the write checkbox and then a dialog that names the image and the
    pad's PID.
- **Alternatives:** follow the DLL and rewrite region A on mismatch. Rejected: it is not needed
  for the proven recovery, and a failure there can leave no bootable bank.

## Layers

Crates: `gsfw` and `gsfw-gui` depend on `gsfw-core`. `gsfw-core` knows neither. Inside
`gsfw-core` the modules import only downward:

| Module | May import |
| --- | --- |
| `bytes` | nothing |
| `formats` | `bytes` |
| `jieli` | `bytes`, `formats` |
| `catalog` | `formats` |
| `app` | `bytes`, `catalog`, `formats`, `jieli` |
| `usb` | `app`, `bytes`, `jieli` |
| `net` | `app` |
| `ops` | all of the above |

- `app` owns the `GipLink` and `Source` ports. `usb` and the test simulator implement
  `GipLink`. `net` and a test table implement `Source`.
- `formats` and `jieli` are pure and need no USB.
- No tool checks the module direction. Review keeps it. Cargo enforces the crate direction.

## Risks

- **Reaching the upgrade protocol:** a pad must enumerate with the GIP interface.
  - On macOS, 6.64 came up only as HID `1082`, so the recovery needed Windows. On 6.40, macOS
    `gsfw probe` and `gsfw crc` worked (2026-09-29). Nobody ran `flash` there.
  - On Linux (xpad) it has not been tried.
  - Documenting how a pad in the broken state reaches GIP on each OS is the main open question.
- **Other models:** only the G7 SE is proven. Models with other erase units or EOFFSET 0x10 hit
  planning paths that no live run has checked.
- **Chip key:** C1 needs the `_Key` image's key. Pads that need `_No_Key` have not been seen.

## Decided (owner, 2026-09-29)

1. **Hosting firmware images:** both, see D4. Takedown requests are honoured.
2. **Licence for the project's own work:** The Unlicense (`LICENSE`).
3. **Repository:** this repository, restructured as the `gsfw` Rust workspace. `private/` stays
   gitignored and is never published.
