use crate::str::ByteStr;

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

    pub(crate) fn pos(&self) -> usize {
        self.pos
    }

    pub(crate) fn skip_to(&mut self, target_offset: usize) {
        self.pos = target_offset;
    }

    pub(crate) fn read_bytes<const N: usize>(&mut self) -> Result<&'code [u8; N], ParserError> {
        if let Some(bytes) = self.data[self.pos..].first_chunk() {
            self.pos += N;
            Ok(bytes)
        } else {
            Err(ParserError::EndOfStream { offset: self.pos })
        }
    }

    pub(crate) fn expect_bytes<const N: usize>(&mut self, expected_bytes: &[u8; N]) -> Result<(), ParserError> {
        if let Some(bytes) = self.data[self.pos..].first_chunk() {
            if bytes == expected_bytes {
                self.pos += N;
                Ok(())
            } else {
                Err(ParserError::UnexpectedBytes { offset: self.pos })
            }
        } else {
            Err(ParserError::NotEnoughBytes { offset: self.pos })
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
    pub(crate) fn read_f64(&mut self) -> Result<f64, ParserError> {
        self.read_bytes().map(|b| f64::from_le_bytes(*b))
    }

    #[inline]
    pub(crate) fn read<T: Item>(&mut self) -> Result<T, ParserError> {
        T::read(self, self.pos)
    }

    pub(crate) fn read_str(&mut self) -> Result<&'code ByteStr, ParserError> {
        let len = self.read_usize()?;
        let bytes = self.read_slice(len)?;

        // SAFETY: ByteStr has the same layout as [u8]
        Ok(unsafe { core::mem::transmute(bytes) })
    }

    pub(crate) fn marker(&mut self) -> Marker<'code> {
        Marker {
            data: self.data,
            start: self.pos,
        }
    }
}

pub(crate) struct Marker<'code> {
    data: &'code [u8],
    start: usize,
}

impl<'code> Marker<'code> {
    pub(crate) fn pos(&self) -> usize {
        self.start
    }

    pub(crate) fn into_slice(self, reader: &mut Reader<'code>) -> &'code [u8] {
        &self.data[self.start..reader.pos]
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
#[derive(Clone, Copy)]
#[repr(u8)]
pub(crate) enum TypeKind {
    Func = 0x60,
    F64 = 0x7c,
    I32 = 0x7F,
}

impl TypeKind {
    pub(crate) fn len_bytes(&self) -> usize {
        match *self {
            TypeKind::Func => todo!(),
            TypeKind::F64 => 8,
            TypeKind::I32 => 4,
        }
    }
}

impl Item for TypeKind {
    fn read(reader: &mut Reader, offset: usize) -> Result<Self, ParserError> {
        match reader.read_u8()? {
            0x60 => Ok(TypeKind::Func),
            0x7C => Ok(TypeKind::F64),
            0x7F => Ok(TypeKind::I32),
            _ => Err(ParserError::InvalidValue { offset }),
        }
    }
}

#[derive(Debug, Eq, PartialEq)]
pub enum ParserError {
    EndOfStream { offset: usize },
    InvalidValue { offset: usize },
    UnexpectedBytes { offset: usize },
    NotEnoughBytes { offset: usize },
}
