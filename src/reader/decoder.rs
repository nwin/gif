use std::cmp;
use std::mem;
use std::default::Default;
use std::rc::Rc;

use std::io;
use std::io::prelude::*;

use num;
use lzw;

use traits::{HasParameters, Parameter};
use types::{Frame, Block};
use types::{DisposalMethod};

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

/// Configures how extensions should be handled
#[derive(PartialEq, Debug)]
pub enum Extensions {
    /// Saves all extention data
    Save,
    /// Skips the data of unknown extensions
    /// and extracts the data from known ones
    Skip
}

impl Parameter<StreamingDecoder> for Extensions {
    fn set_param(self, this: &mut StreamingDecoder) {
        this.skip_extensions = match self {
            Extensions::Skip => true,
            Extensions::Save => false,

        }
    }
}

/// Indicates whether a certain object has been decoded
pub enum Decoded<'a> {
    Nothing,
    Trailer,
    BlockStart(Block),
    SubBlockFinished(u8, &'a [u8]),
    BlockFinished(u8, &'a [u8]),
    GlobalPalette(Rc<Vec<u8>>),
    Frame(&'a Frame<'static>),
    Data(&'a [u8]),
    DataEnd,

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
    BlockEnd(u8),
    ExtensionBlock(u8),
    SkipBlock(usize),
    LocalPalette(usize),
    LzwInit(u8),
    DecodeSubBlock(usize),
    FrameDecoded,
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

/// GIF decoder which supports streaming
#[derive(Debug)]
pub struct StreamingDecoder {
    state: Option<State>,
    lzw_reader: Option<lzw::Decoder<lzw::LsbReader>>,
    skip_extensions: bool,
    version: &'static str,
    width: u16,
    height: u16,
    global_color_table: Rc<Vec<u8>>,
    background_color: [u8; 4],
    /// ext buffer
    ext: (u8, Vec<u8>, bool),
    /// Frame data
    current: Option<Frame<'static>>,
}

impl HasParameters for StreamingDecoder {}

impl StreamingDecoder {
    pub fn new() -> StreamingDecoder {
        StreamingDecoder {
            state: Some(Magic(0, [0; 6])),
            lzw_reader: None,
            skip_extensions: true,
            version: "",
            width: 0,
            height: 0,
            global_color_table: Rc::new(Vec::new()),
            background_color: [0, 0, 0, 0xFF],
            ext: (0, Vec::with_capacity(256), true), // 0xFF + 1 byte length
            current: None
        }
    }
    
    pub fn update<'a>(&'a mut self, mut buf: &[u8])
    -> Result<(usize, Decoded<'a>), DecodingError> {
        // NOTE: Do not change the function signature without double-checking the
        //       unsafe block!
        let len = buf.len();
        while buf.len() > 0 && self.state.is_some() {
            match self.next_state(buf) {
                Ok((bytes, Decoded::Nothing)) => {
                    buf = &buf[bytes..]
                }
                Ok((bytes, Decoded::Trailer)) => {
                    buf = &buf[bytes..];
                    break
                }
                Ok((bytes, result)) => {
                    buf = &buf[bytes..];
                    return Ok(
                        (len-buf.len(), 
                        // This transmute just casts the lifetime away. Since Rust only 
                        // has SESE regions, this early return cannot be worked out and
                        // such that the borrow region of self includes the whole block.
                        // The explixit lifetimes in the function signature ensure that
                        // this is safe.
                        // ### NOTE
                        // To check that everything is sound, return the result without
                        // the match (e.g. `return Ok(try!(self.next_state(buf)))`). If
                        // it compiles the returned lifetime is correct.
                        unsafe { 
                            mem::transmute::<Decoded, Decoded>(result)
                        }
                    ))
                }
                Err(err) => return Err(err)
            }
        }
        Ok((len-buf.len(), Decoded::Nothing))
        
    }
    
    /// Index of the background color in the global palette
    pub fn bg_color(&self) -> usize {
        self.global_color_table.chunks(PLTE_CHANNELS).position(
            |v| v == &self.background_color[..3] 
        ).unwrap_or(0) as usize
    }
    
    pub fn last_ext(&self) -> (u8, &[u8], bool) {
        (self.ext.0, &*self.ext.1, self.ext.2)
    }
    
    #[inline(always)]
    pub fn current_frame_mut<'a>(&'a mut self) -> &'a mut Frame<'static> {
        self.current.as_mut().unwrap()
    }
    
    #[inline(always)]
    pub fn current_frame<'a>(&'a self) -> &'a Frame<'static> {
        self.current.as_ref().unwrap()
    }

    /// Width of the image
    pub fn width(&self) -> u16 {
        self.width
    }

    /// Height of the image
    pub fn height(&self) -> u16 {
        self.height
    }

    fn next_state<'a>(&'a mut self, buf: &[u8]) -> Result<(usize, Decoded<'a>), DecodingError> {
        macro_rules! goto (
            ($n:expr, $state:expr) => ({
                self.state = Some($state); 
                Ok(($n, Decoded::Nothing))
            });
            ($state:expr) => ({
                self.state = Some($state); 
                Ok((1, Decoded::Nothing))
            });
            ($n:expr, $state:expr, emit $res:expr) => ({
                self.state = Some($state); 
                Ok(($n, $res))
            });
            ($state:expr, emit $res:expr) => ({
                self.state = Some($state); 
                Ok((1, $res))
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
                        self.current_frame_mut().delay = delay;
                        goto!(Byte(ByteValue::TransparentIdx))
                    },
                    (ImageLeft, left) => {
                        self.current_frame_mut().left = left;
                        goto!(U16(U16Value::ImageTop))
                    },
                    (ImageTop, top) => {
                        self.current_frame_mut().top = top;
                        goto!(U16(U16Value::ImageWidth))
                    },
                    (ImageWidth, width) => {
                        self.current_frame_mut().width = width;
                        goto!(U16(U16Value::ImageHeight))
                    },
                    (ImageHeight, height) => {
                        self.current_frame_mut().height = height;
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
                            self.global_color_table.make_unique().reserve_exact(entries);
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
                            self.current_frame_mut().transparent = Some(0)
                        }
                        self.current_frame_mut().needs_user_input =
                            control_flags & 0b10 != 0;
                        self.current_frame_mut().dispose = match DisposalMethod::from_u8(
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
                        if let Some(ref mut idx) = self.current_frame_mut().transparent {
                             *idx = b
                        }
                        goto!(SkipBlock(0))
                        //goto!(AwaitBlockEnd)
                    }
                    ImageFlags => {
                        let local_table = (b & 0b1000_0000) != 0;
                        let interlaced   = (b & 0b0100_0000) != 0;
                        let table_size  =  b & 0b0000_0111;
                        
                        self.current_frame_mut().interlaced = interlaced;
                        if local_table {
                            let entries = PLTE_CHANNELS * (1 << (table_size + 1));
                            
                            self.current_frame_mut().palette =
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
                    self.global_color_table.make_unique().push_all(&buf[..n]);
                    goto!(n, GlobalPalette(left - n))
                } else {
                    let idx = self.background_color[0];
                    match self.global_color_table.chunks(PLTE_CHANNELS).nth(idx as usize) {
                        Some(chunk) => for i in 0..PLTE_CHANNELS {
                            self.background_color[i] = chunk[i]
                        },
                        None => self.background_color[0] = 0
                    }
                    goto!(BlockStart(num::FromPrimitive::from_u8(b)), emit Decoded::GlobalPalette(
                        self.global_color_table.clone()
                    ))
                }
            }
            BlockStart(type_) => {
                use types::Block::*;
                match type_ {
                    Some(Image) => {
                        self.add_frame();
                        goto!(U16Byte1(U16Value::ImageLeft, b), emit Decoded::BlockStart(Image))
                    }
                    Some(Extension) => goto!(ExtensionBlock(b), emit Decoded::BlockStart(Extension)),
                    Some(Trailer) => goto!(0, State::Trailer, emit Decoded::BlockStart(Trailer)),
                    None => {
                        return Err(DecodingError::Format(
                        "unknown block type encountered"
                    ))}
                }
            }
            BlockEnd(terminator) => {
                if terminator == 0 {
                    if b == Block::Trailer as u8 {
                        goto!(0, Trailer)
                    } else {
                        goto!(BlockStart(num::FromPrimitive::from_u8(b)))
                    }
                } else {
                    return Err(DecodingError::Format(
                        "expected block terminator not found"
                    ))
                }
            }
            ExtensionBlock(type_) => {
                use types::Extension::*;
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
                        self.ext.2 = true;
                        goto!(BlockEnd(b), emit Decoded::BlockFinished(self.ext.0, &self.ext.1))
                    } else {
                        self.ext.2 = false;
                        goto!(SkipBlock(b as usize), emit Decoded::SubBlockFinished(self.ext.0,&self.ext.1))
                    }
                    
                }
            }
            LocalPalette(left) => {
                let n = cmp::min(left, buf.len());
                if left > 0 {
                    
                    self.current_frame_mut().palette
                        .as_mut().unwrap().push_all(&buf[..n]);
                    goto!(n, LocalPalette(left - n))
                } else {
                    goto!(LzwInit(b))
                }
            }
            LzwInit(code_size) => {
                self.lzw_reader = Some(lzw::Decoder::new(lzw::LsbReader::new(), code_size));
                goto!(DecodeSubBlock(b as usize), emit Decoded::Frame(self.current_frame_mut()))
            }
            DecodeSubBlock(left) => {
                if left > 0 {
                    let n = cmp::min(left, buf.len());
                    let decoder = self.lzw_reader.as_mut().unwrap();
                    let (consumed, bytes) = try!(decoder.decode_bytes(&buf[..n]));
                    goto!(consumed, DecodeSubBlock(left - consumed), emit Decoded::Data(bytes))
                }  else if b != 0 { // decode next sub-block
                    goto!(DecodeSubBlock(b as usize))
                } else {
                    // end of image data reached
                    self.current = None;
                    goto!(0, FrameDecoded, emit Decoded::DataEnd)
                }
            }
            FrameDecoded => {
                goto!(BlockEnd(b))
            }
            Trailer => {
                self.state = None;
                Ok((1, Decoded::Trailer))
                //panic!("EOF {:?}", self)
            }
        }
    }
    
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
            self.current = Some(Frame::default())
        }
    }
}