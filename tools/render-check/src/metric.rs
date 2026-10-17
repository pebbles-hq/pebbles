//! Image type and diff metric (§6.10).

/// Tightly packed RGBA8 (`width * height * 4` bytes).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rgba8 {
    pub width: u32,
    pub height: u32,
    pub data: Vec<u8>,
}
