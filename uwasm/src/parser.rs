pub(crate) struct Reader<'code> {
    data: &'code [u8],
    pos: usize,
}

impl<'code> Reader<'code> {
    pub(crate) fn new(data: &'code [u8]) -> Self {
        Self {
            data,
            pos: 0,
        }
    }

    pub(crate) fn read_bytes<const N: usize>(&mut self) -> Result<&'code [u8; N], ParserError> {
        if self.pos + N <= self.data.len() {
            let bytes = self.data[self.pos..][..N].try_into()
                .expect("enough bytes in stream");
            self.pos += N;
            Ok(bytes)
        } else {
            Err(ParserError::EndOfStream { offset: self.pos })
        }
    }

    pub(crate) fn read_slice(&mut self, n: usize) -> Result<&'code [u8], ParserError> {
        if self.pos + n <= self.data.len() {
            let bytes = &self.data[self.pos..][..n];
            self.pos += n;
            Ok(bytes)
        } else {
            Err(ParserError::EndOfStream { offset: self.pos })
        }
    }

    #[inline]
    pub(crate) fn read_u8(&mut self) -> Result<u8, ParserError> {
        self.read_bytes::<1>().map(|b| b[0])
    }

    #[inline]
    pub(crate) fn read_u32(&mut self) -> Result<u32, ParserError> {
        self.read_bytes::<4>().map(|b| u32::from_le_bytes(*b))
    }

    #[inline]
    pub(crate) fn read_usize(&mut self) -> Result<usize, ParserError> {
        // TODO: support LEB128 encoding
        self.read_u8().map(|b| b as usize)
    }

    #[inline]
    pub(crate) fn read<T: Item>(&mut self) -> Result<T, ParserError> {
        T::read(self, self.pos)
    }
}

pub(crate) trait Item: Sized {
    fn read(reader: &mut Reader, offset: usize) -> Result<Self, ParserError>;
}


#[derive(Copy, Clone, Debug)]
#[repr(u8)]
pub(crate) enum SectionKind {
    Custom = 0x00,
    Type = 0x01,
    Function = 0x03,
    Export = 0x07,
    Code = 0x0A,
}

impl Item for SectionKind {
    fn read(reader: &mut Reader, offset: usize) -> Result<Self, ParserError> {
        match reader.read_u8()? {
            0x00 => Ok(SectionKind::Custom),
            0x01 => Ok(SectionKind::Type),
            0x03 => Ok(SectionKind::Function),
            0x07 => Ok(SectionKind::Export),
            0x0A => Ok(SectionKind::Code),
            _ => Err(ParserError::InvalidValue { offset }),
        }
    }
}

#[derive(Debug)]
#[repr(u8)]
pub(crate) enum TypeKind {
    Func = 0x60,
    I32 = 0x7F,
}

impl Item for TypeKind {
    fn read(reader: &mut Reader, offset: usize) -> Result<Self, ParserError> {
        match reader.read_u8()? {
            0x60 => Ok(TypeKind::Func),
            0x7F => Ok(TypeKind::I32),
            _ => Err(ParserError::InvalidValue { offset }),
        }
    }
}

#[derive(Debug, Eq, PartialEq)]
pub enum ParserError {
    EndOfStream { offset: usize },
    InvalidValue { offset: usize },
}
