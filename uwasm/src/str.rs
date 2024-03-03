use core::fmt;

#[repr(transparent)]
pub struct ByteStr([u8]);

impl fmt::Display for ByteStr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // TODO: avoid  utf-8 checking
        let str = core::str::from_utf8(&self.0).unwrap();
        f.write_str(str)
    }
}
