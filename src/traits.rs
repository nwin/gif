//! Traits used in this library
use std::io;

/// Configuration parameter trait
pub trait Parameter<Object> {
    fn set_param(self, &mut Object);
}

/// Object has parameters
pub trait HasParameters: Sized {
    fn set<T: Parameter<Self>>(&mut self, value: T) -> &mut Self {
        value.set_param(self);
        self
    }
}

/// Writer extesion to write little endian data
pub trait WriteBytesExt<T> {
	fn write_le(&mut self, n: T) -> io::Result<()>;

	/*
	#[inline]
	fn write_byte(&mut self, n: u8) -> io::Result<()> where Self: Write {
		self.write_all(&[n])
	}
	*/
}

impl<W: io::Write + ?Sized> WriteBytesExt<u8> for W {
	#[inline]
	fn write_le(&mut self, n: u8) -> io::Result<()> {
		self.write_all(&[n])
		
	}
}

impl<W: io::Write + ?Sized> WriteBytesExt<u16> for W {
	#[inline]
	fn write_le(&mut self, n: u16) -> io::Result<()> {
		self.write_all(&[n as u8, (n>>8) as u8])
		
	}
}

impl<W: io::Write + ?Sized> WriteBytesExt<u32> for W {
	#[inline]
	fn write_le(&mut self, n: u32) -> io::Result<()> {
		try!(self.write_le(n as u16));
		self.write_le((n >> 16) as u16)
		
	}
}

impl<W: io::Write + ?Sized> WriteBytesExt<u64> for W {
	#[inline]
	fn write_le(&mut self, n: u64) -> io::Result<()> {
		try!(self.write_le(n as u32));
		self.write_le((n >> 32) as u32)
		
	}
}