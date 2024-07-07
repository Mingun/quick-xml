use std::iter::FusedIterator;
use std::str::Chars;

use super::ValidationError::*;
use super::{is_xml11_char, ValidationError};

#[derive(Clone, Debug)]
pub struct CommentValidationIter<'i> {
    /// Iterator over characters of the comment events. Includes the one `-`
    /// from the closing sequence of the comment to simplifying check for
    /// double-dash.
    iter: Chars<'i>,
    /// `true`, if last character being seen is a dash character (`-`).
    dash: bool,
}

impl<'i> Iterator for CommentValidationIter<'i> {
    type Item = ValidationError;

    fn next(&mut self) -> Option<Self::Item> {
        for ch in &mut self.iter {
            let dash = ch == '-';
            if self.dash && dash {
                // Does not consider the second dash as start of new sequence
                self.dash = false;
                return Some(DoubleHyphenInComment);
            }
            self.dash = dash;
            if !is_xml11_char(ch) {
                return Some(RestrictedChar(ch));
            }
        }
        None
    }
}

impl<'i> From<&'i str> for CommentValidationIter<'i> {
    fn from(value: &'i str) -> Self {
        Self {
            iter: value.chars(),
            dash: false,
        }
    }
}

impl<'i> FusedIterator for CommentValidationIter<'i> {}

////////////////////////////////////////////////////////////////////////////////////////////////////

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reader::validation::is_xml11_char;
    use pretty_assertions::assert_eq;

    #[test]
    fn empty() {
        let mut it = CommentValidationIter::from("");

        assert_eq!(it.next(), None);
        assert_eq!(it.next(), None);
    }

    #[test]
    fn restricted_chars() {
        for i in 0..=0x10FFFF {
            match char::from_u32(i) {
                Some(ch) if !is_xml11_char(ch) => {
                    let text = format!("{ch} - not an XML {ch} character");
                    let mut it = CommentValidationIter::from(text.as_ref());

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

    mod dash {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn at_start() {
            let mut it = CommentValidationIter::from("- -");

            assert_eq!(it.next(), None);
            assert_eq!(it.next(), None);
        }

        #[test]
        fn in_middle() {
            let mut it = CommentValidationIter::from(" - -");

            assert_eq!(it.next(), None);
            assert_eq!(it.next(), None);
        }

        #[test]
        fn at_end() {
            let mut it = CommentValidationIter::from(" --");

            assert_eq!(it.next(), Some(DoubleHyphenInComment));
            assert_eq!(it.next(), None);
            assert_eq!(it.next(), None);
        }
    }

    mod two_dashes {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn at_start() {
            let mut it = CommentValidationIter::from("-- -");

            assert_eq!(it.next(), Some(DoubleHyphenInComment));
            assert_eq!(it.next(), None);
            assert_eq!(it.next(), None);
        }

        #[test]
        fn in_middle() {
            let mut it = CommentValidationIter::from(" -- -");

            assert_eq!(it.next(), Some(DoubleHyphenInComment));
            assert_eq!(it.next(), None);
            assert_eq!(it.next(), None);
        }

        #[test]
        fn at_end() {
            let mut it = CommentValidationIter::from(" ---");

            assert_eq!(it.next(), Some(DoubleHyphenInComment));
            assert_eq!(it.next(), None);
            assert_eq!(it.next(), None);
        }
    }

    #[test]
    fn valid() {
        let mut it = CommentValidationIter::from("<<&&<<just - text>>&&>>");

        assert_eq!(it.next(), None);
        assert_eq!(it.next(), None);
    }
}
