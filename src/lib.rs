#![feature(collections)]
#![feature(core)]
#![feature(box_syntax)]
#![cfg_attr(test, feature(test))]
#![feature(alloc)]
#![feature(libc)]

#[cfg(feature = "c_api")]
extern crate libc;
extern crate lzw;
extern crate num;

#[macro_use] extern crate enum_primitive;

mod traits;
mod reader;
mod writer;

#[cfg(feature = "c_api")]
mod c_api_utils;
#[cfg(feature = "c_api")]
pub mod c_api;

pub use traits::HasParameters;

pub use reader::{Decoder, Progress, Decoded, DecodingError, Frame};
/// Decoder configuration parameters
pub use reader::{ColorOutput, Extensions};
pub use reader::{Block, Extension, DisposalMethod};
pub use reader::Reader;

pub use writer::{Encoder, HeaderWritten, ExtensionData};

#[cfg(test)]
#[test]
fn round_trip() {
	use std::io::prelude::*;
	use std::fs::File;
	let mut data = vec![];
	File::open("tests/samples/sample_1.gif").unwrap().read_to_end(&mut data).unwrap();
	let mut decoder = Reader::new(&*data);
    let frame = &decoder.read_to_end().unwrap()[0];
	let mut data2 = vec![];
	{
		let mut encoder = Encoder::new(&mut data2, frame.width, frame.height);
		let _ = encoder.write_frame(frame);
	}

}