use std::iter::FusedIterator;
use std::str::Chars;

use super::{valid_chars, ValidationError};

#[derive(Clone, Debug)]
pub struct StartValidationIter<'i> {
    /// Iterator over characters of the Start of Empty event
    iter: Chars<'i>,
}

impl<'i> Iterator for StartValidationIter<'i> {
    type Item = ValidationError;

    fn next(&mut self) -> Option<Self::Item> {
        valid_chars(&mut self.iter)
    }
}

impl<'i> From<&'i str> for StartValidationIter<'i> {
    fn from(value: &'i str) -> Self {
        Self {
            iter: value.chars(),
        }
    }
}

impl<'i> FusedIterator for StartValidationIter<'i> {}

////////////////////////////////////////////////////////////////////////////////////////////////////
