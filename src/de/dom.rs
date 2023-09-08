//! Serde deserialization support for a DOM tree.

use crate::de::key::QNameDeserializer;
use crate::de::simple_type::SimpleTypeDeserializer;
use crate::de::TEXT_KEY;
use crate::errors::serialize::DeError;
use crate::events::BytesStart;
use crate::reader::dom::{Element, Node};
use crate::utils::CowStrVisitor;
use serde::de::{
    Deserialize, DeserializeSeed, Deserializer, EnumAccess, Error, IntoDeserializer, MapAccess,
    VariantAccess, Visitor,
};
use std::borrow::Cow;
use std::fmt;

mod map;
use map::ElementMapAccess;

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

macro_rules! deserialize_node {
    ($name:ident) => {
        fn $name<V>(self, visitor: V) -> Result<V::Value, Self::Error>
        where
            V: Visitor<'de>,
        {
            match self {
                Self::Element(e) => e.deserialize_map(visitor),
                Self::Text(text) | Self::Space(text) => {
                    SimpleTypeDeserializer::from_text(text).$name(visitor)
                }
            }
        }
    };
}

impl<'de> IntoDeserializer<'de, DeError> for Node<'de> {
    type Deserializer = Self;

    #[inline]
    fn into_deserializer(self) -> Self::Deserializer {
        self
    }
}

impl<'de> Deserializer<'de> for Node<'de> {
    type Error = DeError;

    deserialize_node!(deserialize_any);
    deserialize_node!(deserialize_bool);

    deserialize_node!(deserialize_i8);
    deserialize_node!(deserialize_i16);
    deserialize_node!(deserialize_i32);
    deserialize_node!(deserialize_i64);
    deserialize_node!(deserialize_i128);

    deserialize_node!(deserialize_u8);
    deserialize_node!(deserialize_u16);
    deserialize_node!(deserialize_u32);
    deserialize_node!(deserialize_u64);
    deserialize_node!(deserialize_u128);

    deserialize_node!(deserialize_f32);
    deserialize_node!(deserialize_f64);

    deserialize_node!(deserialize_char);
    deserialize_node!(deserialize_str);
    deserialize_node!(deserialize_string);
    deserialize_node!(deserialize_bytes);
    deserialize_node!(deserialize_byte_buf);
    deserialize_node!(deserialize_identifier);

    fn deserialize_option<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        visitor.visit_some(self)
    }

    #[inline]
    fn deserialize_unit<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        visitor.visit_unit()
    }

    #[inline]
    fn deserialize_unit_struct<V>(
        self,
        _name: &'static str,
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        visitor.visit_unit()
    }

    fn deserialize_newtype_struct<V>(
        self,
        _name: &'static str,
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        visitor.visit_newtype_struct(self)
    }

    fn deserialize_seq<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        match self {
            Self::Element(e) => e.deserialize_seq(visitor),
            Self::Text(text) | Self::Space(text) => {
                SimpleTypeDeserializer::from_text(text).deserialize_seq(visitor)
            }
        }
    }

    fn deserialize_tuple<V>(self, len: usize, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        match self {
            Self::Element(e) => e.deserialize_tuple(len, visitor),
            Self::Text(text) | Self::Space(text) => {
                SimpleTypeDeserializer::from_text(text).deserialize_tuple(len, visitor)
            }
        }
    }

    fn deserialize_tuple_struct<V>(
        self,
        name: &'static str,
        len: usize,
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        match self {
            Self::Element(e) => e.deserialize_tuple_struct(name, len, visitor),
            Self::Text(text) | Self::Space(text) => {
                SimpleTypeDeserializer::from_text(text).deserialize_tuple_struct(name, len, visitor)
            }
        }
    }

    #[inline]
    fn deserialize_map<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        match self {
            Self::Element(e) => e.deserialize_map(visitor),
            Self::Text(text) | Self::Space(text) => {
                SimpleTypeDeserializer::from_text(text).deserialize_map(visitor)
            }
        }
    }

    fn deserialize_struct<V>(
        self,
        name: &'static str,
        fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        match self {
            Self::Element(e) => e.deserialize_struct(name, fields, visitor),
            Self::Text(text) | Self::Space(text) => {
                SimpleTypeDeserializer::from_text(text).deserialize_struct(name, fields, visitor)
            }
        }
    }

    fn deserialize_enum<V>(
        self,
        name: &'static str,
        variants: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        match self {
            Self::Element(e) => e.deserialize_enum(name, variants, visitor),
            Self::Text(text) | Self::Space(text) => {
                SimpleTypeDeserializer::from_text(text).deserialize_enum(name, variants, visitor)
            }
        }
    }

    #[inline]
    fn deserialize_ignored_any<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        visitor.visit_unit()
    }
}

////////////////////////////////////////////////////////////////////////////////////////////////////

/// Allows to deserialize "xxx" in "<tag>xxx</tag>" into a primitive type
macro_rules! deserialize_element {
    ($name:ident) => {
        fn $name<V>(mut self, visitor: V) -> Result<V::Value, Self::Error>
        where
            V: Visitor<'de>,
        {
            if self.children.len() == 1 {
                // SAFETY: We check that size == 1
                self.children.pop_front().unwrap().$name(visitor)
            } else {
                self.deserialize_map(visitor)
            }
        }
    };
    (@ $name:ident) => {
        fn $name<V>(mut self, visitor: V) -> Result<V::Value, Self::Error>
        where
            V: Visitor<'de>,
        {
            match self.children.len() {
                0 => SimpleTypeDeserializer::from_text(Cow::Borrowed("")).$name(visitor),
                // SAFETY: We check that size == 1
                1 => self.children.pop_front().unwrap().$name(visitor),
                _ => self.deserialize_map(visitor),
            }
        }
    };
}

