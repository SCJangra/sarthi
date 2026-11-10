use std::io;

use rmp::Marker;

/// Free space kept in the read buffer before each read, so a single read can take in a lot of data.
const READ_CHUNK: usize = 64 * 1024;

/// Splits a msgpack byte stream into complete top-level values ("frames") without decoding them.
///
/// Knowing where each message ends lets the caller deserialize it from a slice and skip it when it
/// doesn't match the expected types, without losing its place in the stream.
pub(crate) struct FrameReader<R> {
	reader: R,

	/// Read buffer. Its length is its usable size; only `buf[start..end]` holds unread data.
	buf: Vec<u8>,

	/// Start of the bytes that haven't been returned as a frame yet
	start: usize,

	/// End of the bytes read so far
	end: usize,
}

impl<R> FrameReader<R>
where
	R: io::Read,
{
	pub(crate) fn new(reader: R) -> Self {
		Self { reader, buf: vec![0; READ_CHUNK], start: 0, end: 0 }
	}

	/// Returns the next complete frame, or `None` once the stream ended after the last one.
	///
	/// Fails on I/O errors, when the stream ends in the middle of a frame, and on bytes that aren't valid msgpack.
	pub(crate) fn next_frame(&mut self) -> io::Result<Option<&[u8]>> {
		loop {
			if let Some(len) = frame_len(&self.buf[self.start..self.end])? {
				let frame = self.start..self.start + len;
				self.start += len;
				return Ok(Some(&self.buf[frame]));
			}

			if !self.fill()? {
				if self.start == self.end {
					return Ok(None);
				}
				return Err(io::ErrorKind::UnexpectedEof.into());
			}
		}
	}

	/// Moves the incomplete frame at the end of the buffer to the front and reads more data after it. Returns false at
	/// the end of the stream.
	fn fill(&mut self) -> io::Result<bool> {
		if self.start > 0 {
			self.buf.copy_within(self.start..self.end, 0);
			self.end -= self.start;
			self.start = 0;
		}

		// Growing zeroes the new space, but this only happens until the buffer fits the largest frame
		// seen plus `READ_CHUNK`; after that, reads reuse the same memory.
		let size = self.end + READ_CHUNK;
		if self.buf.len() < size {
			self.buf.resize(size, 0);
		}

		let read = loop {
			match self.reader.read(&mut self.buf[self.end..]) {
				Err(err) if err.kind() == io::ErrorKind::Interrupted => continue,
				read => break read?,
			}
		};

		self.end += read;
		Ok(read > 0)
	}
}

/// How a msgpack value continues after its marker byte.
enum Shape {
	/// `n` more bytes of data
	Data(usize),

	/// `n` nested values
	Items(u64),

	/// A big-endian length of `len_size` bytes, followed by `len + extra` bytes of data
	SizedData { len_size: usize, extra: usize },

	/// A big-endian length of `len_size` bytes, followed by `len * per_len` nested values
	SizedItems { len_size: usize, per_len: u64 },
}

/// Returns the length of the msgpack value at the start of `bytes`, or `None` if `bytes` ends
/// before the value does.
fn frame_len(bytes: &[u8]) -> io::Result<Option<usize>> {
	let mut pos = 0;

	// Values still to skip. Arrays and maps add their elements to it.
	let mut remaining: u64 = 1;

	while remaining > 0 {
		remaining -= 1;

		let Some(&marker) = bytes.get(pos) else { return Ok(None) };
		pos += 1;

		let shape = match Marker::from_u8(marker) {
			Marker::FixPos(_) | Marker::FixNeg(_) | Marker::Null | Marker::True | Marker::False => Shape::Data(0),
			Marker::U8 | Marker::I8 => Shape::Data(1),
			Marker::U16 | Marker::I16 => Shape::Data(2),
			Marker::U32 | Marker::I32 | Marker::F32 => Shape::Data(4),
			Marker::U64 | Marker::I64 | Marker::F64 => Shape::Data(8),
			Marker::FixStr(len) => Shape::Data(usize::from(len)),
			Marker::Str8 | Marker::Bin8 => Shape::SizedData { len_size: 1, extra: 0 },
			Marker::Str16 | Marker::Bin16 => Shape::SizedData { len_size: 2, extra: 0 },
			Marker::Str32 | Marker::Bin32 => Shape::SizedData { len_size: 4, extra: 0 },
			Marker::FixArray(len) => Shape::Items(u64::from(len)),
			Marker::Array16 => Shape::SizedItems { len_size: 2, per_len: 1 },
			Marker::Array32 => Shape::SizedItems { len_size: 4, per_len: 1 },
			Marker::FixMap(len) => Shape::Items(2 * u64::from(len)),
			Marker::Map16 => Shape::SizedItems { len_size: 2, per_len: 2 },
			Marker::Map32 => Shape::SizedItems { len_size: 4, per_len: 2 },
			// Extension values carry a one byte type before their data.
			Marker::FixExt1 => Shape::Data(1 + 1),
			Marker::FixExt2 => Shape::Data(1 + 2),
			Marker::FixExt4 => Shape::Data(1 + 4),
			Marker::FixExt8 => Shape::Data(1 + 8),
			Marker::FixExt16 => Shape::Data(1 + 16),
			Marker::Ext8 => Shape::SizedData { len_size: 1, extra: 1 },
			Marker::Ext16 => Shape::SizedData { len_size: 2, extra: 1 },
			Marker::Ext32 => Shape::SizedData { len_size: 4, extra: 1 },
			Marker::Reserved => {
				return Err(io::Error::new(io::ErrorKind::InvalidData, "reserved msgpack marker 0xc1"));
			},
		};

		match shape {
			Shape::Data(len) => pos += len,
			Shape::Items(len) => remaining += len,
			Shape::SizedData { len_size, extra } => {
				let Some(len) = read_len(bytes, pos, len_size) else { return Ok(None) };
				pos += len_size + len + extra;
			},
			Shape::SizedItems { len_size, per_len } => {
				let Some(len) = read_len(bytes, pos, len_size) else { return Ok(None) };
				pos += len_size;
				remaining += len as u64 * per_len;
			},
		}
	}

	Ok((pos <= bytes.len()).then_some(pos))
}

