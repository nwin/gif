use std::borrow::Cow;
use std::io;
use std::cmp;
use std::mem;
use std::rc::Rc;
use std::io::prelude::*;

use traits::{HasParameters, Parameter};
use types::Frame;
use util;

mod decoder;
pub use self::decoder::{
    StreamingDecoder, Decoded, DecodingError, Extensions
};


const N_CHANNELS: usize = 4;

impl<T, R> Parameter<Decoder<R>> for T
where T: Parameter<StreamingDecoder>, R: Read {
    fn set_param(self, this: &mut Decoder<R>) {
        this.decoder.set(self);
    }

}

/// Output mode for the image data
/// ### FIXME: NOT DOCS YET DUE TO RUST BUG
enum_from_primitive!{
#[derive(PartialEq, Debug)]
pub enum ColorOutput {
    // FIXME enum_from_primitive and make this a doc-comment
    // The decoder expands the image data to 32bit RGBA
    TrueColor,
    // FIXME enum_from_primitive and make this a doc-comment
    // The decoder returns the raw indexed data*/
    Indexed,
}
}

impl<R: Read> Parameter<Decoder<R>> for ColorOutput {
    fn set_param(self, this: &mut Decoder<R>) {
        this.color_output = self
    }
}

impl<R: Read> HasParameters for Decoder<R> {}

/// GIF decoder
pub struct Decoder<R: Read> {
    r: R,
    decoder: StreamingDecoder,
    color_output: ColorOutput,
}

impl<R: Read> Decoder<R> {
    pub fn new(r: R) -> Decoder<R> {
        Decoder {
            r: r,
            decoder: StreamingDecoder::new(),
            color_output: ColorOutput::Indexed
        }
    }
    
    pub fn read_info(self) -> Result<Reader<R>, DecodingError> {
        Reader::new(self.r, self.decoder, self.color_output).init()
    }
}

struct ReadDecoder<R: Read> {
    reader: io::BufReader<R>,
    decoder: StreamingDecoder,
}

impl<R: Read> ReadDecoder<R> {
    fn decode_next(&mut self) -> Result<Option<Decoded>, DecodingError> {
        loop {
            let (consumed, result) = {
                let buf = try!(self.reader.fill_buf());
                if buf.len() == 0 {
                    return Err(DecodingError::Format(
                        "unexpected EOF"
                    ))
                }
                try!(self.decoder.update(buf))
            };
            self.reader.consume(consumed);
            match result {
                Decoded::Nothing => continue,
                Decoded::Trailer => return Ok(None),
                result => return Ok(unsafe{
                    // FIXME: #6393
                    Some(mem::transmute::<Decoded, Decoded>(result))
                }),
            }
        }
    }
}

pub struct Reader<R: Read> {
    decoder: ReadDecoder<R>,
    color_output: ColorOutput,
    global_palette: Option<Rc<Vec<u8>>>,
    current_frame: Frame<'static>,
    buffer: Vec<u8>,
    // Offset in current frame
    offset: usize

}

impl<R> Reader<R> where R: Read {
    fn new(reader: R, decoder: StreamingDecoder, color_output: ColorOutput) -> Reader<R> {
        Reader {
            decoder: ReadDecoder {
                reader: io::BufReader::new(reader),
                decoder: decoder
            },
            global_palette: None,
            buffer: Vec::with_capacity(32),
            color_output: color_output,
            current_frame: Frame::default(),
            offset: 0
        }
    }
    
    fn init(mut self) -> Result<Self, DecodingError> {
        match try!(self.next_frame()) {
            Some(_) => (),
            None => return Err(DecodingError::Format(
                "File does not contain any image data"
            ))
            
        }
        Ok(self)
    }
    
