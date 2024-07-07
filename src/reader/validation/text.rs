use std::iter::FusedIterator;
use std::str::Chars;

use super::ValidationError::*;
use super::{is_xml11_char, valid_chars, ValidationError};

#[derive(Clone, Debug)]
pub struct TextValidationIter<'i> {
    /// Iterator over characters of the Text
    iter: Chars<'i>,
}

impl<'i> From<&'i str> for TextValidationIter<'i> {
    fn from(value: &'i str) -> Self {
        Self {
            iter: value.chars(),
        }
    }
}

impl<'i> Iterator for TextValidationIter<'i> {
    type Item = ValidationError;

    fn next(&mut self) -> Option<Self::Item> {
        // Ampersand can be only the first character of the `Text` event, because
        // if reference is correct it is reported as `GeneralRef` event.
        match self.iter.next() {
            Some('&') => return Some(UnclosedReference),
            Some(ch) if !is_xml11_char(ch) => return Some(RestrictedChar(ch)),
            _ => {}
        }
        // Check all other chars for validity
        valid_chars(&mut self.iter)
    }
}

impl<'i> FusedIterator for TextValidationIter<'i> {}

////////////////////////////////////////////////////////////////////////////////////////////////////

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn empty() {
        let mut it = TextValidationIter::from("");

        assert_eq!(it.next(), None);
        assert_eq!(it.next(), None);
    }

    /// Ampersand can be only the first character of the `Text` event, because
    /// if reference is correct it is reported as `GeneralRef` event.
    #[test]
    fn amp() {
        let mut it = TextValidationIter::from("&some text");

        assert_eq!(it.next(), Some(UnclosedReference));
        assert_eq!(it.next(), None);
        assert_eq!(it.next(), None);
    }

    mod restricted_chars {
        use super::*;
        use crate::reader::validation::is_xml11_discouraged_char;
        use pretty_assertions::assert_eq;

        #[test]
        fn first() {
            for i in 0..=0x10FFFF {
                match char::from_u32(i) {
                    Some(ch) if !is_xml11_char(ch) => {
                        let text = format!("{ch} - not an XML character");
                        let mut it = TextValidationIter::from(text.as_ref());

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
        fn not_first() {
            for i in 0..=0x10FFFF {
                match char::from_u32(i) {
                    Some(ch) if !is_xml11_char(ch) => {
                        let text = format!("text with {ch} character");
                        let mut it = TextValidationIter::from(text.as_ref());

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
    }

    #[test]
    fn valid() {
        let mut it = TextValidationIter::from("just text");

        assert_eq!(it.next(), None);
        assert_eq!(it.next(), None);
    }
}