/// Reads the big-endian length field of `size` bytes at `pos`.
fn read_len(bytes: &[u8], pos: usize, size: usize) -> Option<usize> {
	let len = bytes.get(pos..pos + size)?;
	Some(len.iter().fold(0, |len, &byte| (len << 8) | usize::from(byte)))
}

#[cfg(test)]
mod tests {
	use std::io;

	use super::{FrameReader, READ_CHUNK, frame_len};

	#[test]
	fn frame_len_scalars() {
		assert_eq!(frame_len(&[0x01]).unwrap(), Some(1));
		assert_eq!(frame_len(&[0xC0]).unwrap(), Some(1));
		assert_eq!(frame_len(&[0xCD, 0x01, 0x02]).unwrap(), Some(3));
		assert_eq!(frame_len(&[0xA3, b'a', b'b', b'c']).unwrap(), Some(4));
		assert_eq!(frame_len(&[0xD9, 0x02, b'a', b'b']).unwrap(), Some(4));
		assert_eq!(frame_len(&[0xD4, 0x01, 0x02]).unwrap(), Some(3));
		assert_eq!(frame_len(&[0xC7, 0x02, 0x01, 0xAA, 0xBB]).unwrap(), Some(5));
	}

	#[test]
	fn frame_len_containers() {
		// [1, "a", {2: [3]}]
		let bytes = [0x93, 0x01, 0xA1, b'a', 0x81, 0x02, 0x91, 0x03];
		assert_eq!(frame_len(&bytes).unwrap(), Some(bytes.len()));

		// array16 with two elements
		assert_eq!(frame_len(&[0xDC, 0x00, 0x02, 0x01, 0x02]).unwrap(), Some(5));
	}

	#[test]
	fn frame_len_stops_at_first_value() {
		assert_eq!(frame_len(&[0x91, 0x01, 0x02, 0x03]).unwrap(), Some(2));
	}

	#[test]
	fn frame_len_incomplete() {
		assert_eq!(frame_len(&[]).unwrap(), None);
		assert_eq!(frame_len(&[0x92, 0x01]).unwrap(), None);
		assert_eq!(frame_len(&[0xA3, b'a']).unwrap(), None);
		assert_eq!(frame_len(&[0xDA, 0x00]).unwrap(), None);
		assert_eq!(frame_len(&[0xDC, 0x00, 0x02, 0x01]).unwrap(), None);
	}

	#[test]
	fn frame_len_reserved_marker() {
		assert_eq!(frame_len(&[0xC1]).unwrap_err().kind(), io::ErrorKind::InvalidData);
	}

	/// Hands out the stream one byte at a time, so frames always span several reads.
	struct ByteReader(io::Cursor<Vec<u8>>);

	impl io::Read for ByteReader {
		fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
			let len = buf.len().min(1);
			self.0.read(&mut buf[..len])
		}
	}

	#[test]
	fn reader_splits_frames_across_reads() {
		let stream = vec![0x92, 0x01, 0xA1, b'a', 0xC3, 0x91, 0x02];
		let mut frames = FrameReader::new(ByteReader(io::Cursor::new(stream)));

		assert_eq!(frames.next_frame().unwrap().unwrap(), [0x92, 0x01, 0xA1, b'a']);
		assert_eq!(frames.next_frame().unwrap().unwrap(), [0xC3]);
		assert_eq!(frames.next_frame().unwrap().unwrap(), [0x91, 0x02]);
		// The stream ended between frames.
		assert_eq!(frames.next_frame().unwrap(), None);
	}

	#[test]
	fn reader_eof_mid_frame() {
		let mut frames = FrameReader::new(io::Cursor::new(vec![0x92, 0x01]));
		assert_eq!(frames.next_frame().unwrap_err().kind(), io::ErrorKind::UnexpectedEof);
	}

	#[test]
	fn reader_grows_for_large_frame() {
		// str32 longer than the initial buffer, followed by a small frame
		let len = READ_CHUNK * 3 / 2;
		let mut stream = vec![0xDB];
		stream.extend_from_slice(&(len as u32).to_be_bytes());
		stream.resize(stream.len() + len, b'a');
		stream.push(0xC3);

		let mut frames = FrameReader::new(io::Cursor::new(stream));

		assert_eq!(frames.next_frame().unwrap().unwrap().len(), 1 + 4 + len);
		assert_eq!(frames.next_frame().unwrap().unwrap(), [0xC3]);
	}
}
