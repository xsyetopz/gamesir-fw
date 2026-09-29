//! `JieLi` GIP upgrade protocol (`JL_Upgrade_Gip.dll`), pure logic, no USB.
//!
//! Sources: RVAs into `JL_Upgrade_Gip.dll` (decompile in `private/re/gip-upgrade/all.c`) and
//! `private/re/gip-upgrade/findings.md`. A command is a 512-byte JSUD packet, ENC-scrambled with
//! the session key, cut into ten `4A 0A idx len data[52]` fragments. Each fragment goes out as a
//! GIP vendor message 0x0F whose payload is `F0 00 00 00` + the 56-byte fragment (Nexus
//! `SendToDevice`). Replies come back as `F1 ?? ?? ??` + `4A total idx len data`.
//!
//! Guesses, not read from code: the GIP header flags (0x00, from g7ctl's raw `0f 00 seq len`)
//! and the reply prefix bytes after F1.

pub mod commands;
mod error;
pub mod flash_head;
pub mod gip;
pub mod packet;
pub mod plan;

pub use commands::{
    BLOCK, C0_HANDSHAKE, C1_QUERY, C2_RESULT, C3_ERASE, C4_WRITE, C5_CRC, CA_RESET, CB_RANGE,
    MAX_WRITE, PAGE, RAND_LEN, REPLY_HANDSHAKE, REPLY_STATUS, SECTOR, STATUS_KEY_MISMATCH,
    STATUS_OK, Status, c0_handshake, c1_query, c2_result, c3_erase, c4_write, c5_crc, ca_reset,
    cb_range, parse_c1, parse_crc, parse_result, parse_status, session_key,
};
pub use error::ProtocolError;
pub use flash_head::{DirEntry, ERASED, FLASH_HEAD_KEY, FlashHead, HEAD_LEN, parse_flash_head};
pub use gip::{
    ACK_CMD, FRAG_CMD, FRAG_DATA, FRAG_LEN, FRAG_TOTAL, GIP_FLAGS, GIP_NEEDS_ACK, GIP_POWER_ON,
    GIP_VENDOR_CMD, OUT_PREFIX, REPLY_MARK, Reassembler, fragments, gip_ack, gip_unwrap, gip_wrap,
    is_ack, reply_fragment,
};
pub use packet::{
    BODY_AT, MAGIC, PACKET_LEN, SDK_ID, TOOL_ID, build, find_key, new_tag, tag_of, unscramble,
};
pub use plan::{
    EOFFSET_SHIFT, FlashPlan, KILL_LEN, eoffset_shift, erase_steps, plan_flash, write_chunks,
};
