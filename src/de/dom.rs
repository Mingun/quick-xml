//! Serde deserialization support for a DOM tree.

use crate::de::key::QNameDeserializer;
use crate::de::simple_type::SimpleTypeDeserializer;
use crate::de::{TEXT_KEY, VALUE_KEY};
use crate::encoding::Decoder;
use crate::errors::serialize::DeError;
use crate::events::attributes::IterState;
use crate::name::QName;
use crate::reader::dom::{Element, Node};
use crate::utils::CowRef;
use serde::de::value::BorrowedStrDeserializer;
use serde::de::{DeserializeSeed, Deserializer, IntoDeserializer, MapAccess, Visitor};
use serde::{forward_to_deserialize_any, serde_if_integer128};
use std::borrow::Cow;
use std::ops::Range;
use std::collections::vec_deque::IntoIter;

////////////////////////////////////////////////////////////////////////////////////////////////////

macro_rules! deserialize_num {
    ($name:ident => $visit:ident) => {
        fn $name<V>(self, visitor: V) -> Result<V::Value, Self::Error>
        where
            V: Visitor<'de>,
        {
            match self {
                Self::Element(_) => self.deserialize_map(visitor),
                Self::Text(text) => match text.parse() {
                    Ok(num) => visitor.$visit(num),
                    Err(_) => deserialize_text(text, visitor),
                },
            }
        }
    };
}

/// Allows to deserialize "xxx" in "<tag>xxx</tag>" into a primitive type
macro_rules! deserialize_primitive {
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
}

#[inline]
fn deserialize_text<'de, V>(text: Cow<'de, str>, visitor: V) -> Result<V::Value, DeError>
where
    V: Visitor<'de>,
{
    match text {
        Cow::Borrowed(text) => visitor.visit_borrowed_str(text),
        Cow::Owned(text) => visitor.visit_string(text),
    }
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

    forward_to_deserialize_any! { char str string bytes byte_buf identifier }

    #[inline]
    fn deserialize_any<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        match self {
            Self::Element(_) => self.deserialize_map(visitor),
            Self::Text(text) => deserialize_text(text, visitor),
        }
    }

    fn deserialize_bool<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        match self {
            Self::Element(_) => self.deserialize_map(visitor),
            Self::Text(Cow::Borrowed(text)) => CowRef::Input(text).deserialize_bool(visitor),
            Self::Text(Cow::Owned(text)) => CowRef::<str>::Owned(text).deserialize_bool(visitor),
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
            Self::Text(text) => {
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
            Self::Text(text) => {
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
            Self::Text(text) => {
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
        dbg!(self).deserialize_map(visitor)
    }

    deserialize_primitive!(deserialize_bool);

    deserialize_primitive!(deserialize_i8);
    deserialize_primitive!(deserialize_i16);
    deserialize_primitive!(deserialize_i32);
    deserialize_primitive!(deserialize_i64);
    deserialize_primitive!(deserialize_i128);

    deserialize_primitive!(deserialize_u8);
    deserialize_primitive!(deserialize_u16);
    deserialize_primitive!(deserialize_u32);
    deserialize_primitive!(deserialize_u64);
    deserialize_primitive!(deserialize_u128);

    deserialize_primitive!(deserialize_f32);
    deserialize_primitive!(deserialize_f64);

    deserialize_primitive!(deserialize_char);
    deserialize_primitive!(deserialize_str);
    deserialize_primitive!(deserialize_string);

    deserialize_primitive!(deserialize_bytes);
    deserialize_primitive!(deserialize_byte_buf);
    deserialize_primitive!(deserialize_identifier);

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
        dbg!(self).deserialize_struct("", &[], visitor)
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
        visitor.visit_map(ElementMapAccess::new(dbg!(self), fields))
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

////////////////////////////////////////////////////////////////////////////////////////////////////

#[derive(Debug, PartialEq, Eq)]
enum Value<'i> {
    Unknown,
    Attribute(Range<usize>),
    Text(Cow<'i, str>),
    Value(Node<'i>),
    Field(Element<'i>),
}

#[derive(Debug)]
struct ElementMapAccess<'de> {
    attributes: Cow<'de, [u8]>,
    /// State of the iterator over attributes. Contains the next position in the
    /// inner `attributes` slice, from which next attribute should be parsed.
    iter: IterState,

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
    key_buf: String,
}

impl<'de> ElementMapAccess<'de> {
    fn new(element: Element<'de>, fields: &'static [&'static str]) -> Self {
        dbg!(Self {
            iter: IterState::new(element.start.name().as_ref().len(), false),
            attributes: element.start.buf,
            children: element.children.into_iter(),
            pending_value: Value::Unknown,
            has_text_field: fields.contains(&TEXT_KEY),
            has_value_field: fields.contains(&VALUE_KEY),
            fields,
            key_buf: String::new(),
        })
    }
}

impl<'de> MapAccess<'de> for ElementMapAccess<'de> {
    type Error = DeError;

    fn next_key_seed<K: DeserializeSeed<'de>>(
        &mut self,
        seed: K,
    ) -> Result<Option<K::Value>, Self::Error> {
        debug_assert_eq!(self.pending_value, Value::Unknown);

        if let Some(a) = self.iter.next(&self.attributes) {
            let (key, value) = a?.into();
            self.pending_value = Value::Attribute(value.unwrap_or_default());

            let key = QName(&self.attributes[key]);
            // FIXME: Get decoder from Reader
            let de = QNameDeserializer::from_attr(key, Decoder::utf8(), &mut self.key_buf)?;
            return seed.deserialize(de).map(Some);
        }

        match self.children.next() {
            Some(Node::Text(text)) if self.has_value_field && !self.has_text_field => {
                self.pending_value = Value::Value(Node::Text(text));
                // Deserialize `key` from special attribute name which means
                // that value should be taken from the text content of the
                // XML node
                let de = BorrowedStrDeserializer::<DeError>::new(VALUE_KEY);
                seed.deserialize(de).map(Some)
            }
            Some(Node::Text(text)) => {
                self.pending_value = Value::Text(text);
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
                // FIXME: Get decoder from Reader
                let de = QNameDeserializer::from_elem(e.start.raw_name(), Decoder::utf8())?;
                let key = seed.deserialize(de).map(Some);
                self.pending_value = Value::Field(e);
                key
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
                &self.attributes,
                value,
                true,
                // FIXME: Get decoder from Reader
                Decoder::utf8(),
            )),
            Value::Text(text) => seed.deserialize(SimpleTypeDeserializer::from_text(text)),
            Value::Value(node) => seed.deserialize(node),
            Value::Field(elem) => seed.deserialize(elem),
            Value::Unknown => panic!("ElementMapAccess::next_key_seed was not called"),
        }
    }
}

////////////////////////////////////////////////////////////////////////////////////////////////////

#[cfg(test)]
mod tests {
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

                assert_eq!(from_str::<Root>($xml).unwrap(), Root { field: $value },);

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
        "<root><field>true</field></root>",
        true
    );
    check!(
        deserialize_false,
        bool,
        "<root><field>false</field></root>",
        false
    );
}
