//! Utility functions for serde integration tests

use pretty_assertions::assert_eq;
use quick_xml::de::Deserializer;
use quick_xml::reader::dom::Element;
use quick_xml::DeError;
use serde::Deserialize;
use std::fmt::Debug;

/// Deserialize an instance of type T from a string of XML text.
/// If deserialization was succeeded checks that all XML events was consumed
#[track_caller]
pub fn from_str<'de, T>(source: &'de str) -> Result<T, DeError>
where
    T: Deserialize<'de> + PartialEq + Debug,
{
    // Log XML that we try to deserialize to see it in the failed tests output
    dbg!(source);
    let mut de = Deserializer::from_str(source);
    let result = T::deserialize(&mut de);

    let dom = Element::from_str(source);

    match result {
        // If type was deserialized, the whole XML document should be consumed
        Ok(ref de_result) => {
            de.check_eof_reached();

            let dom = dom.expect("DOM parsing should succeed when type is deserialized");
            let dom_result = T::deserialize(dbg!(dom))
                .expect("DOM deserialization should succeed when type is deserialized");
            assert_eq!(&dom_result, de_result);
        }
        Err(_) => {
            if let Ok(dom) = dom {
                if let Ok(dom_result) = T::deserialize(dbg!(dom)) {
                    panic!("DOM deserialization should fail, but got: {:?}", dom_result);
                }
            }
        }
    }

    result
}
