//! # GIF encoding and decoding library
//!
//! This library provides all functions necessary to decode and encode GIF files. 
//! 
//! ## High level interface
//! 
//! The high level interface is very simple to use but can be memory intensive
//! since the whole image is decoded at once. It is based on the two types
//! [`Encoder`](struct.Encoder.html) and [`Decoder`](struct.Decoder.html).
//! 
//! ### Decoding GIF files
//! 
//! TODO
//! 
//! ### Encoding GIF files
//! 
//! ```
//! use gif::{Frame, Encoder};
//! use std::fs::File;
//! 
//! let color_map = &[0, 0, 0, 0xFF, 0xFF, 0xFF];
//! let mut frame = Frame::default();
//! // Generate checkerboard lattice
//! for (i, j) in (0..10).zip(0..10) {
//! 	frame.buffer.push(if (i * j) % 2 == 0 {
//! 		1
//! 	} else {
//! 		0
//! 	})
//! }
//! let mut file = File::create("test.gif").unwrap();
//! let mut encoder = Encoder::new(&mut file, 100, 100);
//! encoder.write_global_palette(color_map).unwrap().write_frame(&frame).unwrap();
//! ```
//! 
//! ## C API

// TODO: make this compile
// ```
// use gif::{Frame, Encoder};
// use std::fs::File;
// let color_map = &[0, 0, 0, 0xFF, 0xFF, 0xFF];
// let mut frame = Frame::default();
// // Generate checkerboard lattice
// for (i, j) in (0..10).zip(0..10) {
// 	frame.buffer.push(if (i * j) % 2 == 0 {
// 		1
// 	} else {
// 		0
// 	})
// }
// # (|| {
// {
// let mut file = try!(File::create("test.gif"));
// let mut encoder = Encoder::new(&mut file, 100, 100);
// try!(encoder.write_global_palette(color_map)).write_frame(&frame)
// }
// # })().unwrap();
// ```

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