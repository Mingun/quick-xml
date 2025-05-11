use std::iter::FusedIterator;
use std::slice::Iter;

use crate::events::BytesComment;

use super::ValidationError::*;
use super::{is_xml11_char, ValidationError};

/// An iterator that search validation errors in comments. It is created by
/// [`BytesComment::validate()`] method.
#[derive(Clone, Debug)]
pub struct CommentValidationIter<'i> {
    /// Iterator over characters of the comment events. Includes the one `-`
    /// from the closing sequence of the comment to simplifying check for
    /// double-dash.
    iter: Iter<'i, u8>,
    /// `true`, if the last character being seen is a dash character (`-`).
    dash: bool,
}

impl<'i> CommentValidationIter<'i> {
    fn wrap(content: &'i str) -> Self {
        let mut iter = content.as_bytes().iter();
        Self {
            // FIXME: check the first character
            dash: matches!(iter.next(), Some(b'-')),
            iter,
        }
    }
    pub(crate) fn new(comment: &'i BytesComment) -> Self {
        Self::wrap(comment)
    }
}

impl<'i> Iterator for CommentValidationIter<'i> {
    type Item = ValidationError;

    fn next(&mut self) -> Option<Self::Item> {
        for ch in self.iter.by_ref() {
            let dash = *ch == b'-';
            if self.dash && dash {
                // Does not consider the second dash as a start of a new sequence
                self.dash = false;
                return Some(DoubleHyphenInComment);
            }
            self.dash = dash;
            if !is_xml11_char(*ch as char) {
                return Some(RestrictedChar(*ch as char));
            }
        }
        // If comment ends with a dash, we should report error
        if self.dash {
            // Error is reported, do not report it again
            self.dash = false;
            return Some(DoubleHyphenInComment);
        }
        None
    }
}

impl<'i> From<&'i str> for CommentValidationIter<'i> {
    fn from(comment_content: &'i str) -> Self {
        Self::wrap(comment_content)
    }
}

impl<'i> FusedIterator for CommentValidationIter<'i> {}

////////////////////////////////////////////////////////////////////////////////////////////////////

#[cfg(test)]
mod tests {
    use super::*;
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

            assert_eq!(it.next(), Some(DoubleHyphenInComment)); // comment ends with --->
            assert_eq!(it.next(), None);
            assert_eq!(it.next(), None);
        }

        #[test]
        fn in_middle() {
            let mut it = CommentValidationIter::from(" - -");

            assert_eq!(it.next(), Some(DoubleHyphenInComment)); // comment ends with --->
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
            assert_eq!(it.next(), Some(DoubleHyphenInComment)); // comment ends with --->
            assert_eq!(it.next(), None);
            assert_eq!(it.next(), None);
        }

        #[test]
        fn in_middle() {
            let mut it = CommentValidationIter::from(" -- -");

            assert_eq!(it.next(), Some(DoubleHyphenInComment));
            assert_eq!(it.next(), Some(DoubleHyphenInComment)); // comment ends with --->
            assert_eq!(it.next(), None);
            assert_eq!(it.next(), None);
        }

        #[test]
        fn at_end() {
            let mut it = CommentValidationIter::from(" ---");

            assert_eq!(it.next(), Some(DoubleHyphenInComment));
            assert_eq!(it.next(), Some(DoubleHyphenInComment)); // comment ends with --->
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
