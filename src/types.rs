//! Common types used both by decoder and encoder
extern crate color_quant;

use std::mem;
use std::borrow::Cow;

use util;

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
            dispose: DisposalMethod::Keep,
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

impl Frame<'static> {
    
    /// Creates a frame from pixels in RGBA format.
    ///
    /// Note: This method is not optimized for speed.
    pub fn from_rgba(width: u16, height: u16, pixels: &mut [u8]) -> Frame<'static> {
        assert_eq!(width as usize * height as usize * 4, pixels.len());
        let mut frame = Frame::default();
        let mut transparent = None;
        for pix in pixels.chunks_mut(4) {
            if pix[3] != 0 {
                pix[3] = 0xFF;
            } else {
                transparent = Some([pix[0], pix[1], pix[2], pix[3]])
            }
        }
        frame.width = width;
        frame.height = height;
        let nq = color_quant::NeuQuant::new(1, 256, pixels);
        frame.buffer = Cow::Owned(pixels.chunks(4).map(|pix| nq.index_of(pix) as u8).collect());
        frame.palette = Some(nq.color_map_rgb());
        frame.transparent = if let Some(t) = transparent {
            Some(nq.index_of(&t) as u8)
        } else {
            None
        };
        frame
        
        
    }
    
    /// Creates a frame from pixels in RGB format.
    ///
    /// Note: This method is not optimized for speed.
    pub fn from_rgb(width: u16, height: u16, pixels: &[u8]) -> Frame<'static> {
        assert_eq!(width as usize * height as usize * 3, pixels.len());
        let mut vec: Vec<u8> = Vec::with_capacity(pixels.len() + width as usize * height as usize);
        for v in pixels.chunks(3) {
            vec.extend([v[0], v[1], v[2], 0xFF].iter().cloned())
        }
        Frame::from_rgba(width, height, &mut vec)
    }
}