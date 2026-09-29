# Public JieLi tooling (T5 survey, 2026-09-28)

Found by a web survey. Each claim is the source's own words or constants, read in its code or
docs. Nothing below is checked against our `.ufw` files until T7-T9 say so.

## 1. kagaimiq/jl-misctools (`firmware/fwunpack_newfw.py`, `firmware/jltech/`)

- URL: <https://github.com/kagaimiq/jl-misctools>
- Claim, UFW header:
  - The first 0x40 bytes are descrambled with the ENC cipher, key 0xFFFF.
  - The header is `<HHIHHI48s`: hdrcrc, listcrc, imgsize, numents, ?, ?, chipname[48].
  - hdrcrc = crc16(header[2:0x40]), computed after descrambling.
- Claim, entry table:
  - It is `numents` entries of 0x50 bytes, starting at 0x40. listcrc is the crc16 of the
    table before descrambling.
  - Each entry is descrambled on its own with key 0xFFFF, and is `<HHHHIII44s16s`: type,
    index, data_crc, ?, offset, size, size2, ?[44], name[16].
  - Type 0 is the flash image. Its data_crc = crc16(data).
- Claim, CRC and flash cipher:
  - crc16 is poly 0x1021, init 0x0000, not reflected. crc32 is poly 0x04C11DB7, init
    0x26536734, reflected.
  - Flash data is decrypted with `jl_sfc_cipher`: 32-byte blocks, each ENC-ciphered with key
    `chipkey ^ ((off - base) >> 2)`.
  - The chip key comes from `isd_config.ini` in newer images. `bruteforce.py` recovers a chip
    key from known plaintext.
- License: MIT
- checked against our files: no

## 2. kagaimiq/jielie (`misc/cipher.md`, `datafmt/newfw.md`, `datafmt/jlfs.md`, `datafmt/sdfile.md`, `cpu/pi32v2.md`)

- URL: <https://github.com/kagaimiq/jielie>
- Claim, ENC cipher:
  - Per byte: `data ^= key` (low byte), then `key = (key << 1) ^ (0x1021 if key bit 15 else 0)`.
- Claim, CrcDecode cipher:
  - The key is always 0xFFFFFFFF. It is used for loader blobs and the chip-key reply.
  - It is a CRC16 stream over a 16-byte magic `C3 CF C0 E8 CE D2 B0 AE C4 E3 A3 AC D3 F1 C1 D6`.
- Claim, formats:
  - JLFS header (fields: hdr_crc, burner_size, vid[4], flash_size, fs_ver, block_align,
    pid[16]) and 32-byte file entries.
  - New-format layout: `uboot.boot`, `isd_config.ini`, then `app_dir_head`, which holds
    app.bin, cfg_tool.bin, VM and more.
  - The older SDFILE header for AC69xx.
- Claim, CPU:
  - pi32v2 has ELF machine 0xF1, is little-endian and has 16 GPRs.
- License: none stated
- checked against our files: no

## 3. kagaimiq/ghidra-jieli

- URL: <https://github.com/kagaimiq/ghidra-jieli>
- Claim: a Ghidra processor module with `pi32.slaspec`, `pi32v2.slaspec` and `JieLi.opinion`.
  - The survey found no IDA module.
  - Guess, not checked: the AC695X is BR23 family, with a pi32v2 core.
- License: Apache-2.0
- checked against our files: yes (T9, 2026-09-28)
  - Setup: cloned to `private/re/ghidra-jieli`, compiled with Ghidra 12.1.3 `support/sleigh -a`,
    and linked as a user extension in `~/Library/ghidra/ghidra_12.1.3_PUBLIC/Extensions/`.
  - Test: a Ghidra headless script (removed 2026-09-29) disassembled G7SE v6.6.4 app.bin linearly from
    0x1e000c0 (first 256 decode attempts).

    | Language | Valid | Invalid | Targets outside image |
    | --- | --- | --- | --- |
    | `pi32v2` | 190 | 66 | 7 |
    | `pi32` | 187 | 69 | 10 |
    | `q32s` | 250 | 6 | 42 |

  - Many of q32s's outside targets are relative branches to about 0x1dff000, so a different
    base address would not fix them.
  - None passes. The README calls pi32v2 and q32s "very early stage".
  - Guess: the core is q32s-like, or the image starts with data rather than code.
  - Not tested: other entry points, or ROM-relative addresses.

## 4. kagaimiq/jl-uboot-tool (`jltech/cipher.py`, `jltech/crc.py`)

- URL: <https://github.com/kagaimiq/jl-uboot-tool>
- Claim:
  - It has the same ENC, CrcDecode and RxGp ciphers, and the same crc16.
  - It ships loader blobs for br23/br25.
  - The USB key for the ROM loader is 0x16EF. That is DFU entry, which is out of scope here.
- License: MIT
- checked against our files: no

## 5. kagaimiq/jl-misctools `keyfile/`

- URL: <https://github.com/kagaimiq/jl-misctools/tree/master/keyfile>
- Claim: `.key` files carry the chip key for chips whose key is programmed (not 0xFFFF).
  Example: "BACAA000" → chip key 0xBACA.
- License: MIT
- checked against our files: no

## 6. ElectronicCats/jieli-ble-badge-research (`tools/ufw-repack/`)

- URL: <https://github.com/ElectronicCats/jieli-ble-badge-research>
- Claim:
  - It vendors kagaimiq's `jltech`.
  - It decrypts flash.bin with the sfc cipher and the chip key.
  - It adds a vendor "Qix" wrapper, CRC-16/CCITT-FALSE (init 0xFFFF) over payload[27:].
  - The chip is AC707N/BR35, not ours.
- License: MIT
- checked against our files: no

## 7. For comparison: aeromodes/8cryptdo, fwupd/8bitdo-firmware

- URL: <https://github.com/aeromodes/8cryptdo>, <https://github.com/fwupd/8bitdo-firmware>
- Claim:
  - 8cryptdo covers 8BitDo GD32 `.dat` files: a 28-byte plaintext header, then a keyless
    chaining layer.
  - 8bitdo-firmware only archives releases.
  - Neither covers JieLi.
- License: MIT (8cryptdo), none stated (8bitdo-firmware)
- checked against our files: no

## Not found

- No public source defines the entry names `PB01_00_0`, `UPDATE_JUMP`, `EOFFSET` or
  `2_3_0_0`. Searched: grep of jl-misctools, jielie and ElectronicCats, plus one web search.
- No UFW magic is documented. kagaimiq recognises a UFW file by the header CRC after ENC with
  key 0xFFFF.
- Not reviewed: AC695X SDK copies (qqq5127/AC6951) and Jieli-Tech fw-Bootloader, both listed in
  jielie `links.md`.
