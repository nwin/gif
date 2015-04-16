#![feature(collections)]
#![feature(core)]
#![feature(io)]
#![feature(box_syntax)]
#![cfg_attr(test, feature(test))]
#![feature(libc)]
#![feature(alloc)]

extern crate libc;
extern crate lzw;

mod traits;
mod reader;

#[cfg(feature = "c_api")]
mod c_api_utils;
#[cfg(feature = "c_api")]
pub mod c_api;

pub use traits::HasParameters;

pub use reader::{Decoder, Progress, ColorOutput, DecodingError};
pub use reader::{Frame, DisposalMethod};
pub use reader::Reader;
