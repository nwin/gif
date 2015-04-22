use std::io;
use std::io::prelude::*;

use lzw;

use {Block, Frame, Extension, DisposalMethod};

pub enum ExtensionData {
	Control { flags: u8, delay: u16, trns: u8 }
}

impl ExtensionData {
	pub fn new_control_ext(delay: u16, dispose: DisposalMethod, 
						   needs_user_input: bool, trns: Option<usize>) -> ExtensionData {
		let mut flags = 0;
		let trns = match trns {
			Some(trns) => {
				flags |= 1;
				trns as u8
			},
			None => 0
		};
		flags |= (needs_user_input as u8) << 1;
		flags |= (dispose as u8) << 2;
		ExtensionData::Control {
			flags: flags,
			delay: delay,
			trns: trns
		}
	}
}

trait WriteBytesExt<T> {
	fn write_le(&mut self, n: T) -> io::Result<()>;
}

impl<W> WriteBytesExt<u16> for W where W: Write {
	fn write_le(&mut self, n: u16) -> io::Result<()> {
		self.write_all(&[n as u8, (n>>8) as u8])
		
	}
}

impl<W> WriteBytesExt<u8> for W where W: Write {
	fn write_le(&mut self, n: u8) -> io::Result<()> {
		self.write_all(&[n])
	}
}

pub struct Encoder<'a, W: Write + 'a> {
	w: &'a mut W,
	header_written: bool,
	global_palette: bool,
	width: u16,
	height: u16
}
/*

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

*/
impl<'a, W: Write + 'a> Encoder<'a, W> {
	pub fn new(w: &'a mut W, width: u16, height: u16) -> Self {
		Encoder {
			w: w,
			header_written: false,
			global_palette: false,
			width: width,
			height: height
		}
	}

	/// Writes a complete frame to the image
	///
	/// Note: This function also writes a control extention if necessary.
	pub fn write_frame(&mut self, frame: &Frame) -> io::Result<()> {
		try!(self.write_screen_desc());
		if frame.delay > 0 || frame.transparent.is_some() {
			try!(self.write_extension(ExtensionData::new_control_ext(
				frame.delay,
				frame.dispose,
				frame.needs_user_input,
				frame.transparent

			)))
		}
		try!(self.w.write_le(Block::Image as u8));
		try!(self.w.write_le(frame.left));
		try!(self.w.write_le(frame.top));
		try!(self.w.write_le(frame.width));
		try!(self.w.write_le(frame.height));
		let mut flags = 0;
		try!(match frame.palette {
			Some(ref palette) => {
				flags |= 0b1000_0000;
				flags |= flag_size(palette.len());
				try!(self.w.write_le(flags));
				self.write_color_table(palette)
			},
			None => if !self.global_palette {
				return Err(io::Error::new(
					io::ErrorKind::InvalidInput,
					"The GIF format requires a color palette but none was given."
				))
			} else {
				self.w.write_le(flags)
			}
		});
		self.write_image_block(&frame.buffer)
	}

	fn write_image_block(&mut self, data: &[u8]) -> io::Result<()> {
		{
			let min_code_size: u8 = flag_size((*data.iter().max().unwrap_or(&0) + 1) as usize) + 1;
			try!(self.w.write_le(min_code_size));
			let mut enc = try!(lzw::Encoder::new(lzw::LsbWriter::new(&mut self.w), min_code_size));
			try!(enc.encode_bytes(data));
		}
		self.w.write_le(0u8)
	}

	fn write_color_table(&mut self, table: &[u8]) -> io::Result<()> {
		let num_colors = table.len() / 3;
        let size = flag_size(num_colors);
		try!(self.w.write_all(&table[..num_colors * 3]));
        // Waste some space as of gif spec
        for _ in 0..((2 << size) - num_colors) {
            try!(self.w.write_all(&[0, 0, 0]))
        }
        Ok(())
	}

	/// Writes an extension to the image
	pub fn write_extension(&mut self, extension: ExtensionData) -> io::Result<()> {
		use self::ExtensionData::*;
		try!(self.write_screen_desc());
		try!(self.w.write_le(Block::Extension as u8));
		match extension {
			Control { flags, delay, trns } => {
				try!(self.w.write_le(Extension::Control as u8));
				try!(self.w.write_le(4u8));
				try!(self.w.write_le(flags));
				try!(self.w.write_le(delay));
				try!(self.w.write_le(trns));
			}
		}
		self.w.write_le(0u8)
	}

	/// Writes an extension to the image
	pub fn write_raw_extension(&mut self, func: u8, data: &[u8]) -> io::Result<()> {
		try!(self.write_screen_desc());
		try!(self.w.write_le(Block::Extension as u8));
		try!(self.w.write_le(func as u8));
		for chunk in data.chunks(0xFF) {
			try!(self.w.write_le(chunk.len() as u8));
			try!(self.w.write_all(chunk));
		}
		self.w.write_le(0u8)
	}

	/// Writes the logical screen desriptor
	fn write_screen_desc(&mut self) -> io::Result<()> {
		if !self.header_written {
			try!(self.w.write_all(b"GIF89a"));
			try!(self.w.write_le(self.width));
			try!(self.w.write_le(self.height));
			try!(self.w.write_le(0u8)); // packed field
			try!(self.w.write_le(0u8)); // bg index
			try!(self.w.write_le(0u8)); // aspect ratio
			self.header_written = true;
		}
		Ok(())
	}
}

// Color table size converted to flag bits
fn flag_size(size: usize) -> u8 {
    match size {
        0  ...2   => 0,
        3  ...4   => 1,
        5  ...8   => 2,
        7  ...16  => 3,
        17 ...32  => 4,
        33 ...64  => 5,
        65 ...128 => 6,
        129...256 => 7,
        _ => 7
    }
}

impl<'a, W: Write + 'a> Drop for Encoder<'a, W> {
	fn drop(&mut self) {
		let _ = self.w.write_le(Block::Trailer as u8);
	}
}