    /// Returns the next frame
    fn next_frame(&mut self) -> Result<Option<&Frame<'static>>, DecodingError> {
        loop {
            match try!(self.decoder.decode_next()) {
                Some(Decoded::Frame(frame)) => {
                    self.current_frame = frame.clone();
                    if frame.palette.is_none() && self.global_palette.is_none() {
                        return Err(DecodingError::Format(
                            "Image does not contain any color table."
                        ))
                    }
                    break  
                },
                Some(Decoded::GlobalPalette(palette)) => {
                    self.global_palette = Some(palette)
                },
                Some(_) => (),
                None => return Ok(None)
                
            }
        }
        Ok(Some(&self.current_frame))
    }
    
    /// Reads the next frame
    pub fn read_next_frame(&mut self) -> Result<Option<&Frame<'static>>, DecodingError> {
        let mut buf = vec![0; self.buffer_size()];
        for line in buf.chunks_mut(self.line_length()) {
            if !try!(self.next_line(line)) {
                return Err(DecodingError::Format(
                    "Image truncated"
                ))
            }
        }
        self.current_frame.buffer = Cow::Owned(buf);
        Ok(Some(&self.current_frame))
    }
    
    /// Fills the buffer with the data of the next line
    ///
    /// The buffer has to be as long as `Self::line_length`
    pub fn next_line(&mut self, mut buf: &mut [u8]) -> Result<bool, DecodingError> {
        use self::ColorOutput::*;
        const PLTE_CHANNELS: usize = 3;
        macro_rules! handle_data(
            ($data:expr) => {
                match self.color_output {
                    TrueColor => {
                        let transparent = self.current_frame.transparent;
                        println!("{:?}", transparent);
                        let palette: &[u8] = match self.current_frame.palette {
                            Some(ref table) => &*table,
                            None => &*self.global_palette.as_ref().unwrap(),
                        };
                        let len = cmp::min(buf.len()/N_CHANNELS, $data.len());
                        for (rgba, &idx) in buf[..len*N_CHANNELS].chunks_mut(N_CHANNELS).zip($data.iter()) {
                            let plte_offset = PLTE_CHANNELS * idx as usize;
                            if palette.len() >= plte_offset + PLTE_CHANNELS {
                                let colors = &palette[plte_offset..];
                                rgba[0] = colors[0];
                                rgba[1] = colors[1];
                                rgba[2] = colors[2];
                                rgba[3] = if let Some(t) = transparent {
                                    if t == idx { 0x00 } else { 0xFF }
                                } else {
                                    0xFF
                                }
                            }
                        }
                        (len, N_CHANNELS)
                    },
                    Indexed => {
                        let len = cmp::min(buf.len(), $data.len());
                        util::copy_memory(&$data[..len], &mut buf[..len]);
                        (len, 1)
                    }
                }
            }
        );
        let buf_len = self.buffer.len();
        if buf_len > 0 {
            let (len, channels) = handle_data!(&self.buffer);
            self.buffer.truncate(buf_len-len);
            let buf_ = buf; buf = &mut buf_[len*channels..];
            if buf.len() == 0 {
                return Ok(true)
            }
        }
        loop {
            match try!(self.decoder.decode_next()) {
                Some(Decoded::Data(data)) => {
                    let (len, channels) = handle_data!(data);
                    let buf_ = buf; buf = &mut buf_[len*channels..]; // shorten buf
                    if buf.len() > 0 {
                        continue
                    } else if len < data.len() {
                        self.buffer.extend(data[len..].iter().map(|&v| v));
                    }
                    return Ok(true)
                },
                Some(_) => return Ok(false), // make sure that no important result is missed
                None => return Ok(false)
                
            }
        }
    }
    
    /// Output buffer size
    pub fn buffer_size(&self) -> usize {
        self.line_length() * self.current_frame.height as usize
    }
    
    /// Line length of the current frame
    pub fn line_length(&self) -> usize {
        use self::ColorOutput::*;
        match self.color_output {
            TrueColor => self.current_frame.width as usize * N_CHANNELS,
            Indexed => self.current_frame.width as usize
        }
    }
    
    /// The global color palette
    pub fn palette(&self) -> &[u8] {
        match self.current_frame.palette {
            Some(ref table) => &*table,
            None => &*self.global_palette.as_ref().unwrap(),
        }
    }


    /// Width of the image
    pub fn width(&self) -> u16 {
        unimplemented!()
    }

    /// Height of the image
    pub fn height(&self) -> u16 {
        unimplemented!()
    }

    /// Index of the background color in the global palette
    pub fn bg_color(&self) -> usize {
        unimplemented!();
    }
}

#[cfg(test)]
mod test {
    extern crate test;

    use std::fs::File;
    use std::io::prelude::*;

    use traits::HasParameters;
    use super::{Decoder, ColorOutput};
    
    
    #[bench]
    fn bench_tiny(b: &mut test::Bencher) {
        let mut data = Vec::new();
        File::open("tests/samples/sample_1.gif").unwrap().read_to_end(&mut data).unwrap();
        b.iter(|| {
            let mut decoder = Decoder::new(&*data).read_info().unwrap();
            let frame = decoder.read_next_frame().unwrap().unwrap();
            test::black_box(frame);
        });
        let mut decoder = Decoder::new(&*data).read_info().unwrap();
        b.bytes = decoder.read_next_frame().unwrap().unwrap().buffer.len() as u64
    }
    
    #[bench]
    fn bench_big(b: &mut test::Bencher) {
        let mut data = Vec::new();
        File::open("tests/samples/sample_big.gif").unwrap().read_to_end(&mut data).unwrap();
        b.iter(|| {
            let mut decoder = Decoder::new(&*data).read_info().unwrap();
            let frame = decoder.read_next_frame().unwrap().unwrap();
            test::black_box(frame);
        });
        let mut decoder = Decoder::new(&*data).read_info().unwrap();
        b.bytes = decoder.read_next_frame().unwrap().unwrap().buffer.len() as u64
    }
    
