use core::mem::MaybeUninit;

#[derive(Debug, Clone)]
pub enum BufferError {
    // Trying to allocate more fields that the buffer has space for.
    LenOverflow,
    // Trying to allocate more data than the buffer has space for.
    SizeOverflow,
    WrongFieldIndex,
    WrongFieldSize,
    Utf8Error(core::str::Utf8Error),
}

pub struct Buffer<const SIZE: usize, const MAX_LEN_P1: usize> {
    data: [u8; SIZE],
    offsets: [usize; MAX_LEN_P1],
    len: usize,
}

impl<const SIZE: usize, const MAX_LEN_P1: usize> Buffer<SIZE, MAX_LEN_P1> {
    pub fn new() -> Self {
        let mut buf = Buffer {
            data: unsafe { MaybeUninit::uninit().assume_init() },
            offsets: unsafe { MaybeUninit::uninit().assume_init() },
            len: 0,
        };
        buf.offsets[0] = 0;
        buf
    }


    fn get_field_fixed_size<const FIELD_SIZE: usize>(
        &self,
        index: usize,
    ) -> Result<[u8; FIELD_SIZE], BufferError> {
        if index >= self.len {
            return Err(BufferError::WrongFieldIndex);
        }
        if self.offsets[index + 1] - self.offsets[index] == FIELD_SIZE {
            let mut array = [0; FIELD_SIZE];
            array.clone_from_slice(&self.data[self.offsets[index]..self.offsets[index + 1]]);
            Ok(array)
        } else {
            Err(BufferError::WrongFieldSize)
        }
    }
}

pub trait GenBuffer {
    fn add_field(&mut self, field_size: usize) -> Result<&mut [u8], BufferError>;

    fn field_as_u8(&self, index: usize) -> Result<u8, BufferError>;

    fn field_as_i32(&self, index: usize) -> Result<i32, BufferError>;

    fn field_as_str(&self, index: usize) -> Result<&str, BufferError>;

    fn field_as_slice_fixed(&self, index: usize, expected_size: usize) -> Result<&[u8], BufferError>;

    fn len(&self) -> usize;
}

impl<const SIZE: usize, const MAX_LEN_P1: usize> GenBuffer for Buffer<SIZE, MAX_LEN_P1> {
    fn add_field(&mut self, field_size: usize) -> Result<&mut [u8], BufferError> {
        if self.len >= MAX_LEN_P1 - 1 {
            return Err(BufferError::LenOverflow);
        }
        if self.offsets[self.len] + field_size > SIZE {
            return Err(BufferError::SizeOverflow);
        }

        self.offsets[self.len + 1] = self.offsets[self.len] + field_size;
        self.len += 1;

        Ok(&mut self.data[self.offsets[self.len - 1]..self.offsets[self.len]])
    }

    fn field_as_u8(&self, index: usize) -> Result<u8, BufferError> {
        let field = self.get_field_fixed_size::<1>(index)?;
        Ok(field[0])
    }

    fn field_as_i32(&self, index: usize) -> Result<i32, BufferError> {
        let field = self.get_field_fixed_size::<4>(index)?;

        Ok(i32::from_ne_bytes(field))
    }

    fn field_as_str(&self, index: usize) -> Result<&str, BufferError> {
        if index >= self.len {
            return Err(BufferError::WrongFieldIndex);
        }

        core::str::from_utf8(&self.data[self.offsets[index]..self.offsets[index + 1]])
            .map_err(|e| BufferError::Utf8Error(e))
    }

    fn field_as_slice_fixed(&self, index: usize, expected_size: usize) -> Result<&[u8], BufferError> {
        if index >= self.len {
            return Err(BufferError::WrongFieldIndex);
        }
        if self.offsets[index + 1] - self.offsets[index] == expected_size {
            Ok(&self.data[self.offsets[index] .. self.offsets[index + 1]])
        } else {
            Err(BufferError::WrongFieldSize)
        }
    }

    fn len(&self) -> usize {
        self.len
    }
}
