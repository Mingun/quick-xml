//! Serde deserialization support for a DOM tree.

use crate::de::TEXT_KEY;
use crate::events::BytesStart;
use crate::reader::dom::{Element, Node};
use crate::utils::CowStrVisitor;
use serde::de::{Deserialize, DeserializeSeed, Deserializer, Error, MapAccess, Visitor};
use std::borrow::Cow;
use std::fmt;

impl<'de> Visitor<'de> for Element<'de> {
    type Value = Self;

    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("a map")
    }

    fn visit_map<A>(mut self, mut map: A) -> Result<Self::Value, A::Error>
    where
        A: MapAccess<'de>,
    {
        while let Some(key) = map.next_key_seed(CowStrVisitor)? {
            if TEXT_KEY == key {
                let text = map.next_value_seed(CowStrVisitor)?;
                self.children.push_back(Node::Text(text));
            } else if let Some(key) = key.strip_prefix('@') {
                let attr = map.next_value_seed(CowStrVisitor)?;
                self.tag.push_attribute((key, attr));
            } else {
                let tag = BytesStart::new(key);
                let elem = map.next_value_seed(Element::from(tag))?;
                self.children.push_back(Node::Element(elem));
            }
        }
        Ok(self)
    }
}

impl<'de> DeserializeSeed<'de> for Element<'de> {
    type Value = Self;

    fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_map(self)
    }
}

impl<'de> Deserialize<'de> for Element<'de> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let tag = BytesStart::new("");
        deserializer.deserialize_map(Element::from(tag))
    }
}

////////////////////////////////////////////////////////////////////////////////////////////////////

struct NodeVisitor;
impl<'de> Visitor<'de> for NodeVisitor {
    type Value = Node<'de>;

    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("a string or a map")
    }

    fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
    where
        E: Error,
    {
        Ok(Node::Text(Cow::Owned(v.to_owned())))
    }

    #[inline]
    fn visit_borrowed_str<E>(self, v: &'de str) -> Result<Self::Value, E>
    where
        E: Error,
    {
        Ok(Node::Text(Cow::Borrowed(v)))
    }

    #[inline]
    fn visit_string<E>(self, v: String) -> Result<Self::Value, E>
    where
        E: Error,
    {
        Ok(Node::Text(Cow::Owned(v)))
    }

    fn visit_map<A>(self, map: A) -> Result<Self::Value, A::Error>
    where
        A: MapAccess<'de>,
    {
        let tag = BytesStart::new("");
        Element::from(tag).visit_map(map).map(Node::Element)
    }
}

impl<'de> Deserialize<'de> for Node<'de> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_map(NodeVisitor)
    }
}

////////////////////////////////////////////////////////////////////////////////////////////////////

#[cfg(test)]
mod element {
    use super::*;
    use crate::de::Deserializer;
    use pretty_assertions::assert_eq;

    macro_rules! same {
        ($name:ident, $xml:literal) => {
            #[test]
            fn $name() {
                let de = &mut Deserializer::from_str($xml);
                let e1 = <Element as Deserialize>::deserialize(de).unwrap();
                let e2 = Element::from_str($xml).unwrap();

                let a1: Vec<_> = e1.tag.attributes().collect();
                let a2: Vec<_> = e2.tag.attributes().collect();

                // tag names are not equal because MapAccess do not provide the struct name
                assert_eq!(e1.children, e2.children);
                assert_eq!(a1, a2);
            }
        };
    }

    same!(empty, "<root/>");
    same!(text, "<root>text</root>");
    same!(number, "<root>123</root>");
    same!(attributes, "<root attr1 = 'value1' attr2 = 'value2'/>");
    same!(
        fields,
        "<root>\
            <child1/>\
            <child2/>\
        </root>"
    );
    same!(
        list,
        "<root>\
            <child/>\
            <child/>\
        </root>"
    );
    same!(
        mixed1,
        "<root>\
            <child1/>\
            text\
            <child2/>\
        </root>"
    );
    same!(
        mixed2,
        "<root>\
            text1\
            <child/>\
            text2\
        </root>"
    );
}

#[cfg(test)]
mod node {
    use super::*;
    use crate::de::Deserializer;
    use pretty_assertions::assert_eq;

    macro_rules! same {
        ($name:ident, $xml:literal) => {
            #[test]
            fn $name() {
                let de = &mut Deserializer::from_str($xml);
                let e1 = <Node as Deserialize>::deserialize(de).unwrap();
                let e2 = Node::from_str($xml).unwrap();

                // tag names are not equal because MapAccess do not provide the struct name
                match (e1, e2) {
                    (Node::Element(e1), Node::Element(e2)) => {
                        let a1: Vec<_> = e1.tag.attributes().collect();
                        let a2: Vec<_> = e2.tag.attributes().collect();

                        assert_eq!(e1.children, e2.children);
                        assert_eq!(a1, a2);
                    }
                    (e1, e2) => assert_eq!(e1, e2),
                }
            }
        };
    }

    same!(root_text, "text");
    same!(root_number, "456");
    same!(root_attr, "@attribute");

    same!(empty, "<root/>");
    same!(text, "<root>text</root>");
    same!(number, "<root>123</root>");
    same!(attributes, "<root attr1 = 'value1' attr2 = 'value2'/>");
    same!(
        fields,
        "<root>\
            <child1/>\
            <child2/>\
        </root>"
    );
    same!(
        list,
        "<root>\
            <child/>\
            <child/>\
        </root>"
    );
    same!(
        mixed1,
        "<root>\
            <child1/>\
            text\
            <child2/>\
        </root>"
    );
    same!(
        mixed2,
        "<root>\
            text1\
            <child/>\
            text2\
        </root>"
    );
}
