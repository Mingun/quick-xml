//! Serde deserialization support for a DOM tree.

use super::str2bool;
use crate::de::{Text, TEXT_KEY, VALUE_KEY};
use crate::de::key::QNameDeserializer;
use crate::de::simple_type::SimpleTypeDeserializer;
use crate::dom::{Element, Node};
use crate::events::BytesStart;
use crate::errors::serialize::DeError;
use serde::de::{Deserializer, DeserializeSeed, MapAccess, Visitor};
use serde::de::value::BorrowedStrDeserializer;
use serde::{forward_to_deserialize_any, serde_if_integer128};
use std::borrow::Cow;
use std::ops::Range;
use std::vec::IntoIter;

////////////////////////////////////////////////////////////////////////////////////////////////////

macro_rules! deserialize_num {
    ($name:ident => $visit:ident) => {
        fn $name<V>(self, visitor: V) -> Result<V::Value, Self::Error>
        where
            V: Visitor<'de>,
        {
            match self {
                Self::Element(_) => self.deserialize_map(visitor),
                Self::Text(text) => visitor.$visit(text.parse()?),
            }
        }
    };
}

impl<'de> Deserializer<'de> for Node<'de> {
    type Error = DeError;

    forward_to_deserialize_any! { char str string bytes byte_buf identifier }

    #[inline]
    fn deserialize_any<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        match self {
            Self::Element(_) => self.deserialize_map(visitor),
            Self::Text(Cow::Borrowed(text)) => visitor.visit_borrowed_str(text),
            Self::Text(Cow::Owned(text)) => visitor.visit_string(text),
        }
    }

    fn deserialize_bool<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        match self {
            Self::Element(_) => self.deserialize_map(visitor),
            Self::Text(text) => str2bool(&text, visitor),
        }
    }

    deserialize_num!(deserialize_i8 => visit_i8);
    deserialize_num!(deserialize_u8 => visit_u8);
    deserialize_num!(deserialize_i16 => visit_i16);
    deserialize_num!(deserialize_u16 => visit_u16);
    deserialize_num!(deserialize_i32 => visit_i32);
    deserialize_num!(deserialize_u32 => visit_u32);
    deserialize_num!(deserialize_i64 => visit_i64);
    deserialize_num!(deserialize_u64 => visit_u64);

    serde_if_integer128! {
        deserialize_num!(deserialize_i128 => visit_i128);
        deserialize_num!(deserialize_u128 => visit_u128);
    }

    deserialize_num!(deserialize_f32 => visit_f32);
    deserialize_num!(deserialize_f64 => visit_f64);

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
            Self::Text(text) => SimpleTypeDeserializer::from_text(text).deserialize_seq(visitor),
        }
    }

    fn deserialize_tuple<V>(self, len: usize, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        match self {
            Self::Element(e) => e.deserialize_tuple(len, visitor),
            Self::Text(text) => SimpleTypeDeserializer::from_text(text).deserialize_tuple(len, visitor),
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
            Self::Text(text) => SimpleTypeDeserializer::from_text(text).deserialize_tuple_struct(name, len, visitor),
        }
    }

    #[inline]
    fn deserialize_map<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        match self {
            Self::Element(e) => e.deserialize_map(visitor),
            Self::Text(_) => self.deserialize_str(visitor),
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
            Self::Text(_) => self.deserialize_str(visitor),
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
            Self::Text(text) => SimpleTypeDeserializer::from_text(text).deserialize_enum(name, variants, visitor),
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

#[derive(Debug, PartialEq, Eq)]
enum Value<'i> {
    Unknown,
    Attribute(Range<usize>),
    Text(Text<'i>),
    Value(Node<'i>),
    Field(Element<'i>),
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

    forward_to_deserialize_any! {
        bool i8 i16 i32 i64 i128 u8 u16 u32 u64 u128 f32 f64 char str string
        bytes byte_buf identifier
    }

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

    fn deserialize_seq<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        todo!()
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
        name: &'static str,
        variants: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        todo!()
    }

    #[inline]
    fn deserialize_ignored_any<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        visitor.visit_unit()
    }
}

#[derive(Debug)]
struct ElementMapAccess<'de> {
    attributes: BytesStart<'de>,
    children: IntoIter<Node<'de>>,
    pending_value: Value<'de>,
    /// If `true`, then the deserialized struct has a field with a special name:
    /// [`TEXT_KEY`]. That field should be deserialized from the whole content
    /// of an XML node, including tag name:
    ///
    /// ```xml
    /// <tag>value for VALUE_KEY field<tag>
    /// ```
    has_text_field: bool,
    /// If `true`, then the deserialized struct has a field with a special name:
    /// [`VALUE_KEY`]. That field should be deserialized from the whole content
    /// of an XML node, including tag name:
    ///
    /// ```xml
    /// <tag>value for VALUE_KEY field<tag>
    /// ```
    has_value_field: bool,
    /// List of field names of the struct. It is empty for maps
    fields: &'static [&'static str],
}

impl<'de> ElementMapAccess<'de> {
    fn new(element: Element<'de>, fields: &'static [&'static str]) -> Self {
        Self {
            attributes: element.start,
            children: element.children.into_iter(),
            pending_value: Value::Unknown,
            has_text_field: fields.contains(&TEXT_KEY),
            has_value_field: fields.contains(&VALUE_KEY),
            fields,
        }
    }
}

impl<'de> MapAccess<'de> for ElementMapAccess<'de> {
    type Error = DeError;

    fn next_key_seed<K: DeserializeSeed<'de>>(
        &mut self,
        seed: K,
    ) -> Result<Option<K::Value>, Self::Error> {
        debug_assert_eq!(self.pending_value, Value::Unknown);

        let decoder = todo!("decoder");
        match self.children.next() {
            Some(Node::Text(_)) if self.has_value_field && !self.has_text_field => {
                // Deserialize `key` from special attribute name which means
                // that value should be taken from the text content of the
                // XML node
                let de = BorrowedStrDeserializer::<DeError>::new(VALUE_KEY);
                seed.deserialize(de).map(Some)
            }
            Some(Node::Text(_)) => {
                // Deserialize `key` from special attribute name which means
                // that value should be taken from the text content of the
                // XML node
                let de = BorrowedStrDeserializer::<DeError>::new(TEXT_KEY);
                seed.deserialize(de).map(Some)
            }
            /*Some(Node::Element(e)) if self.has_value_field && not_in(self.fields, e, decoder)? => {
                let de = BorrowedStrDeserializer::<DeError>::new(VALUE_KEY);
                seed.deserialize(de).map(Some)
            }*/
            Some(Node::Element(e)) => {
                let de = QNameDeserializer::from_elem(e.start.raw_name(), decoder)?;
                seed.deserialize(de).map(Some)
            }
            None => Ok(None),
        }
    }

    fn next_value_seed<K: DeserializeSeed<'de>>(
        &mut self,
        seed: K,
    ) -> Result<K::Value, Self::Error> {
        match std::mem::replace(&mut self.pending_value, Value::Unknown) {
            Value::Attribute(value) => seed.deserialize(SimpleTypeDeserializer::from_part(
                &self.attributes.buf,
                value,
                true,
                todo!("decoder"),
            )),
            Value::Text(text) => seed.deserialize(SimpleTypeDeserializer::from_text_content(text)),
            Value::Value(node) => seed.deserialize(node),
            Value::Field(elem) => seed.deserialize(elem),
            Value::Unknown => panic!(),
        }
    }
}