impl<'de> IntoDeserializer<'de, DeError> for Element<'de> {
    type Deserializer = Self;

    #[inline]
    fn into_deserializer(self) -> Self::Deserializer {
        self
    }
}

impl<'de> Deserializer<'de> for Element<'de> {
    type Error = DeError;

    #[inline]
    fn deserialize_any<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_map(visitor)
    }

    deserialize_element!(deserialize_bool);

    deserialize_element!(deserialize_i8);
    deserialize_element!(deserialize_i16);
    deserialize_element!(deserialize_i32);
    deserialize_element!(deserialize_i64);
    deserialize_element!(deserialize_i128);

    deserialize_element!(deserialize_u8);
    deserialize_element!(deserialize_u16);
    deserialize_element!(deserialize_u32);
    deserialize_element!(deserialize_u64);
    deserialize_element!(deserialize_u128);

    deserialize_element!(deserialize_f32);
    deserialize_element!(deserialize_f64);

    deserialize_element!(deserialize_char);
    deserialize_element!(@ deserialize_str);
    deserialize_element!(@ deserialize_string);

    deserialize_element!(deserialize_bytes);
    deserialize_element!(deserialize_byte_buf);

    // Called to deserialize xs:list from text content
    deserialize_element!(@ deserialize_seq);
    deserialize_element!(deserialize_identifier);

    fn deserialize_option<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        visitor.visit_some(self)
    }

    #[inline]
    fn deserialize_unit<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        visitor.visit_unit()
    }

    fn deserialize_unit_struct<V>(
        self,
        _name: &'static str,
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        visitor.visit_unit()
    }

    fn deserialize_newtype_struct<V>(
        self,
        _name: &'static str,
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        visitor.visit_newtype_struct(self)
    }

    #[inline]
    fn deserialize_tuple<V>(self, _len: usize, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_seq(visitor)
    }

    #[inline]
    fn deserialize_tuple_struct<V>(
        self,
        _name: &'static str,
        _len: usize,
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_seq(visitor)
    }

    #[inline]
    fn deserialize_map<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_struct("", &[], visitor)
    }

    fn deserialize_struct<V>(
        self,
        _name: &'static str,
        fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        visitor.visit_map(ElementMapAccess::new(self, fields))
    }

    fn deserialize_enum<V>(
        self,
        _name: &'static str,
        _variants: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        visitor.visit_enum(self)
    }

    #[inline]
    fn deserialize_ignored_any<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        visitor.visit_unit()
    }
}

impl<'de> EnumAccess<'de> for Element<'de> {
    type Error = DeError;
    type Variant = Self;

    fn variant_seed<V>(self, seed: V) -> Result<(V::Value, Self::Variant), DeError>
    where
        V: DeserializeSeed<'de>,
    {
        let name = seed.deserialize(QNameDeserializer::from_elem(&self.tag)?)?;
        Ok((name, self))
    }
}

impl<'de> VariantAccess<'de> for Element<'de> {
    type Error = DeError;

    #[inline]
    fn unit_variant(self) -> Result<(), Self::Error> {
        Ok(())
    }

    fn newtype_variant_seed<T>(self, seed: T) -> Result<T::Value, Self::Error>
    where
        T: DeserializeSeed<'de>,
    {
        seed.deserialize(self)
    }

    #[inline]
    fn tuple_variant<V>(self, _len: usize, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        todo!()
    }

    #[inline]
    fn struct_variant<V>(
        self,
        fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        visitor.visit_map(ElementMapAccess::new(self, fields))
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

#[cfg(test)]
mod de {
    use super::*;
    use crate::de::from_str;
    use pretty_assertions::assert_eq;
    use serde::Deserialize;

    macro_rules! check {
        ($name:ident, $ty:ty, $xml:expr, $value:expr) => {
            #[test]
            fn $name() {
                #[derive(Debug, Deserialize, PartialEq)]
                struct Root {
                    field: $ty,
                }

                assert_eq!(from_str::<Root>($xml).unwrap(), Root { field: $value });

                let el = Element::from_str($xml).unwrap();
                dbg!(&el);
                assert_eq!(
                    Root::deserialize(el.into_deserializer()).unwrap(),
                    Root { field: $value },
                );
            }
        };
    }

    check!(
        deserialize_true,
        bool,
        "<root><field> true </field></root>",
        true
    );
    check!(
        deserialize_false,
        bool,
        "<root><field> false </field></root>",
        false
    );

    check!(deserialize_i8, i8, "<root><field> -42 </field></root>", -42);
    check!(
        deserialize_i16,
        i16,
        "<root><field> -42 </field></root>",
        -42
    );
    check!(
        deserialize_i32,
        i32,
        "<root><field> -42 </field></root>",
        -42
    );
    check!(
        deserialize_i64,
        i64,
        "<root><field> -42 </field></root>",
        -42
    );
    check!(
        deserialize_i128,
        i128,
        "<root><field> -42 </field></root>",
        -42
    );

    check!(deserialize_u8, u8, "<root><field> 42 </field></root>", 42);
    check!(deserialize_u16, u16, "<root><field> 42 </field></root>", 42);
    check!(deserialize_u32, u32, "<root><field> 42 </field></root>", 42);
    check!(deserialize_u64, u64, "<root><field> 42 </field></root>", 42);
    check!(
        deserialize_u128,
        u128,
        "<root><field> 42 </field></root>",
        42
    );
}
