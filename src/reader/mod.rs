use std::io;
use std::mem;
use std::io::prelude::*;

use traits::{HasParameters, Parameter};

mod decoder;
pub use self::decoder::{
    Decoder, Progress, Decoded, DecodingError,
    Frame, DisposalMethod, Block,
    ColorOutput, Extensions,
    N_CHANNELS, PLTE_CHANNELS
};

impl<T, R> Parameter<Reader<R>> for T
where T: Parameter<Decoder>, R: Read {
    fn set_param(self, this: &mut Reader<R>) {
        this.decoder.set(self);
    }

}

pub struct Reader<R: Read> {
    r: io::BufReader<R>,
    decoder: Decoder,
    // Offset in current frame
    offset: usize

}

impl<R: Read> HasParameters for Reader<R> {}

impl<R> Reader<R> where R: Read {
    pub fn new(reader: R) -> Reader<R> {
        Reader {
            r: io::BufReader::new(reader),
            decoder: Decoder::new(),
            offset: 0
        }
    }
    
    pub fn read_to_end(&mut self) -> Result<&[Frame], DecodingError> {
        try!(self.read_until(Progress::Trailer));
        Ok(self.decoder.frames())
    }

    /// Width of the image
    pub fn width(&self) -> u16 {
        self.decoder.width()
    }

    /// Height of the image
    pub fn height(&self) -> u16 {
        self.decoder.height()
    }

    /// The global color palette
    pub fn global_palette(&self) -> &[u8] {
        self.decoder.global_palette()
    }

    /// Index of the background color in the global palette
    pub fn bg_color(&self) -> usize {
        self.decoder.bg_color()
    }

    fn decode_next(&mut self) -> Result<Decoded, DecodingError> {
        loop {
            let (consumed, state) = {
                let buf = try!(self.r.fill_buf());
                if buf.len() == 0 {
                    return Err(DecodingError::Format(
                        "unexpected EOF"
                    ))
                }
                try!(self.decoder.decode_bytes(buf))
            };
            self.r.consume(consumed);
            match state {
                Some(state) => return Ok(unsafe{
                    // FIXME: #6393
                    mem::transmute::<Decoded, Decoded>(state)
                }),
                None => (),
            }
        }
    }

    fn read_until(&mut self, stop_at: Progress) -> Result<(), DecodingError> {
        while self.decoder.progress() != stop_at {
            let consumed = {
                let buf = try!(self.r.fill_buf());
                if buf.len() == 0 {
                    return Err(DecodingError::Format(
                        "unexpected EOF"
                    ))
                }
                try!(self.decoder.update_until(buf, stop_at))
            };
            self.r.consume(consumed);
        }
        Ok(())
    }
}

#[cfg(feature = "c_api")]
mod c_interface {
    use std::io::prelude::*;
    use std::ptr;
    use std::num;

    use libc::c_int;
    
    use c_api::{self, GifWord};
    use c_api_utils::{CInterface, copy_colormap, copy_data, saved_images_new};

    use super::decoder::{Block, Progress, DecodingError};

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
            this.ImageCount = self.decoder.frames().len() as c_int;
            let images = saved_images_new(this.ImageCount as usize);
            for (i, frame) in self.decoder.frames().iter().enumerate() {
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