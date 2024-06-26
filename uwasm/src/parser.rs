use core::fmt;
use core::fmt::{Display, Formatter};
use crate::str::ByteStr;

#[derive(Clone)]
pub(crate) struct Reader<'code> {
    data: &'code [u8],
    pos: usize,
}

impl fmt::Debug for Reader<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:02X?}", &self.data[self.pos - 2..][..100])
    }
}

impl<'code> Reader<'code> {
    pub(crate) fn new(data: &'code [u8]) -> Self {
        Self { data, pos: 0 }
    }

    pub(crate) fn pos(&self) -> usize {
        self.pos
    }

    pub(crate) fn skip_to(&mut self, target_offset: usize) {
        self.pos = target_offset;
    }

    pub(crate) fn skip_to_end(&mut self) {
        self.pos = self.data.len();
    }

    pub(crate) fn read_bytes<const N: usize>(&mut self) -> Result<&'code [u8; N], ParserError> {
        if let Some(bytes) = self.data[self.pos..].first_chunk() {
            self.pos += N;
            Ok(bytes)
        } else {
            Err(ParserError::EndOfStream { offset: self.pos })
        }
    }

    pub(crate) fn expect_bytes<const N: usize>(
        &mut self,
        expected_bytes: &[u8; N],
    ) -> Result<(), ParserError> {
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
    #[cfg(debug_assertions)]
    pub(crate) fn read_i32(&mut self) -> Result<i32, ParserError> {
        self.read_u32().map(|v| v as i32)
    }

    #[inline]
    #[cfg(debug_assertions)]
    pub(crate) fn read_u64(&mut self) -> Result<u64, ParserError> {
        self.read_bytes::<8>().map(|b| u64::from_le_bytes(*b))
    }

    #[inline]
    #[cfg(debug_assertions)]
    pub(crate) fn read_i64(&mut self) -> Result<i64, ParserError> {
        self.read_u64().map(|v| v as i64)
    }

    #[inline]
    pub(crate) fn read_usize(&mut self) -> Result<usize, ParserError> {
        let val = self.read_unsigned()?;
        Ok(val as usize)
    }

    #[inline]
    pub(crate) fn read_isize(&mut self) -> Result<isize, ParserError> {
        let val = self.read_signed()?;
        Ok(val as isize)
    }

    #[inline]
    fn read_number_raw(&mut self) -> Result<(u64, u8, u32), ParserError> {
        let mut result: u64 = 0;
        let mut shift = 0;
        let byte = loop {
            let byte = self.read_u8()?;
            result |= u64::from(byte & 0x7F) << shift;
            shift += 7;
            if byte & 0x80 == 0 {
                break byte;
            }
        };

        Ok((result, byte, shift))
    }

    pub(crate) fn read_unsigned(&mut self) -> Result<u64, ParserError> {
        let (result, _byte, _shift) = self.read_number_raw()?;
        Ok(result)
    }

    pub(crate) fn read_signed(&mut self) -> Result<i64, ParserError> {
        let (result, byte, shift) = self.read_number_raw()?;
        let size = i64::BITS;
        if (shift < size) && (byte & 0x40) != 0 {
            return Ok((result | (u64::MAX << shift)) as i64);
        }
        Ok(result as i64)
    }

    #[inline]
    pub(crate) fn read_f32(&mut self) -> Result<f32, ParserError> {
        self.read_bytes().map(|b| f32::from_le_bytes(*b))
    }

    #[inline]
    pub(crate) fn read_f64(&mut self) -> Result<f64, ParserError> {
        self.read_bytes().map(|b| f64::from_le_bytes(*b))
    }

    #[inline]
    #[track_caller]
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
    Import = 0x02,
    Function = 0x03,
    Table = 0x04,
    Memory = 0x05,
    Global = 0x06,
    Export = 0x07,
    Elem = 0x09,
    Code = 0x0A,
    Data = 0x0B,
}

impl Item for SectionKind {
    #[track_caller]
    fn read(reader: &mut Reader, offset: usize) -> Result<Self, ParserError> {
        match reader.read_u8()? {
            0x00 => Ok(SectionKind::Custom),
            0x01 => Ok(SectionKind::Type),
            0x02 => Ok(SectionKind::Import),
            0x03 => Ok(SectionKind::Function),
            0x04 => Ok(SectionKind::Table),
            0x05 => Ok(SectionKind::Memory),
            0x06 => Ok(SectionKind::Global),
            0x07 => Ok(SectionKind::Export),
            0x09 => Ok(SectionKind::Elem),
            0x0A => Ok(SectionKind::Code),
            0x0B => Ok(SectionKind::Data),
            other => Err(ParserError::InvalidValue { offset, found: other }),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(u8)]
pub enum TypeKind {
    Void = 0x40,
    Func = 0x60,
    FuncRef = 0x70,
    F64 = 0x7C,
    F32 = 0x7D,
    I64 = 0x7E,
    I32 = 0x7F,
}

impl TypeKind {
    pub(crate) fn len_bytes(&self) -> usize {
        match *self {
            TypeKind::Void => todo!(),
            TypeKind::Func => todo!(),
            TypeKind::FuncRef => todo!(),
            TypeKind::F64 => 8,
            TypeKind::I64 => 8,
            TypeKind::I32 => 4,
            TypeKind::F32 => 4,
        }
    }
}

impl Item for TypeKind {
    fn read(reader: &mut Reader, offset: usize) -> Result<Self, ParserError> {
        match reader.read_u8()? {
            0x40 => Ok(TypeKind::Void),
            0x60 => Ok(TypeKind::Func),
            0x70 => Ok(TypeKind::FuncRef),
            0x7C => Ok(TypeKind::F64),
            0x7D => Ok(TypeKind::F32),
            0x7E => Ok(TypeKind::I64),
            0x7F => Ok(TypeKind::I32),
            other => Err(ParserError::InvalidValue { offset, found: other }),
        }
    }
}

#[derive(Debug, Eq, PartialEq)]
pub enum ParserError {
    EndOfStream { offset: usize },
    InvalidValue { offset: usize, found: u8 },
    UnexpectedBytes { offset: usize },
    NotEnoughBytes { offset: usize },
}

impl Display for ParserError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl core::error::Error for ParserError {

}