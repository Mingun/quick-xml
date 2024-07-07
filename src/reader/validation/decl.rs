use std::iter::FusedIterator;
use std::str::Chars;

use super::{valid_chars, ValidationError};

#[derive(Clone, Debug)]
pub struct DeclValidationIter<'i> {
    /// Iterator over characters of the `Decl` event.
    iter: Chars<'i>,
}

impl<'i> Iterator for DeclValidationIter<'i> {
    type Item = ValidationError;

    fn next(&mut self) -> Option<Self::Item> {
        valid_chars(&mut self.iter)
    }
}

impl<'i> From<&'i str> for DeclValidationIter<'i> {
    fn from(value: &'i str) -> Self {
        Self {
            iter: value.chars(),
        }
    }
}

impl<'i> FusedIterator for DeclValidationIter<'i> {}

////////////////////////////////////////////////////////////////////////////////////////////////////

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reader::validation::{is_xml11_char, ValidationError::*};
    use pretty_assertions::assert_eq;

    #[test]
    fn empty() {
        let mut it = DeclValidationIter::from("");

        assert_eq!(it.next(), None);
        assert_eq!(it.next(), None);
    }

    #[test]
    fn restricted_chars() {
        for i in 0..=0x10FFFF {
            match char::from_u32(i) {
                Some(ch) if !is_xml11_char(ch) => {
                    let text = format!("{ch} start {ch} end {ch}");
                    let mut it = DeclValidationIter::from(text.as_ref());

                    assert_eq!(
                        it.next(),
                        Some(RestrictedChar(ch)),
                        "character 0x{:x} (`{ch}`)",
                        ch as u32,
                    );
                    assert_eq!(
                        it.next(),
                        Some(RestrictedChar(ch)),
                        "character 0x{:x} (`{ch}`)",
                        ch as u32,
                    );
                    assert_eq!(
                        it.next(),
                        Some(RestrictedChar(ch)),
                        "character 0x{:x} (`{ch}`)",
                        ch as u32,
                    );
                    assert_eq!(it.next(), None, "character 0x{:x} (`{ch}`)", ch as u32);
                    assert_eq!(it.next(), None, "character 0x{:x} (`{ch}`)", ch as u32);
                }
                // Do not check non-discouraged characters and codepoints thats are not characters
                _ => {}
            }
        }
    }

    #[test]
    fn valid() {
        let mut it = DeclValidationIter::from("<<&&<<just - text>>&&>>");

        assert_eq!(it.next(), None);
        assert_eq!(it.next(), None);
    }
}
