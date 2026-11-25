//! AJAZZ "AA" family: AK980 PRO on firmware 769 (USB 0C45:8039) and
//! relatives.
//!
//! Wire format (protocol notes `protokol-aa.md` §2):
//! * Commands are 64-byte HID **output** reports on the FF68 collection,
//!   written as 65 bytes with a leading `00` report ID: an 8-byte header
//!   `AA CMD LEN ADDR_LO ADDR_HI 00 LAST 00` and 56 bytes of data.
//! * Every request is answered by a 64-byte input report `55 CMD LEN ADDR…`
//!   carrying the data of a read, or echoing the data of a write.
//! * Blocks longer than 56 bytes travel in 56-byte chunks: ADDR is the
//!   chunk's byte offset, LEN its length, LAST is set on the final chunk
//!   (as AJAZZ's tools do; firmware 769 never looks at it).
//! * The screen takes 4104-byte packets on FF67 ([`screen`]).
//!
//! What firmware 769 does with them (protocol notes §F, from its code):
//! the settings live straight in flash, in 1 KB pages, and a write chunk
//! erases its page only when it starts on the page's first byte; every
//! other chunk is programmed over what is there. So a table is written
//! once, from address 0 upwards, with a chunk on every page boundary
//! ([`frame::paged_chunks`]), never patched in the middle. Nothing bounds
//! ADDR or LEN on the keyboard: a write past its table erases firmware
//! code, so [`guard::guard`] keeps every write inside its table. A write's
//! reply echoes the request even when the keyboard dropped the write (as
//! it does during its power-on animation and its reset blink): only a
//! read-back tells it landed.
//!
//! There are no sessions, no checksums and no feature reports. Everything
//! here is pure: the driver in kb-core does the I/O and passes every packet
//! through [`guard::guard`] first.
//!
//! The domain types (effects, key actions, macro steps, screen animations,
//! clock) are the "04 xx" family's, from [`crate::sonix`]: the app speaks
//! one language for both families, and only the bytes differ.

pub mod clock;
pub mod effect;
pub mod frame;
pub mod guard;
pub mod info;
pub mod keymap;
pub mod macros;
pub mod notify;
pub mod perkey;
pub mod realtime;
pub mod screen;

/// Command codes this project sends or expects (AJAZZ's WebHID driver
/// names in the comments). [`guard`] decides which go where.
pub mod cmd {
    /// GET_DEVICE_INFO: identity, firmware version, battery, limits.
    pub const DEVICE_INFO: u8 = 0x10;
    /// GET_GAME_MODE: the keyboard's system options (read only here).
    pub const OPTIONS: u8 = 0x11;
    /// GET_KEY: base-layer keymap.
    pub const KEYMAP: u8 = 0x12;
    /// GET_LED_EFFECT: the lighting effect.
    pub const EFFECT: u8 = 0x13;
    /// GET_CUSTOM_LED_DATA: the per-key colour table.
    pub const PER_KEY: u8 = 0x14;
    /// GET_MACRO: the macro table and blocks.
    pub const MACROS: u8 = 0x15;
    /// GET_FN_KEY: Fn-layer keymap.
    pub const FN_KEYMAP: u8 = 0x16;
    /// GET_DEFAULT_FN_KEY_MATRIX: the factory Fn layer.
    pub const DEFAULT_FN: u8 = 0x1C;
    /// SET_KEY.
    pub const SET_KEYMAP: u8 = 0x22;
    /// SET_LED_EFFECT.
    pub const SET_EFFECT: u8 = 0x23;
    /// SET_CUSTOM_LED_DATA.
    pub const SET_PER_KEY: u8 = 0x24;
    /// SET_MACRO.
    pub const SET_MACROS: u8 = 0x25;
    /// SET_FN_KEY.
    pub const SET_FN_KEYMAP: u8 = 0x26;
    /// GET_LED_DATA, which in fact shows a real-time frame (RAM only).
    pub const REALTIME: u8 = 0x32;
    /// GET_ALL_LIGHTS_RGB: the colours the LEDs show right now.
    pub const LIVE_COLORS: u8 = 0x33;
    /// SET_TEMPORARY_COMMAND_DATA: the clock.
    pub const CLOCK: u8 = 0x34;
    /// SET_TFT_USER_ANIMATION on FF67: one screen packet.
    pub const SCREEN: u8 = 0x50;
    /// The command byte of FF67's acknowledgement (SET_LED_USER_ANIMATION).
    /// Only ever received: the guard never lets it out.
    pub const SCREEN_ACK: u8 = 0x41;
}

/// What a value or a request violates, found before anything is sent.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CodecError {
    #[error("effect mode {0} does not exist")]
    Mode(u8),
    #[error("{field} {value} is outside {min}–{max}")]
    Range { field: &'static str, value: u32, min: u32, max: u32 },
    #[error("{0}")]
    Unsupported(&'static str),
    #[error(
        "{macros} macros with {entries} steps take {bytes} bytes; the keyboard takes {limit} \
         (and at most 99 macros)"
    )]
    TooLarge { macros: usize, entries: usize, bytes: usize, limit: usize },
    #[error("malformed {0}")]
    Malformed(&'static str),
    #[error("invalid {0}")]
    Invalid(&'static str),
}

pub(crate) fn range(field: &'static str, value: u32, min: u32, max: u32) -> Result<(), CodecError> {
    if (min..=max).contains(&value) {
        Ok(())
    } else {
        Err(CodecError::Range { field, value, min, max })
    }
}
