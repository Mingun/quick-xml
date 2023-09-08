use crate::de::key::QNameDeserializer;
use crate::de::simple_type::SimpleTypeDeserializer;
use crate::de::{TEXT_KEY, VALUE_KEY};
use crate::encoding::{Decoder, EncodingError};
use crate::errors::serialize::DeError;
use crate::events::attributes::IterState;
use crate::name::QName;
use crate::reader::dom::{Element, Node};
use serde::de::value::BorrowedStrDeserializer;
use serde::de::{DeserializeSeed, Deserializer, MapAccess, SeqAccess, Visitor};
use std::borrow::Cow;
use std::collections::VecDeque;
use std::ops::Range;

#[derive(Debug, PartialEq, Eq)]
enum Value<'i> {
    Unknown,
    Attribute(Range<usize>),
    Text(Cow<'i, str>),
    Value(Node<'i>),
    Field(Element<'i>),
}

#[derive(Debug)]
pub struct ElementMapAccess<'de> {
    attributes: Cow<'de, [u8]>,
    /// State of the iterator over attributes. Contains the next position in the
    /// inner `attributes` slice, from which next attribute should be parsed.
    iter: IterState,

    children: VecDeque<Node<'de>>,
    skipped: Vec<Node<'de>>,
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
    decoder: Decoder,
}

impl<'de> ElementMapAccess<'de> {
    pub fn new(element: Element<'de>, fields: &'static [&'static str]) -> Self {
        Self {
            decoder: element.tag.decoder(),
            iter: IterState::new(element.tag.name().as_ref().len(), false),
            attributes: element.tag.buf,
            children: element.children,
            skipped: Vec::new(),
            pending_value: Value::Unknown,
            has_text_field: fields.contains(&TEXT_KEY),
            has_value_field: fields.contains(&VALUE_KEY),
            fields,
            key_buf: String::new(),
        }
    }

    fn skip_whitespaces(&mut self) -> Option<Node<'de>> {
        loop {
            match self.children.pop_front() {
                Some(Node::Space(_)) => continue,
                e => return e,
            }
        }
    }

    fn not_in(&self, element: &Element<'de>) -> Result<bool, EncodingError> {
        let tag = element
            .tag
            .decoder()
            .decode(element.tag.local_name().into_inner())?;

        Ok(self.fields.iter().all(|&field| field != tag.as_ref()))
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

            // Attributes in mapping starts from @ prefix
            // TODO: Customization point - may customize prefix
            self.key_buf.clear();
            self.key_buf.push('@');

            let key = QName(&self.attributes[key]);
            let de = QNameDeserializer::from_attr(key, self.decoder, &mut self.key_buf)?;
            return seed.deserialize(de).map(Some);
        }

        match self.skip_whitespaces() {
            Some(Node::Text(text)) | Some(Node::Space(text))
                if self.has_value_field && !self.has_text_field =>
            {
                self.pending_value = Value::Value(Node::Text(text));
                // Deserialize `key` from special attribute name which means
                // that value should be taken from the text content of the
                // XML node
                let de = BorrowedStrDeserializer::<DeError>::new(VALUE_KEY);
                seed.deserialize(de).map(Some)
            }
            Some(Node::Text(text)) | Some(Node::Space(text)) => {
                self.pending_value = Value::Text(text);
                // Deserialize `key` from special attribute name which means
                // that value should be taken from the text content of the
                // XML node
                let de = BorrowedStrDeserializer::<DeError>::new(TEXT_KEY);
                seed.deserialize(de).map(Some)
            }
            Some(Node::Element(e)) if self.has_value_field && self.not_in(&e)? => {
                let de = BorrowedStrDeserializer::<DeError>::new(VALUE_KEY);
                let key = seed.deserialize(de).map(Some);
                self.pending_value = Value::Value(Node::Element(e));
                key
            }
            Some(Node::Element(e)) => {
                let de = QNameDeserializer::from_elem(&e.tag)?;
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
                self.decoder,
            )),
            Value::Text(text) => seed.deserialize(SimpleTypeDeserializer::from_text(text)),
            Value::Value(node) => seed.deserialize(ValueDeserializer { map: self, node }),
            Value::Field(elem) => seed.deserialize(FieldDeserializer { map: self, elem }),
            Value::Unknown => Err(DeError::KeyNotRead),
        }
    }
}

////////////////////////////////////////////////////////////////////////////////////////////////////

macro_rules! forward_to_element {
    ($name:ident) => {
        #[inline]
        fn $name<V>(self, visitor: V) -> Result<V::Value, Self::Error>
        where
            V: Visitor<'de>,
        {
            self.elem.$name(visitor)
        }
    };
}

#[derive(Debug)]
struct FieldDeserializer<'de, 'm> {
    map: &'m mut ElementMapAccess<'de>,
    elem: Element<'de>,
}

