use core::{fmt, ops};

#[repr(transparent)]
pub struct ByteStr([u8]);

impl ByteStr {
    pub fn from_bytes(data: &[u8]) -> &ByteStr {
        // SAFETY: ByteStr has the same layout as [u8]
        unsafe { core::mem::transmute(data) }
    }

    pub fn as_bytes(&self) -> &[u8] {
        // SAFETY: ByteStr has the same layout as [u8]
        unsafe { core::mem::transmute(self) }
    }
}

impl ops::Deref for ByteStr {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        self.as_bytes()
    }
}

impl fmt::Display for ByteStr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for c in self.0.iter().copied() {
            if c < 127 {
                write!(f, "{}", c as char)?;
            } else {
                write!(f, "\\x{:02X}", c)?;
            }
        }
        Ok(())
    }
}

impl fmt::Debug for ByteStr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, f)
    }
}
