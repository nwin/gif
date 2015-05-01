#![feature(collections)]
#![feature(core)]
#![feature(box_syntax)]
#![cfg_attr(test, feature(test))]
#![feature(alloc)]

#[cfg(feature = "c_api")]
extern crate libc;
extern crate lzw;
extern crate num;

#[macro_use] extern crate enum_primitive;

mod traits;
mod types;
mod reader;
mod encoder;

#[cfg(feature = "c_api")]
mod c_api_utils;
#[cfg(feature = "c_api")]
pub mod c_api;

pub use traits::HasParameters;
pub use types::{Block, Extension, DisposalMethod, Frame};

pub use reader::{Decoder, Progress, Decoded, DecodingError};
/// Decoder configuration parameters
pub use reader::{ColorOutput, Extensions};
pub use reader::Reader;

pub use encoder::{Encoder, HeaderWritten, ExtensionData};

#[cfg(test)]
#[test]
fn round_trip() {
	use std::io::prelude::*;
	use std::fs::File;
	let mut data = vec![];
	File::open("tests/samples/sample_1.gif").unwrap().read_to_end(&mut data).unwrap();
	let mut decoder = Reader::new(&*data);
	let _ = decoder.read_to_end().unwrap();
	let mut data2 = vec![];
	{
    	let encoder = {
    		let frame = &decoder.frames()[0];
    		Encoder::new(&mut data2, frame.width, frame.height)
    	};
		let mut encoder = encoder.write_global_palette(decoder.global_palette()).unwrap();
		let frame = &decoder.frames()[0];
		
		encoder.write_frame(frame).unwrap();
	}
	assert_eq!(&data[..], &data2[..])
}