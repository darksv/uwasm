use core::fmt;

#[repr(transparent)]
pub struct ByteStr([u8]);

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
