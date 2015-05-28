//! Common types used both by decoder and encoder
use std::mem;
use std::borrow::Cow;

/// Disposal method
#[derive(Debug, Copy, Clone)]
#[repr(u8)]
pub enum DisposalMethod {
    /// StreamingDecoder is not required to take any action.
    Any = 0,
    /// Do not dispose.
    Keep = 1,
    /// Restore to background color.
    Background = 2,
    /// Restore to previous.
    Previous = 3,
}

impl DisposalMethod {
    pub fn from_u8(n: u8) -> Option<DisposalMethod> {
        if n <= 3 {
            Some(unsafe { mem::transmute(n) })
        } else {
            None
        }
    }
}

/// Known block types
enum_from_primitive!{
#[derive(Debug, Copy, Clone)]
pub enum Block {
    Image = 0x2C,
    Extension = 0x21,
    Trailer = 0x3B
}
}

/// Known GIF extensions
enum_from_primitive!{
#[derive(Debug)]
pub enum Extension {
    Text = 0x01,
    Control = 0xF9,
    Comment = 0xFE,
    Application = 0xFF
}
}

/// A GIF frame
#[derive(Debug, Clone)]
pub struct Frame<'a> {
    pub delay: u16,
    pub dispose: DisposalMethod,
    pub transparent: Option<u8>,
    pub needs_user_input: bool,
    pub top: u16,
    pub left: u16,
    pub width: u16,
    pub height: u16,
    pub interlaced: bool,
    pub palette: Option<Vec<u8>>,
    pub buffer: Cow<'a, [u8]>
}

impl<'a> Default for Frame<'a> {
    fn default() -> Frame<'a> {
        Frame {
            delay: 0,
            dispose: DisposalMethod::Any,
            transparent: None,
            needs_user_input: false,
            top: 0,
            left: 0,
            width: 0,
            height: 0,
            interlaced: false,
            palette: None,
            buffer: Cow::Borrowed(&[])
        }
    }
}