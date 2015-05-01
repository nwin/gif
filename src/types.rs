//! Common types used both by decoder and encoder

/// Disposal methods
/// ### FIXME: NOT DOCS YET DUE TO RUST BUG
enum_from_primitive!{
#[derive(Debug, Copy, Clone)]
pub enum DisposalMethod {
    // FIXME enum_from_primitive and make this a doc-comment
    // Decoder is not required to take any action.
    Any = 0,
    // FIXME enum_from_primitive and make this a doc-comment
    // Do not dispose.
    Keep = 1,
    // FIXME enum_from_primitive and make this a doc-comment
    // Restore to background color.
    Background = 2,
    // FIXME enum_from_primitive and make this a doc-comment
    // Restore to previous.
    Previous = 3,
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

/// A frame
#[derive(Debug)]
pub struct Frame {
    pub delay: u16,
    pub dispose: DisposalMethod,
    pub transparent: Option<usize>,
    pub needs_user_input: bool,
    pub top: u16,
    pub left: u16,
    pub width: u16,
    pub height: u16,
    pub interlaced: bool,
    pub palette: Option<Vec<u8>>,
    pub buffer: Vec<u8>
}

impl Default for Frame {
    fn default() -> Frame {
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
            buffer: Vec::new()
        }
    }
}