impl<'de, 'm> Deserializer<'de> for FieldDeserializer<'de, 'm> {
    type Error = DeError;

    forward_to_element!(deserialize_any);
    forward_to_element!(deserialize_bool);

    forward_to_element!(deserialize_i8);
    forward_to_element!(deserialize_i16);
    forward_to_element!(deserialize_i32);
    forward_to_element!(deserialize_i64);
    forward_to_element!(deserialize_i128);

    forward_to_element!(deserialize_u8);
    forward_to_element!(deserialize_u16);
    forward_to_element!(deserialize_u32);
    forward_to_element!(deserialize_u64);
    forward_to_element!(deserialize_u128);

    forward_to_element!(deserialize_f32);
    forward_to_element!(deserialize_f64);

    forward_to_element!(deserialize_char);
    forward_to_element!(deserialize_str);
    forward_to_element!(deserialize_string);

    forward_to_element!(deserialize_bytes);
    forward_to_element!(deserialize_byte_buf);
    forward_to_element!(deserialize_identifier);

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
        name: &'static str,
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        dbg!((name, &self));
        todo!()
    }

    fn deserialize_seq<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        let seq = visitor.visit_seq(FieldSeqAccess {
            map: self.map,
            item: Some(self.elem),
        });
        self.map.children.extend(self.map.skipped.drain(..));
        seq
    }

    #[inline]
    fn deserialize_tuple<V>(self, _len: usize, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_seq(visitor)
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
        todo!()
    }

    #[inline]
    fn deserialize_map<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        todo!()
    }

    #[inline]
    fn deserialize_struct<V>(
        self,
        name: &'static str,
        fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.elem.deserialize_struct(name, fields, visitor)
    }

    #[inline]
    fn deserialize_enum<V>(
        mut self,
        name: &'static str,
        variants: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        match self.elem.children.len() {
            0 => SimpleTypeDeserializer::from_text(Cow::Borrowed(""))
                .deserialize_enum(name, variants, visitor),
            // SAFETY: We check that size == 1
            1 => self
                .elem
                .children
                .pop_front()
                .unwrap()
                .deserialize_enum(name, variants, visitor),
            _ => self.elem.deserialize_map(visitor),
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

#[derive(Debug)]
struct FieldSeqAccess<'de, 'm> {
    map: &'m mut ElementMapAccess<'de>,
    item: Option<Element<'de>>,
}

impl<'de, 'm> SeqAccess<'de> for FieldSeqAccess<'de, 'm> {
    type Error = DeError;

    fn next_element_seed<T>(&mut self, seed: T) -> Result<Option<T::Value>, Self::Error>
    where
        T: DeserializeSeed<'de>,
    {
        if let Some(pending) = self.item.take() {
            while let Some(node) = self.map.children.pop_front() {
                match node {
                    Node::Element(e) if e.tag.name() == pending.tag.name() => {
                        self.item = Some(e);
                        break;
                    }
                    _ => self.map.skipped.push(node),
                }
            }
            return seed.deserialize(pending).map(Some);
        }
        Ok(None)
    }
}

////////////////////////////////////////////////////////////////////////////////////////////////////

macro_rules! forward_to_node {
    ($name:ident) => {
        #[inline]
        fn $name<V>(self, visitor: V) -> Result<V::Value, Self::Error>
        where
            V: Visitor<'de>,
        {
            self.node.$name(visitor)
        }
    };
}

#[derive(Debug)]
struct ValueDeserializer<'de, 'm> {
    map: &'m mut ElementMapAccess<'de>,
    node: Node<'de>,
}

impl<'de, 'm> Deserializer<'de> for ValueDeserializer<'de, 'm> {
    type Error = DeError;

    forward_to_node!(deserialize_any);
    forward_to_node!(deserialize_bool);

    forward_to_node!(deserialize_i8);
    forward_to_node!(deserialize_i16);
    forward_to_node!(deserialize_i32);
    forward_to_node!(deserialize_i64);
    forward_to_node!(deserialize_i128);

    forward_to_node!(deserialize_u8);
    forward_to_node!(deserialize_u16);
    forward_to_node!(deserialize_u32);
    forward_to_node!(deserialize_u64);
    forward_to_node!(deserialize_u128);

    forward_to_node!(deserialize_f32);
    forward_to_node!(deserialize_f64);

    forward_to_node!(deserialize_char);
    forward_to_node!(deserialize_str);
    forward_to_node!(deserialize_string);

    forward_to_node!(deserialize_bytes);
    forward_to_node!(deserialize_byte_buf);
    forward_to_node!(deserialize_identifier);

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
        name: &'static str,
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        dbg!((name, &self));
        todo!()
    }

    fn deserialize_seq<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        let seq = visitor.visit_seq(ValueSeqAccess {
            map: self.map,
            item: Some(self.node),
        });
        self.map.children.extend(self.map.skipped.drain(..));
        seq
    }

    #[inline]
    fn deserialize_tuple<V>(self, _len: usize, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_seq(visitor)
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
        dbg!((name, len, &self));
        todo!()
    }

    #[inline]
    fn deserialize_map<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        // self.node.deserialize_map(visitor)
        todo!()
    }

    #[inline]
    fn deserialize_struct<V>(
        self,
        name: &'static str,
        fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        dbg!((name, fields, &self));
        todo!()
    }

    #[inline]
    fn deserialize_enum<V>(
        self,
        name: &'static str,
        variants: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        dbg!((name, variants, &self));
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

#[derive(Debug)]
struct ValueSeqAccess<'de, 'm> {
    map: &'m mut ElementMapAccess<'de>,
    item: Option<Node<'de>>,
}

impl<'de, 'm> SeqAccess<'de> for ValueSeqAccess<'de, 'm> {
    type Error = DeError;

    fn next_element_seed<T>(&mut self, seed: T) -> Result<Option<T::Value>, Self::Error>
    where
        T: DeserializeSeed<'de>,
    {
        if let Some(pending) = self.item.take() {
            while let Some(node) = self.map.children.pop_front() {
                match node {
                    Node::Element(e) if !self.map.not_in(&e)? => {
                        self.map.skipped.push(Node::Element(e));
                    }
                    Node::Text(_) if self.map.has_text_field => {
                        self.map.skipped.push(node);
                    }
                    Node::Space(_) => {}
                    _ => {
                        self.item = Some(node);
                        break;
                    }
                }
            }
            return seed.deserialize(pending).map(Some);
        }
        Ok(None)
    }
}
