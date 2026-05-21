use thiserror::Error;

pub const MAX_BITS: usize = 53;

#[derive(Debug, Error)]
pub enum BitsError {
  #[error("Cannot set offset below 0")]
  OffsetBelowZero,
  #[error("Cannot set offset to {val}, buffer length is {length}")]
  OffsetExceedsLength { val: usize, length: usize },
  #[error("Cannot write {size} bits, only {remaining} bit(s) left")]
  WriteOverflow { size: usize, remaining: usize },
  #[error("Cannot write {size} bits, max is 53")]
  WriteSizeExceedsMax { size: usize },
  #[error("Cannot read {size} bits, only {remaining} bit(s) left")]
  ReadOverflow { size: usize, remaining: usize },
  #[error("Reading {size} bits would overflow result, max is 53")]
  ReadSizeExceedsMax { size: usize },
}

#[derive(Debug, Clone)]
pub struct Bits {
  pub buffer: Vec<u8>,
  length: usize,
  offset: usize,
}

impl Bits {
  pub fn new(bits: usize) -> Self {
    let byte_count = bits.div_ceil(8);
    let buffer = vec![0u8; byte_count];
    let length = 8 * byte_count;
    Self {
      buffer,
      length,
      offset: 0,
    }
  }

  pub fn from_bytes(buffer: Vec<u8>) -> Self {
    let length = 8 * buffer.len();
    Self {
      buffer,
      length,
      offset: 0,
    }
  }

  pub fn alloc(size: usize, fill: u8) -> Self {
    Self::from_bytes(vec![fill; size])
  }

  pub fn eof(&self) -> bool {
    self.offset == self.length
  }

  pub fn length(&self) -> usize {
    self.length
  }

  pub fn offset(&self) -> usize {
    self.offset
  }

  pub fn set_offset(&mut self, val: isize) -> Result<&mut Self, BitsError> {
    if val < 0 {
      return Err(BitsError::OffsetBelowZero);
    }
    let val = val as usize;
    if val > self.length {
      return Err(BitsError::OffsetExceedsLength {
        val,
        length: self.length,
      });
    }
    self.offset = val;
    Ok(self)
  }

  pub fn remaining(&self) -> usize {
    self.length - self.offset
  }

  pub fn clear(&mut self, fill: u8) -> &mut Self {
    self.buffer.fill(fill);
    self.offset = 0;
    self
  }

  pub fn clear_bit(&mut self, pos: usize) -> Result<&mut Self, BitsError> {
    self.insert(0, 1, Some(pos))?;
    Ok(self)
  }

  pub fn flip_bit(&mut self, pos: usize) -> Result<u64, BitsError> {
    let bit = 1 ^ self.peek(1, Some(pos))?;
    self.modify_bit(bit, pos)?;
    Ok(bit)
  }

  pub fn get_bit(&self, pos: usize) -> Result<u64, BitsError> {
    self.peek(1, Some(pos))
  }

  pub fn insert(
    &mut self,
    value: u64,
    size: usize,
    offset: Option<usize>,
  ) -> Result<usize, BitsError> {
    let mut r = offset.unwrap_or(self.offset);
    if r + size > self.length {
      return Err(BitsError::WriteOverflow {
        size,
        remaining: self.remaining(),
      });
    }
    if size > MAX_BITS {
      return Err(BitsError::WriteSizeExceedsMax { size });
    }

    let mut remaining = size;
    while remaining > 0 {
      let byte_index = r >> 3;
      let bit_index = r & 7;
      let chunk_size = (8 - bit_index).min(remaining);
      let mask = (1u64 << chunk_size) - 1;
      let shift = 8 - chunk_size - bit_index;
      let chunk = ((value >> (remaining - chunk_size)) & mask) << shift;

      self.buffer[byte_index] = (self.buffer[byte_index] & !((mask << shift) as u8)) | chunk as u8;

      r += chunk_size;
      remaining -= chunk_size;
    }
    Ok(r)
  }

  pub fn modify_bit(&mut self, value: u64, pos: usize) -> Result<&mut Self, BitsError> {
    self.insert(value, 1, Some(pos))?;
    Ok(self)
  }

  pub fn peek(&self, size: usize, offset: Option<usize>) -> Result<u64, BitsError> {
    let mut r = offset.unwrap_or(self.offset);
    if r + size > self.length {
      return Err(BitsError::ReadOverflow {
        size,
        remaining: self.remaining(),
      });
    }
    if size > MAX_BITS {
      return Err(BitsError::ReadSizeExceedsMax { size });
    }

    let bit_index = r & 7;
    let first_size = (8 - bit_index).min(size);
    let mask = (1u64 << first_size) - 1;

    let mut result = (self.buffer[r >> 3] as u64 >> (8 - first_size - bit_index)) & mask;
    r += first_size;

    let mut remaining = size - first_size;
    while remaining >= 8 {
      result = (result << 8) | self.buffer[r >> 3] as u64;
      r += 8;
      remaining -= 8;
    }

    if remaining > 0 {
      let shift = 8 - remaining;
      result = (result << remaining) | ((self.buffer[r >> 3] as u64 >> shift) & (255u64 >> shift));
    }

    Ok(result)
  }

  pub fn read(&mut self, size: usize) -> Result<u64, BitsError> {
    let value = self.peek(size, Some(self.offset))?;
    self.offset += size;
    Ok(value)
  }

  pub fn seek(&mut self, val: isize, whence: u8) -> Result<&mut Self, BitsError> {
    match whence {
      2 => {
        let new_offset = self.offset as isize + val;
        self.set_offset(new_offset)?;
      }
      3 => {
        let new_offset = self.length as isize - val;
        self.set_offset(new_offset)?;
      }
      _ => {
        self.set_offset(val)?;
      }
    }
    Ok(self)
  }

  pub fn set_bit(&mut self, pos: usize) -> Result<&mut Self, BitsError> {
    self.insert(1, 1, Some(pos))?;
    Ok(self)
  }

  pub fn skip(&mut self, size: isize) -> Result<&mut Self, BitsError> {
    self.seek(size, 2)
  }

  pub fn test_bit(&self, pos: usize) -> Result<bool, BitsError> {
    Ok(self.peek(1, Some(pos))? != 0)
  }

  pub fn write(&mut self, value: u64, size: usize) -> Result<&mut Self, BitsError> {
    self.offset = self.insert(value, size, Some(self.offset))?;
    Ok(self)
  }

  pub fn into_bytes(self) -> Vec<u8> {
    self.buffer
  }
}
