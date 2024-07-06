use std::iter::FusedIterator;
use std::str::Chars;

use super::ValidationError::*;
use super::{is_xml11_char, ValidationError};

/// Checks if the character corresponds to the [`NameStartChar`] production of
/// the XML specification (both 1.0 and 1.1).
///
/// [`NameStartChar`]: https://www.w3.org/TR/xml11/#NT-NameStartChar
#[inline]
pub fn is_name_start_char(ch: char) -> bool {
    matches!(ch, ':'
        | 'A'..='Z'
        | '_'
        | 'a'..='z'
        | '\u{00C0}'..='\u{00D6}'
        | '\u{00D8}'..='\u{00F6}'
        | '\u{00F8}'..='\u{02FF}'
        | '\u{0370}'..='\u{037D}'
        | '\u{037F}'..='\u{1FFF}'
        | '\u{200C}'..='\u{200D}'
        | '\u{2070}'..='\u{218F}'
        | '\u{2C00}'..='\u{2FEF}'
        | '\u{3001}'..='\u{D7FF}'
        | '\u{F900}'..='\u{FDCF}'
        | '\u{FDF0}'..='\u{FFFD}'
        | '\u{10000}'..='\u{EFFFF}'
    )
}

/// Checks if the character corresponds to the [`NameChar`] production of
/// the XML specification (both 1.0 and 1.1).
///
/// [`NameStartChar`]: https://www.w3.org/TR/xml11/#NT-NameChar
#[inline]
pub fn is_name_char(ch: char) -> bool {
    is_name_start_char(ch)
        || matches!(ch, '-'
            | '.'
            | '0'..='9'
            | '\u{00B7}'
            | '\u{0300}'..='\u{036F}'
            | '\u{203F}'..='\u{2040}'
        )
}

////////////////////////////////////////////////////////////////////////////////////////////////////

#[derive(Clone, Debug)]
pub struct NameValidationIter<'i> {
    /// Iterator over characters of the `QName`
    iter: Chars<'i>,
    /// If `true`, the first character already checked
    first: bool,
}

impl<'i> From<&'i str> for NameValidationIter<'i> {
    fn from(value: &'i str) -> Self {
        Self {
            iter: value.chars(),
            first: true,
        }
    }
}

impl<'i> Iterator for NameValidationIter<'i> {
    type Item = ValidationError;

    fn next(&mut self) -> Option<Self::Item> {
        if self.first {
            self.first = false;
            match self.iter.next() {
                Some(ch) if is_name_start_char(ch) => {}
                Some(ch) if !is_xml11_char(ch) => return Some(RestrictedChar(ch)),
                // Some discouraged characters does not explicitly restricted in names in XML grammar
                Some(_) => return Some(InvalidName),
                _ => return Some(EmptyName),
            }
        }
        // Check all other chars for validity
        for ch in &mut self.iter {
            if !is_name_char(ch) {
                return Some(InvalidName);
            }
            if !is_xml11_char(ch) {
                return Some(RestrictedChar(ch));
            }
        }
        None
    }
}

impl<'i> FusedIterator for NameValidationIter<'i> {}

////////////////////////////////////////////////////////////////////////////////////////////////////

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn empty() {
        let mut it = NameValidationIter::from("");

        assert_eq!(it.next(), Some(EmptyName));
        assert_eq!(it.next(), None);
        assert_eq!(it.next(), None);
    }

    #[test]
    fn space() {
        let mut it = NameValidationIter::from(" ");

        assert_eq!(it.next(), Some(InvalidName));
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
                        let name = format!("{ch}-name");
                        let mut it = NameValidationIter::from(name.as_ref());

                        assert!(
                            matches!(it.next(), Some(InvalidName) | Some(RestrictedChar(_))),
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
                        let name = format!("n{ch}");
                        let mut it = NameValidationIter::from(name.as_ref());

                        assert!(
                            matches!(it.next(), Some(InvalidName) | Some(RestrictedChar(_))),
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

    mod valid {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn only_name() {
            let mut it = NameValidationIter::from("name");

            assert_eq!(it.next(), None);
            assert_eq!(it.next(), None);
        }

        #[test]
        fn only_prefix() {
            let mut it = NameValidationIter::from("valid:");

            assert_eq!(it.next(), None);
            assert_eq!(it.next(), None);
        }

        #[test]
        fn with_prefix() {
            let mut it = NameValidationIter::from("valid:name");

            assert_eq!(it.next(), None);
            assert_eq!(it.next(), None);
        }

        #[test]
        fn colon() {
            let mut it = NameValidationIter::from(":");

            assert_eq!(it.next(), None);
            assert_eq!(it.next(), None);
        }
    }
}
