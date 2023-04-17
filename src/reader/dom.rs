//! Exports an [`Element`] struct which represents a DOM tree.

use crate::errors::Error;
use crate::events::{BytesStart, Event};
use crate::reader::Reader;
use std::borrow::Cow;
use std::fmt;

/// An error raised during parsing DOM.
#[derive(Clone, Debug)]
pub enum DomError {
    /// Low-level parse error, which includes format violations, mismatched tags,
    /// and encoding issues
    Parse(Error),
    /// The XML Document contains multiple root elements. According to the XML
    /// specification, the document should contain exactly one top-level element
    MultipleRoots,
    /// The XML Document does not contain root element. According to the XML
    /// specification, the document should contain exactly one top-level element
    MissingRoot,
}

impl fmt::Display for DomError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::Parse(e) => write!(f, "parse error: {}", e),
            Self::MultipleRoots => write!(f, "multiple root elements"),
            Self::MissingRoot => write!(f, "missing root element"),
        }
    }
}

impl std::error::Error for DomError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Parse(e) => Some(e),
            _ => None,
        }
    }
}

impl From<Error> for DomError {
    fn from(err: Error) -> Self {
        Self::Parse(err)
    }
}

////////////////////////////////////////////////////////////////////////////////////////////////////

/// Temporary storage for unprocessed event
enum Unprocessed<'a> {
    Start(BytesStart<'a>),
    End,
    Empty(BytesStart<'a>),
    Text(Cow<'a, str>),
}

/// A struct representing a DOM Element.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Element<'a> {
    start: BytesStart<'a>,
    children: Vec<Node<'a>>,
}
impl<'a> Element<'a> {
    /// Creates a DOM Element from XML text, borrowing the input.
    pub fn from_str(xml: &'a str) -> Result<Self, DomError> {
        Self::read_element(&mut Reader::from_str(xml))
    }

    /// Creates a DOM Element by reading one node from a reader.
    pub(crate) fn read_element(reader: &mut Reader<&'a [u8]>) -> Result<Self, DomError> {
        let mut parents = Vec::<Element>::new();
        let mut root = None;
        let mut text = None;
        let mut level = 0;
        loop {
            let (unprocessed, create_text_node) = match reader.read_event()? {
                Event::Decl(_) | Event::PI(_) | Event::DocType(_) | Event::Comment(_) => continue,

                Event::Start(e) => {
                    level += 1;
                    (Unprocessed::Start(e), true)
                }
                Event::End(_) => {
                    level -= 1;
                    (Unprocessed::End, true)
                }
                Event::Empty(e) => (Unprocessed::Empty(e), true),
                Event::Text(e) => (Unprocessed::Text(e.unescape()?), false),
                Event::CData(e) => (Unprocessed::Text(e.decode().map_err(|e| Error::Encoding(e))?), false),

                Event::Eof => break,
            };
            if create_text_node {
                if let Some(text) = text.take() {
                    match parents.last_mut() {
                        Some(parent) => parent.children.push(Node::from(text)),
                        None => return Err(DomError::MissingRoot),
                    }
                }
            }
            // Process events after creating pending text node
            match unprocessed {
                Unprocessed::Start(e) => parents.push(Element::from(e)),
                // Matching to start event already checked in a parser
                Unprocessed::End => match parents.pop() {
                    Some(element) => match parents.last_mut() {
                        Some(parent) => parent.children.push(Node::from(element)),
                        None => match root {
                            None => root = Some(element),
                            Some(_) => return Err(DomError::MultipleRoots),
                        },
                    },
                    None => return Err(DomError::MissingRoot),
                },
                Unprocessed::Empty(e) => {
                    let element = Element::from(e);
                    match parents.last_mut() {
                        Some(parent) => parent.children.push(Node::from(element)),
                        None => match root {
                            None => root = Some(element),
                            Some(_) => return Err(DomError::MultipleRoots),
                        },
                    }
                }
                Unprocessed::Text(t) => {
                    // Append text to storage or store it to storage
                    text = match text.take() {
                        None => Some(t),
                        Some(prefix) => {
                            let mut s = prefix.into_owned();
                            s += &t;
                            Some(Cow::Owned(s))
                        }
                    }
                }
            }
            if level <= 0 {
                break;
            }
        }
        match root {
            Some(element) => Ok(element),
            None => Err(DomError::MissingRoot),
        }
    }
}

impl<'a> From<BytesStart<'a>> for Element<'a> {
    fn from(tag: BytesStart<'a>) -> Self {
        Self {
            start: tag,
            children: Vec::new(),
        }
    }
}

