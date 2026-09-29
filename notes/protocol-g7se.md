# G7 SE vendor protocol, as Nexus 2.5.8 builds it

Source: `HJC.GameSir.Nexus2_0.dll` (image base 0x180000000), decompiled with
Ghidra headless; method names from the CoreRT reflection blobs (`notes/facts.md`, "Nexus method
map"). Both scripts were removed on 2026-09-29. RVAs below are
function starts. Byte values are the constants stored into the `byte[]` in each builder.

## Class chain

- `Core.Devices.G7.ProxyG7SE` (ctor 0x3f9a940) calls `Core.Devices.T7ProW.Proxy..ctor`, so the
  G7 SE uses the T7ProW proxy for all commands. Its `.cctor` (0x38eb700) registers device name
  "G7 SE", product type "G7SE", minimum version "6.4.5", and mode configs VID 0x3537 with PIDs
  0x1010, 0x1069, 0x1071, 0x1073, 0x1075, 0x1077, 0x108F, 0x106D ("G7 SE") and 0x1064, 0x1065,
  0x1079 ("G7 HE"), each with XInput PID 0x10A0; plus PID 0x1001 (minimum version "3.2.7").
  PID 0x1082 is not in this list.
- Profile commands come from `Core.Profile.Templates.T7ProW.Commander`, which extends
  `Core.Profile.Templates.C2.Commander`.

## Transport

Every command below is a `DeviceMessage` whose `MessageID` is 0x0F (GIP vendor message) and
whose `Data` is the byte array shown. `DeviceMessage.Build` (0x3b93380) stores the id at +0xE
and `ToArray(data)` at +0x10; `BuildWithoutAck` (0x3f98900) also clears NeedAck.
`CommandQueue.ProcessQueue` (0x3b6cbd0) hands `(MessageID, Data)` to
`D4XGaming.Devices.ID4XDevice.SendMessage` (0x3acffe0). The GIP header (type, flags, sequence,
length) is added by the Windows GIP driver: D4XGaming passes only class, id and data to `IGipGameControllerProvider` (see `notes/facts.md`). The payloads carry no checksum.

Replies are parsed by `C2.ProfilePacketInfo..ctor(byte[])` (0x3b9d180). It throws
"byte array length must be 60" unless the reply is 60 bytes. Layout:

| Byte | Field |
| --- | --- |
| 0 | reply type (unknown values map to 0xFFFF) |
| 1 | profile index (stored at +0x18) |
| 2, 3 | offset high, low |
| 4 | data length n; for type 0x0F, length is `b4 << 8 \| b5` |
| 5.. | data (from byte 6 for type 0x0F) |

`T7ProW.Proxy.<DealDeviceMessage>d__26.MoveNext` (0x3b94600) dispatches on the reply type:

- 0x05: profile data.
- 0x06: if byte 1 is 1, it logs "Device({0}) is busy now...".
- 0x0A: version.
- 0x0C: current profile.
- 0x0F and 0x11: notifications.
- All types except 0x0E and 0x0F are passed to `CommandQueue.ReceiveMessage` as acks.

`ProxyG7SE.<Device_MessageReceived>d__15.MoveNext` (0x3c031e0) routes these before the dispatch:

- A 60-byte message whose byte 0 is 0xE0 goes to `T7ProW.GamepadStateParseService.Update`.
- When the firmware is older than the minimum version, a 0x20-byte message goes to
  `XBoxXControllerKeyParser`.

## Commands (GIP message 0x0F payloads)

| Name | Bytes | Builder (RVA) | Reply |
| --- | --- | --- | --- |
| version | `09` | `T7ProW.Proxy.LoadFirmwareVersions` 0x3b93d60 | type 0x0A |
| current profile | `0B` | `T7ProW.Proxy.ReadCurrentProfile` 0x3b92c40 | type 0x0C, byte 1 = index |
| switch profile | `07 NN` | `T7ProW.Proxy.<SwitchProfile>d__34.MoveNext` 0x3c038c0 | ack |
| heartbeat | `F2 BB` | `T7ProW.Proxy.<Heartbeat>d__36.MoveNext` 0x3c04a90 | none (`ImmediateWithoutAck`) |
| read profile chunk | `04 II OH OL NN` | `C2.ProfilePacketInfo..ctor` 0x3b93b90, via `C2.Commander.GetReadProfileCommand` 0x3b93a30 | type 0x05 |
| write profile chunk | `03 II OH OL NN data[NN]`, zero-padded to 60 bytes | `C2.Commander.GetWriteProfileCommand` 0x3cd1fc0 | ack |

### Version (`09`, reply `0A`)

The reply handler does the following:

1. It drops byte 0 of the raw reply.
2. It keeps only the nonzero bytes.
3. It formats the first 4 as the firmware version and the next 4 as a second version, with
   `FormatVersion` (0x3b94ba0). `FormatVersion` decodes the bytes, trims `'0'` and joins with
   `"."`.

The last step is a guess from the call shape; the log shows "6.6.4" as `firmwareVersion`.

### Current profile (`0B`, reply `0C`)

`ProxyG7SE.<DealReadCurrentProfileAck>d__13.MoveNext` (0x3c03380) reads the current index from
raw byte 1.

### Switch profile (`07 NN`)

`NN` is set as follows:

- 5 if the UI names "Shift";
- 0x20 if it names "Light";
- otherwise the integer the UI sends.

The strings are .NET literals at 0x183731678 and 0x18378d628. The log names profiles 1-4 and
"ProfileShift" = index 5.

### Heartbeat (`F2 BB`)

`BB` is a bool read from `proxy+0x38 -> +8`. Its meaning is unknown (guess: "state reports on").

### Read profile (`04 II OH OL NN`, reply `05`)

The builder works as follows:

- `C2.Commander.GetReadProfileCommand(idx)` (0x3b939f0) calls the virtual read over
  `[0, length-1]` with command type 4.
- `T7ProW.Commander.GetReadProfileCommand` (0x3f70610) adds `ProfileLightOffset` to both ends
  when `idx == 0x20`. The offset is 0xEAB, set in `T7ProW.Commander..ctor` 0x3fc08e0.
- The range is split into chunks of 55 (0x37) bytes. Each chunk sends `04, idx, off>>8,
  off&0xFF, n` with n = min(55, remaining), and `off` advances by 55.
- The chunk count is `(int)f(len/55.0)`, where `f` is an imported Math call. It is taken as
  ceiling because floor would drop the tail (inference).

Profile lengths come from `T7ProW.Commander.GetProfilePacketLength` (0x3f70640):

- For idx 1-5, the length is `K*7 + 0x154`, with `K` = the length of the 20-name array that
  `C2.BasicProfileInfo..cctor` (0x38e72e0) stores in the static at RVA 0x3711048: Up, Under,
  Left, Right, L1, R1, L3, R3, CRO, CIR, SQU, TRI, PS, Sel, Sta, Camera, FL1, FL2, FR1, FR2.
  So the length is 480, which matches the log's 480-byte payloads. That makes 9 chunks: 8 x 55 + 40.
- For idx 0x20, the length is `(M+1)*3`, with `M` = 6: the array that
  `T7ProW.T7ProWRgbProfileInfo..cctor` (0x38e5640) stores in the static at RVA 0x3711060 holds
  Animation, A, B, X, Y and DPad. So the light profile is 21 bytes: one chunk `04 20 0e ab 15`.

### Light profile layout (idx 0x20, 21 bytes)

From `T7ProWRgbProfileInfo.Parse` (0x3b9b860):

- bytes 0, 1 and 2 are read one byte at a time into three fields at object offsets
  0x30/0x32/0x34 (names not recovered; guess: mode, brightness, speed);
- then there are 3 bytes each for Animation, A, B, X, Y and DPad, in that order (guess: R, G, B).

### Write profile (`03 ...`)

`GetWriteProfileCommand(idx, data, start)` (0x3cd1fc0) allocates 60 bytes per chunk and fills
them as follows:

- bytes 0-4: `03, idx, start>>8, start&0xFF, n`, with n = min(55, remaining);
- from byte 5: `data[consumed : consumed+n]`;
- the rest stays zero.

`T7ProW.Proxy.SaveProfile` (0x3f71a90) queues these through `CommandQueue.AddRange` and sends no
trailing save or switch command.

## Listed commands that Nexus does not send to a G7 SE

- **0D light on/off.** Only `K1.Proxy.setLightStatus` (0x3f83d00) builds `0D, on?1:0, x`.
  `x` is the byte at +0xC of the deserialized light-state JSON object; its field name is not
  mapped. That message is sent immediately, without an ack. `DeviceProxy.setLightStatus`
  (0x3fedd78) is an empty body shared with other no-op methods, and the T7ProW and G7 classes
  have no override.
  - Searched: the method map for `setLightStatus` and `Light`, and every direct call to
    `CommandQueue.Add`, `AddRange`, `DeviceMessage.Build`, `BuildWithoutAck`,
    `ImmediateWithoutAck`, `Immediate`, `Raw` and `ID4XDevice.SendMessage` in `.text`
    (`private/re/nexus/callers.py`).
  - None of the callers is a T7ProW or G7 method that stores 0x0D.
  - The UI strings `setLight`/`LightStatus` occur once (UTF-16 at file offset 0x37ca0ac).
- **E0 status.** Nexus sends no E0 request. E0 is the first byte of the 60-byte
  gamepad-state input the pad pushes (0x3c031e0, `== -0x20`). The same caller search found no
  builder that stores 0xE0.
- **05 / 0C as requests.** These are reply types, not requests (0x3b94600).

## Mode switch: found, over XInput rumble, not over GIP 0x0F

Nexus has no GIP or HID command that changes the USB mode. Searched: method names and UTF-16
strings `SwitchMode`, `ModeSwitch`, `UsbMode`, `ChangeMode`, `EnterApp`, `APP mode`, `PcMode`
(no hits). Instead, `Core.Services.XInputDeviceService` does the following:

1. `<Gamepad_GamepadAdded>d__24.MoveNext` (0x3c61980) checks the new pad.
   - If a `Windows.Gaming.Input` pad is not a D4X (GIP) device and `IsAConfiguredDevice`
     matches it, the handler waits 150 ms, remembers the pad and starts a timer.
   - It also tells the UI to show `XInputDeviceTips`: "GameSir device detected, please press
     any key to switch to APP mode".
   - A pad counts as configured when its PID is a mode config's `XInputPid`, 0x10A0 for the
     G7 SE (inference from `.cctor` 0x38eb700).
2. `<DispatcherTimer_Tick>d__27.MoveNext` (0x3c62bd0) calls `Vibrate(gamepad, left, right)` five
   times. The argument pairs are immediates in `mov r8b/r9b` before each call:

   | Call site | left | right | ASCII |
   | --- | --- | --- | --- |
   | 0x3c62eeb | 0x67 | 0x61 | `g a` |
   | 0x3c62c3e | 0x6D | 0x65 | `m e` |
   | 0x3c62cb3 | 0x73 | 0x69 | `s i` |
   | 0x3c62d28 | 0x72 | 0x61 | `r a` |
   | 0x3c62d9e | 0x70 | 0x70 | `p p` |

   Together the pairs spell "gamesirapp". Next it waits 500 ms and re-arms the timer, so the
   sequence repeats while the pad stays in XInput mode.
3. `<Vibrate>d__28.MoveNext` (0x3c5e7f0) sends each pair as one rumble step:
   - It sets `Gamepad.Vibration` to `{LeftMotor = left/255.0, RightMotor = right/255.0,
     LeftTrigger = 0, RightTrigger = 0}`. The divisor is the double 255.0 at RVA 0x2e5020, used
     by `GetVibrationData` (0x3c5ea00).
   - It waits 15 ms, sets all four motors to 0 and waits 5 ms.

The log fits this reading (`facts.md`, "Nexus log sequence"):

- the tip is logged at line 525;
- 10A0 is removed at line 528;
- 1010 is added at line 531.

The prompt suggests that the pad treats the rumble pattern as an unlock, and that a button press
finishes the switch (inference). On macOS, this needs raw XInput rumble to PID 0x10A0. The
Windows.Gaming.Input motor floats become the device's rumble bytes by driver scaling that is
not in this DLL, so the on-wire bytes are unverified.

## GIP rumble (message 0x09), for reference

`C2.Commander.GetVibrationCommand` (0x3f719b0), sent by `T7ProW.Proxy.Vibrate` (0x3f718c0) as
MessageID 9, builds `00 0F LT RT L R D 00 00`:

- `LT` and `RT` are round(trigger * 0.8), with the double 0.8 at 0x2ea0f0;
- `D` is duration / 10;
- mode 1 drives all motors, mode 2 only the triggers, mode 3 only the main motors.