    #[test]
    fn test_simple_expanded() {
        let mut decoder = Decoder::new(File::open("tests/samples/sample_1.gif").unwrap());
        decoder.set(ColorOutput::TrueColor);
        let mut decoder = decoder.read_info().unwrap();
        let frame = decoder.read_next_frame().unwrap().unwrap();
        assert_eq!((&frame.buffer as &[u8]).iter().map(|&v| v as u32).sum::<u32>(), 59160)
    }
    
    #[test]
    fn test_simple_indexed() {
        let mut decoder = Decoder::new(File::open("tests/samples/sample_1.gif").unwrap()).read_info().unwrap();
        let frame = decoder.read_next_frame().unwrap().unwrap();
        assert_eq!(&*frame.buffer, &[
            1, 1, 1, 1, 1, 2, 2, 2, 2, 2,
            1, 1, 1, 1, 1, 2, 2, 2, 2, 2,
            1, 1, 1, 1, 1, 2, 2, 2, 2, 2,
            1, 1, 1, 0, 0, 0, 0, 2, 2, 2,
            1, 1, 1, 0, 0, 0, 0, 2, 2, 2,
            2, 2, 2, 0, 0, 0, 0, 1, 1, 1,
            2, 2, 2, 0, 0, 0, 0, 1, 1, 1,
            2, 2, 2, 2, 2, 1, 1, 1, 1, 1,
            2, 2, 2, 2, 2, 1, 1, 1, 1, 1,
            2, 2, 2, 2, 2, 1, 1, 1, 1, 1
        ][..])
    }
}


#[cfg(feature = "c_api")]
mod c_interface {
    use std::io::prelude::*;
    use std::ptr;
    use num;

    use libc::c_int;
    
    use types::Block;

    use c_api::{self, GifWord};
    use c_api_utils::{CInterface, copy_colormap, copy_data, saved_images_new};

    use super::decoder::{Progress, DecodingError};

    use super::{Reader};

    impl<R> Reader<R> where R: Read + 'static {   
        pub fn into_c_interface(self) -> Box<CInterface> {
            box self
        }
    }

    impl<R: Read> CInterface for Reader<R> {
        fn read_screen_desc(&mut self, this: &mut c_api::GifFileType) -> Result<(), DecodingError> {
            if self.decoder.progress() == Progress::Start {
                try!(self.read_until(Progress::BlockStart));
                this.SWidth = self.width() as GifWord;
                this.SHeight = self.height() as GifWord;
                this.SColorResolution = 255;//self.global_palette().len() as GifWord;
                this.SBackGroundColor = self.bg_color() as GifWord;
                this.AspectByte = 0;
                self.offset = 0;
            }
            Ok(())
        }

        fn current_image_buffer(&mut self) -> Result<(&[u8], &mut usize), DecodingError> {
            try!(self.seek_to(Progress::DataEnd));
            Ok((&self.decoder.current_frame().buffer, &mut self.offset))
        }


        fn seek_to(&mut self, position: Progress) -> Result<(), DecodingError> {
            self.read_until(position)
        }

        fn last_ext(&self) -> (u8, &[u8]) {
            self.decoder.last_ext()
        }

        fn next_record_type(&mut self) -> Result<Block, DecodingError> {
            try!(self.read_until(Progress::BlockStart));
            if let Some(block) = self.decoder._current_block() {
                Ok(block)
            } else {
                Err(DecodingError::Internal("Not at expected block."))
            }
        }

        unsafe fn read_to_end(&mut self, this: &mut c_api::GifFileType) -> Result<(), DecodingError> {
            try!(self.read_screen_desc(this));
            try!(self.read_to_end());
            this.ImageCount = self.frames().len() as c_int;
            let images = saved_images_new(this.ImageCount as usize);
            for (i, frame) in self.frames().iter().enumerate() {
                *images.offset(i as isize) = c_api::SavedImage {
                    ImageDesc: c_api::GifImageDesc {
                        Left: frame.left as GifWord,
                        Top: frame.top as GifWord,
                        Width: frame.width as GifWord,
                        Height: frame.height as GifWord,
                        Interlace: num::FromPrimitive::from_u8(frame.interlaced as u8).unwrap(),
                        ColorMap: copy_colormap(&frame.palette)
                    },
                    // on malloc(3) heap
                    RasterBits: copy_data(&*frame.buffer),
                    ExtensionBlockCount: 0,
                    ExtensionBlocks: ptr::null_mut()
                }
                
            }
            this.SavedImages = images;
            Ok(())
        }
    }
}