use std::num;
use std::cmp;
use std::default::Default;

use std::io;
use std::io::prelude::*;

use lzw::{LzwDecoder, LsbReader};

use traits::{HasParameters, Parameter};

/// Images get converted to RGBA
pub const N_CHANNELS: usize = 4;
/// GIF palettes are RGB
pub const PLTE_CHANNELS: usize = 3;

#[derive(Debug)]
pub enum DecodingError {
    Format(&'static str),
    Internal(&'static str),
    Io(io::Error),
}

impl From<io::Error> for DecodingError {
    fn from(err: io::Error) -> Self {
        DecodingError::Io(err)
    }
}

/// Known block types
#[derive(FromPrimitive, Debug, Copy)]
pub enum Block {
    Image = 0x2C,
    Extension = 0x21,
    Trailer = 0x3B
}

/// Known GIF extensions
#[derive(FromPrimitive, Debug)]
pub enum Extension {
    Text = 0x01,
    Control = 0xF9,
    Comment = 0xFE,
    Application = 0xFF
}

/// Output mode for the image data
#[derive(PartialEq, Debug)]
pub enum ColorOutput {
    /// The decoder expands the image data to 32bit RGBA
    TrueColor,
    /// The decoder returns the raw indexed data
    Indexed,
}

impl Parameter<Decoder> for ColorOutput {
    fn set_param(self, this: &mut Decoder) {
        this.color_output = self
    }
}

/// Disposal methods
#[derive(FromPrimitive, Debug)]
pub enum DisposalMethod {
    /// Decoder is not required to take any action.
    Any = 0,
    /// Do not dispose.
    Keep = 1,
    /// Restore to background color.
    Background = 2,
    /// Restore to previous.
    Previous = 3,
}

/// Indicated the progress of decoding. Used for block-wise reading
#[derive(Debug, PartialEq, Copy)]
pub enum Progress {
    Start,
    BlockStart,
    ExtSubBlockFinished,
    DataStart,
    DataEnd,
    Trailer
}

/// Internal state of the GIF decoder
#[derive(Debug)]
enum State {
    Magic(usize, [u8; 6]),
    U16Byte1(U16Value, u8),
    U16(U16Value),
    Byte(ByteValue),
    GlobalPalette(usize),
    BlockStart(Option<Block>),
    AwaitBlockEnd,
    BlockEnd(u8),
    ExtensionBlock(u8),
    SkipBlock(usize),
    LocalPalette(usize),
    LzwInit(u8),
    DecodeSubBlock(Box<LzwDecoder<LsbReader>>, usize),
    Trailer
}
use self::State::*;

/// U16 values that may occur in a GIF image
#[derive(Debug)]
enum U16Value {
    /// Logical screen descriptor width
    ScreenWidth,
    /// Logical screen descriptor height
    ScreenHeight,
    /// Delay time
    Delay,
    /// Left frame offset
    ImageLeft,
    /// Top frame offset
    ImageTop,
    /// Frame width
    ImageWidth,
    /// Frame height
    ImageHeight,
}

/// Single byte screen descriptor values
#[derive(Debug)]
enum ByteValue {
    GlobalFlags,
    Background { table_size: usize },
    AspectRatio { table_size: usize },
    ControlFlags,
    ImageFlags,
    TransparentIdx,
    CodeSize,
}

/// A frame
#[derive(Debug)]
pub struct Frame {
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

/// GIF decoder which supports streaming
#[derive(Debug)]
pub struct Decoder {
    state: Option<State>,
    progress: Progress,
    color_output: ColorOutput,
    version: &'static str,
    width: u16,
    height: u16,
    global_color_table: Vec<u8>,
    background_color: [u8; 4],
    /// ext buffer
    ext: (u8, Vec<u8>),
    /// Frame data
    current: Option<usize>,
    frames: Vec<Frame>
}

impl HasParameters for Decoder {}

impl Decoder {
    pub fn new() -> Decoder {
        Decoder {
            state: Some(Magic(0, [0; 6])),
            progress: Progress::Start,
            color_output: ColorOutput::Indexed,
            version: "",
            width: 0,
            height: 0,
            global_color_table: Vec::new(),
            background_color: [0, 0, 0, 0xFF],
            ext: (0, Vec::with_capacity(256)), // 0xFF + 1 byte length
            current: None,
            frames: Vec::new()
        }
    }
    
    pub fn update(&mut self, buf: &[u8]) -> Result<usize, DecodingError> {
        self.update_until(buf, Progress::Trailer)
    }
    
    pub fn update_until(&mut self, mut buf: &[u8], stop_at: Progress)
    -> Result<usize, DecodingError> {
        let len = buf.len();
        while buf.len() > 0 && self.state.is_some() {
            if self.progress == stop_at {
                return Ok(len-buf.len())
            }
            match self.next_state(buf) {
                Ok(bytes) => {
                    buf = &buf[bytes..]
                }
                Err(err) => return Err(err)
            }
        }
        Ok(len-buf.len())
    }
    
    pub fn progress(&self) -> Progress {
        self.progress
    }
    
    pub fn width(&self) -> u16 {
        self.width
    }
    
    pub fn height(&self) -> u16 {
        self.height
    }

    /// The global color palette
    pub fn global_palette(&self) -> &[u8] {
        &*self.global_color_table
    }

    /// Index of the background color in the global palette
    pub fn bg_color(&self) -> u16 {
        self.global_color_table.chunks(PLTE_CHANNELS).position(
            |v| v == &self.background_color[..3] 
        ).unwrap_or(0) as u16
    }
    
    pub fn frames(&self) -> &[Frame] {
        &*self.frames
    }

    pub fn last_ext(&self) -> (u8, &[u8]) {
        (self.ext.0, &*self.ext.1)
    }
    
    /// Returns the current block if the decoder is at the start
    // a block
    pub fn _current_block(&self) -> Option<Block> {
        match self.state {
            Some(BlockStart(block)) => block,
            _ => None
        }

    }

    fn next_state(&mut self, buf: &[u8]) -> Result<usize, DecodingError> {
        macro_rules! goto (
            ($n:expr, $state:expr) => ({
                self.state = Some($state); 
                Ok($n)
            });
            ($state:expr) => ({
                self.state = Some($state); 
                Ok(1)
            })
        );
        
        let b = buf[0];
        
        // Driver should ensure that state is never None
        let state = self.state.take().unwrap();
        //println!("{:?}", state);
        
        match state {
            Magic(i, mut version) => if i < 6 {
                version[i] = b;
                goto!(Magic(i+1, version))
            } else if &version[..3] == b"GIF" {
                self.version = match &version[3..] {
                    b"87a" => "87a",
                    b"89a" => "89a",
                    _ => return Err(DecodingError::Format("unsupported GIF version"))
                };
                goto!(U16Byte1(U16Value::ScreenWidth, b))
            } else {
                Err(DecodingError::Format("malformed GIF header"))
            },
            U16(next) => goto!(U16Byte1(next, b)),
            U16Byte1(next, value) => {
                use self::U16Value::*;
                let value = ((b as u16) << 8) | value as u16;
                match (next, value) {
                    (ScreenWidth, width) => {
                        self.width = width;
                        goto!(U16(U16Value::ScreenHeight))
                    },
                    (ScreenHeight, height) => {
                        self.height = height;
                        goto!(Byte(ByteValue::GlobalFlags))
                    },
                    (Delay, delay) => {
                        self.ext.1.push(value as u8);
                        self.ext.1.push(b);
                        self.current_frame().delay = delay;
                        goto!(Byte(ByteValue::TransparentIdx))
                    },
                    (ImageLeft, left) => {
                        self.current_frame().left = left;
                        goto!(U16(U16Value::ImageTop))
                    },
                    (ImageTop, top) => {
                        self.current_frame().top = top;
                        goto!(U16(U16Value::ImageWidth))
                    },
                    (ImageWidth, width) => {
                        self.current_frame().width = width;
                        goto!(U16(U16Value::ImageHeight))
                    },
                    (ImageHeight, height) => {
                        self.current_frame().height = height;
                        goto!(Byte(ByteValue::ImageFlags))
                    }
                }
            }
            Byte(value) => {
                use self::ByteValue::*;
                match value {
                    GlobalFlags => {
                        let global_table = b & 0x80 != 0;
                        let entries = if global_table {
                            let entries = PLTE_CHANNELS*(1 << ((b & 0b111) + 1) as usize);
                            self.global_color_table.reserve_exact(entries);
                            entries
                        } else {
                            0usize
                        };
                        goto!(Byte(Background { table_size: entries }))
                    },
                    Background { table_size } => {
                        self.background_color[0] = b;
                        goto!(Byte(AspectRatio { table_size: table_size }))
                    },
                    AspectRatio { table_size } => {
                        goto!(GlobalPalette(table_size))
                    },
                    ControlFlags => {
                        self.ext.1.push(b);
                        let control_flags = b;
                        if control_flags & 1 != 0 {
                            // Set to Some(...), gets overwritten later
                            self.current_frame().transparent = Some(0)
                        }
                        self.current_frame().needs_user_input =
                            control_flags & 0b10 != 0;
                        self.current_frame().dispose = match num::FromPrimitive::from_u8(
                            (control_flags & 0b11100) >> 2
                        ) {
                            Some(method) => method,
                            None => return Err(DecodingError::Format(
                                "unknown disposal method"
                            ))
                        };
                        goto!(U16(U16Value::Delay))
                    }
                    TransparentIdx => {
                        self.ext.1.push(b);
                        if let Some(ref mut idx) = self.current_frame().transparent {
                             *idx = b
                        }
                        self.progress == Progress::ExtSubBlockFinished;
                        goto!(AwaitBlockEnd)
                    }
                    ImageFlags => {
                        let local_table = (b & 0b1000_0000) != 0;
                        let interlaced   = (b & 0b0100_0000) != 0;
                        let table_size  =  b & 0b0000_0111;
                        
                        self.current_frame().interlaced = interlaced;
                        if local_table {
                            let entries = PLTE_CHANNELS * (1 << (table_size + 1));
                            
                            self.current_frame().palette =
                                Some(Vec::with_capacity(entries));
                            goto!(LocalPalette(entries))
                        } else {
                            goto!(Byte(CodeSize))
                        }
                    },
                    CodeSize => goto!(LzwInit(b))
                }
            }
            GlobalPalette(left) => {
                let n = cmp::min(left, buf.len());
                if left > 0 {
                    self.global_color_table.push_all(&buf[..n]);
                    goto!(n, GlobalPalette(left - n))
                } else {
                    let idx = self.background_color[0];
                    match self.global_color_table.chunks(PLTE_CHANNELS).nth(idx as usize) {
                        Some(chunk) => for i in 0..PLTE_CHANNELS {
                            self.background_color[i] = chunk[i]
                        },
                        None => self.background_color[0] = 0
                    }
                    self.progress = Progress::BlockStart;
                    goto!(BlockStart(num::FromPrimitive::from_u8(b)))
                }
            }
            BlockStart(type_) => {
                use self::Block::*;
                match type_ {
                    Some(Image) => {
                        self.add_frame();
                        goto!(U16Byte1(U16Value::ImageLeft, b))
                    }
                    Some(Extension) => goto!(ExtensionBlock(b)),
                    Some(Trailer) => goto!(0, State::Trailer),
                    None => {
                        return Err(DecodingError::Format(
                        "unknown block type encountered"
                    ))}
                }
            }
            AwaitBlockEnd => goto!(BlockEnd(b)),
            BlockEnd(terminator) => {
                if terminator == 0 {
                    if b == Block::Trailer as u8 {
                        goto!(0, Trailer)
                    } else {
                        self.progress = Progress::BlockStart;
                        goto!(BlockStart(num::FromPrimitive::from_u8(b)))
                    }
                } else {
                    return Err(DecodingError::Format(
                        "expected block terminator not found"
                    ))
                }
            }
            ExtensionBlock(type_) => {
                use self::Extension::*;
                self.ext.0 = type_;
                self.ext.1.clear();
                self.ext.1.push(b);
                if let Some(ext) = num::FromPrimitive::from_u8(type_) {
                    match ext {
                        Control => {
                            goto!(try!(self.read_control_extension(b)))
                        }
                        Text | Comment | Application => {
                            goto!(SkipBlock(b as usize))
                        }
                    }
                } else {
                    return Err(DecodingError::Format(
                        "unknown extention block encountered"
                    ))
                }
            }
            SkipBlock(left) => {
                let n = cmp::min(left, buf.len());
                if left > 0 {
                    self.ext.1.push(b);
                    goto!(n, SkipBlock(left - n))
                } else {
                    if b == 0 {
                        self.progress == Progress::ExtSubBlockFinished;
                        goto!(BlockEnd(b))
                    } else {
                        self.progress == Progress::ExtSubBlockFinished;
                        goto!(SkipBlock(b as usize))
                    }
                    
                }
            }
            LocalPalette(left) => {
                let n = cmp::min(left, buf.len());
                if left > 0 {
                    
                    self.current_frame().palette
                        .as_mut().unwrap().push_all(&buf[..n]);
                    goto!(n, LocalPalette(left - n))
                } else {
                    goto!(LzwInit(b))
                }
            }
            LzwInit(code_size) => {
                self.progress = Progress::DataStart;
                goto!(DecodeSubBlock(
                    box LzwDecoder::new(LsbReader::new(), code_size),
                    b as usize
                ))
            }
            DecodeSubBlock(mut decoder, left) => {;
                let n = cmp::min(left, buf.len());
                if left > 0 {
                    let mut buf = &buf[..n];
                    while buf.len() > 0 {
                        let (consumed, bytes) = try!(decoder.decode_bytes(buf));
                        
                        self.current_frame().buffer.push_all(bytes);
                        buf = &buf[consumed..];
                    }
                    goto!(n, DecodeSubBlock(decoder, left - n))
                } else if b != 0 { // decode next sub-block
                    goto!(DecodeSubBlock(decoder, b as usize))
                } else { // end of image data reached
                    if self.color_output == ColorOutput::TrueColor {
                        self.expand_palette();
                    }
                    self.current = None;
                    self.progress = Progress::DataEnd;
                    goto!(BlockEnd(b))
                }
            }
            Trailer => {
                self.state = None;
                self.progress = Progress::Trailer;
                Ok(1)
                //panic!("EOF {:?}", self)
            }
        }
    }
    
    #[inline]
    fn read_control_extension(&mut self, b: u8) -> Result<State, DecodingError> {
        self.add_frame();
        self.ext.1.push(b);
        if b != 4 {
            return Err(DecodingError::Format(
                "control extension has wrong length"
            ))
        }
        Ok(Byte(ByteValue::ControlFlags))
    }
    
    fn add_frame(&mut self) {
        if self.current.is_none() {
            self.current = Some(self.frames.len());
            self.frames.push(Default::default());
            let required_bytes = self.width as usize * self.height as usize;
            self.current_frame().buffer.reserve(required_bytes);
        }
    }
    
    #[inline(always)]
    pub fn current_frame(&mut self) -> &mut Frame {
        let c = self.current.unwrap_or(0);
        &mut self.frames[c]
    }
    
    pub fn expand_palette(&mut self) {
        {
            let required_bytes = N_CHANNELS * self.width as usize * self.height as usize;
            let frame = &mut self.current_frame();
            let capacity = frame.buffer.capacity();
            let new_size = cmp::max(capacity, required_bytes);
            frame.buffer.reserve(new_size - capacity);
            for i in 0..(new_size - capacity) {
                frame.buffer.push(0)
            }
            // unsafe { frame.buffer.set_len(required_bytes) }
        }
        let c = self.current.unwrap_or(0);
        let frame = &mut self.frames[c];
        expand_palette(
            &mut *frame.buffer,
            match frame.palette {
                Some(ref table) => &*table,
                None => &*self.global_color_table,
            },
            None
        );
    }
}

/// Naive version, should be optimized for speed
fn expand_palette(buf: &mut [u8], palette: &[u8], transparent: Option<u8>) {
    //use std::iter::RandomAccessIterator;
    for i in (0..buf.len()/N_CHANNELS).rev() {
        let plte_idx = buf[i] as usize;
        //if let Some(colors) = palette.chunks(PLTE_CHANNELS).nth(plte_idx) { // slow...
        //if let Some(colors) = palette.chunks(PLTE_CHANNELS).idx(plte_idx) { // faster...
        let plte_offset = PLTE_CHANNELS*plte_idx;
        if palette.len() >= plte_offset + PLTE_CHANNELS {
            let colors = &palette[plte_offset..plte_offset + PLTE_CHANNELS];
            let idx = i * N_CHANNELS;
            for j in 0..N_CHANNELS-1 {
                buf[idx+j] = colors[j];
            }
            buf[idx+N_CHANNELS-1] = if let Some(transparent) = transparent {
                if plte_idx == transparent as usize {
                    0x00
                } else {
                    0xFF
                }
            } else {
                0xFF
            }
        }
    }
}

#[cfg(test)]
mod test {
    extern crate test;

    use std::fs::File;
    use std::io::prelude::*;
    
    use super::Decoder;
    
    #[bench]
    fn bench_tiny(b: &mut test::Bencher) {
        let mut data = Vec::new();
        File::open("tests/samples/sample_1.gif").unwrap().read_to_end(&mut data).unwrap();
        b.iter(|| {
            test::black_box(Decoder::new().update(&*data).ok().unwrap())
        });
        let mut decoder = Decoder::new();
        decoder.update(&*data).ok().unwrap();
        b.bytes = decoder.frames[0].buffer.len() as u64
    }
    
    #[bench]
    fn bench_big(b: &mut test::Bencher) {
        let mut data = Vec::new();
        File::open("tests/samples/moon_impact.gif").unwrap().read_to_end(&mut data).unwrap();
        b.iter(|| {
            test::black_box(Decoder::new().update(&*data).ok().unwrap())
        });
        let mut decoder = Decoder::new();
        decoder.update(&*data).ok().unwrap();
        b.bytes = (decoder.frames.len() * decoder.frames[0].buffer.len()) as u64
    }
    
    #[test]
    fn test_simple() {
        let mut data = Vec::new();
        File::open("tests/samples/sample_1.gif").unwrap().read_to_end(&mut data).unwrap();
        let mut decoder = Decoder::new();
        decoder.update(&*data).unwrap();
    }
}