////////////////////////////////////////////////////////////////////////////////////////////////////

/// A node in an element tree.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Node<'a> {
    /// An `Element`.
    Element(Element<'a>),
    /// A text node, borrowing from input if possible.
    Text(Cow<'a, str>),
}

impl<'a, E> From<E> for Node<'a>
where
    E: Into<Element<'a>>,
{
    fn from(element: E) -> Self {
        Self::Element(element.into())
    }
}

impl<'a> From<Cow<'a, str>> for Node<'a> {
    fn from(text: Cow<'a, str>) -> Self {
        Self::Text(text)
    }
}

impl<'a> From<String> for Node<'a> {
    fn from(text: String) -> Self {
        Self::Text(Cow::Owned(text))
    }
}

impl<'a> From<&'a str> for Node<'a> {
    fn from(text: &'a str) -> Self {
        Self::Text(Cow::Borrowed(text))
    }
}

////////////////////////////////////////////////////////////////////////////////////////////////////

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn empty() {
        let element = Element::from_str("<empty/>").unwrap();
        assert_eq!(
            element,
            Element {
                start: BytesStart::new("empty"),
                children: vec![],
            }
        );
    }

    #[test]
    fn simple() {
        let element = Element::from_str("<tag></tag>").unwrap();
        assert_eq!(
            element,
            Element {
                start: BytesStart::new("tag"),
                children: vec![],
            }
        );
    }

    #[test]
    fn text() {
        let element = Element::from_str("<tag>text</tag>").unwrap();
        assert_eq!(
            element,
            Element {
                start: BytesStart::new("tag"),
                children: vec![Node::Text("text".into())],
            }
        );
    }

    #[test]
    fn nested() {
        let element = Element::from_str(
            "<tag><tag>HTML <i>text</i> <![CDATA[is awesome! <3]]> &lt;3</tag></tag>",
        )
        .unwrap();
        assert_eq!(
            element,
            Element {
                start: BytesStart::new("tag"),
                children: vec![Node::Element(Element {
                    start: BytesStart::new("tag"),
                    children: vec![
                        Node::Text("HTML ".into()),
                        Node::Element(Element {
                            start: BytesStart::new("i"),
                            children: vec![Node::Text("text".into()),],
                        }),
                        Node::Text(" is awesome! <3 <3".into()),
                    ],
                })],
            }
        );
    }

    #[test]
    fn multiple_roots() {
        let error = Element::from_str("<one/><two/>").unwrap_err();
        assert!(matches!(error, DomError::MultipleRoots));

        let error = Element::from_str("<one/><two></two>").unwrap_err();
        assert!(matches!(error, DomError::MultipleRoots));

        let error = Element::from_str("<one></one><two/>").unwrap_err();
        assert!(matches!(error, DomError::MultipleRoots));

        let error = Element::from_str("<one></one><two></two>").unwrap_err();
        assert!(matches!(error, DomError::MultipleRoots));
    }

    #[test]
    fn missing_root() {
        let error = Element::from_str("text").unwrap_err();
        assert!(matches!(error, DomError::MissingRoot));
    }

    #[test]
    fn missing_open_tag() {
        let error = Element::from_str("</close>").unwrap_err();
        assert!(matches!(
            error,
            DomError::Parse(Error::IllFormed(_))
        ));
    }
}
