//! JSON for JavaScript clients. Keep 64-bit integers exact across the boundary.
//!
//! JavaScript numbers cannot represent every sequence or signed cent value. The
//! serializer carries i64/u64 as decimal strings, including inside nested DTOs,
//! while retaining numbers for bounded u32 counts and floating-point geometry.

use serde::ser::{self, Serialize};

pub fn to_json(value: &impl Serialize) -> Result<String, serde_json::Error> {
    serde_json::to_string(&Wire(value))
}

struct Wire<'a, T: ?Sized>(&'a T);

impl<T: Serialize + ?Sized> Serialize for Wire<'_, T> {
    fn serialize<S: ser::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.0.serialize(Serializer(serializer))
    }
}

struct Serializer<S>(S);
struct Compound<S>(S);

macro_rules! scalar {
    ($($name:ident($ty:ty)),* $(,)?) => {$(
        fn $name(self, value: $ty) -> Result<Self::Ok, Self::Error> {
            self.0.$name(value)
        }
    )*};
}

impl<S: ser::Serializer> ser::Serializer for Serializer<S> {
    type Ok = S::Ok;
    type Error = S::Error;
    type SerializeSeq = Compound<S::SerializeSeq>;
    type SerializeTuple = Compound<S::SerializeTuple>;
    type SerializeTupleStruct = Compound<S::SerializeTupleStruct>;
    type SerializeTupleVariant = Compound<S::SerializeTupleVariant>;
    type SerializeMap = Compound<S::SerializeMap>;
    type SerializeStruct = Compound<S::SerializeStruct>;
    type SerializeStructVariant = Compound<S::SerializeStructVariant>;

    scalar!(
        serialize_bool(bool),
        serialize_i8(i8),
        serialize_i16(i16),
        serialize_i32(i32),
        serialize_u8(u8),
        serialize_u16(u16),
        serialize_u32(u32),
        serialize_f32(f32),
        serialize_f64(f64),
        serialize_char(char),
        serialize_str(&str),
        serialize_bytes(&[u8])
    );

    fn serialize_i64(self, value: i64) -> Result<Self::Ok, Self::Error> {
        self.0.collect_str(&value)
    }
    fn serialize_u64(self, value: u64) -> Result<Self::Ok, Self::Error> {
        self.0.collect_str(&value)
    }
    fn serialize_i128(self, value: i128) -> Result<Self::Ok, Self::Error> {
        self.0.collect_str(&value)
    }
    fn serialize_u128(self, value: u128) -> Result<Self::Ok, Self::Error> {
        self.0.collect_str(&value)
    }
    fn serialize_none(self) -> Result<Self::Ok, Self::Error> {
        self.0.serialize_none()
    }
    fn serialize_some<T: Serialize + ?Sized>(self, value: &T) -> Result<Self::Ok, Self::Error> {
        self.0.serialize_some(&Wire(value))
    }
    fn serialize_unit(self) -> Result<Self::Ok, Self::Error> {
        self.0.serialize_unit()
    }
    fn serialize_unit_struct(self, name: &'static str) -> Result<Self::Ok, Self::Error> {
        self.0.serialize_unit_struct(name)
    }
    fn serialize_unit_variant(
        self,
        name: &'static str,
        index: u32,
        variant: &'static str,
    ) -> Result<Self::Ok, Self::Error> {
        self.0.serialize_unit_variant(name, index, variant)
    }
    fn serialize_newtype_struct<T: Serialize + ?Sized>(
        self,
        name: &'static str,
        value: &T,
    ) -> Result<Self::Ok, Self::Error> {
        self.0.serialize_newtype_struct(name, &Wire(value))
    }
    fn serialize_newtype_variant<T: Serialize + ?Sized>(
        self,
        name: &'static str,
        index: u32,
        variant: &'static str,
        value: &T,
    ) -> Result<Self::Ok, Self::Error> {
        self.0
            .serialize_newtype_variant(name, index, variant, &Wire(value))
    }
    fn serialize_seq(self, len: Option<usize>) -> Result<Self::SerializeSeq, Self::Error> {
        self.0.serialize_seq(len).map(Compound)
    }
    fn serialize_tuple(self, len: usize) -> Result<Self::SerializeTuple, Self::Error> {
        self.0.serialize_tuple(len).map(Compound)
    }
    fn serialize_tuple_struct(
        self,
        name: &'static str,
        len: usize,
    ) -> Result<Self::SerializeTupleStruct, Self::Error> {
        self.0.serialize_tuple_struct(name, len).map(Compound)
    }
    fn serialize_tuple_variant(
        self,
        name: &'static str,
        index: u32,
        variant: &'static str,
        len: usize,
    ) -> Result<Self::SerializeTupleVariant, Self::Error> {
        self.0
            .serialize_tuple_variant(name, index, variant, len)
            .map(Compound)
    }
    fn serialize_map(self, len: Option<usize>) -> Result<Self::SerializeMap, Self::Error> {
        self.0.serialize_map(len).map(Compound)
    }
    fn serialize_struct(
        self,
        name: &'static str,
        len: usize,
    ) -> Result<Self::SerializeStruct, Self::Error> {
        self.0.serialize_struct(name, len).map(Compound)
    }
    fn serialize_struct_variant(
        self,
        name: &'static str,
        index: u32,
        variant: &'static str,
        len: usize,
    ) -> Result<Self::SerializeStructVariant, Self::Error> {
        self.0
            .serialize_struct_variant(name, index, variant, len)
            .map(Compound)
    }
    fn is_human_readable(&self) -> bool {
        self.0.is_human_readable()
    }
}

macro_rules! sequence {
    ($trait:ident, $element:ident) => {
        impl<S: ser::$trait> ser::$trait for Compound<S> {
            type Ok = S::Ok;
            type Error = S::Error;
            fn $element<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<(), Self::Error> {
                self.0.$element(&Wire(value))
            }
            fn end(self) -> Result<Self::Ok, Self::Error> {
                self.0.end()
            }
        }
    };
}
sequence!(SerializeSeq, serialize_element);
sequence!(SerializeTuple, serialize_element);
sequence!(SerializeTupleStruct, serialize_field);
sequence!(SerializeTupleVariant, serialize_field);

impl<S: ser::SerializeMap> ser::SerializeMap for Compound<S> {
    type Ok = S::Ok;
    type Error = S::Error;
    fn serialize_key<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<(), Self::Error> {
        self.0.serialize_key(&Wire(value))
    }
    fn serialize_value<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<(), Self::Error> {
        self.0.serialize_value(&Wire(value))
    }
    fn end(self) -> Result<Self::Ok, Self::Error> {
        self.0.end()
    }
}

macro_rules! structure {
    ($trait:ident) => {
        impl<S: ser::$trait> ser::$trait for Compound<S> {
            type Ok = S::Ok;
            type Error = S::Error;
            fn serialize_field<T: Serialize + ?Sized>(
                &mut self,
                key: &'static str,
                value: &T,
            ) -> Result<(), Self::Error> {
                self.0.serialize_field(key, &Wire(value))
            }
            fn end(self) -> Result<Self::Ok, Self::Error> {
                self.0.end()
            }
        }
    };
}
structure!(SerializeStruct);
structure!(SerializeStructVariant);

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[derive(serde::Serialize)]
    struct Record {
        sequence: u64,
        cents: Option<i64>,
        count: u32,
        levels: Vec<f32>,
    }

    #[test]
    fn nested_sequences_and_signed_money_are_lossless() {
        let records = vec![Record {
            sequence: u64::MAX,
            cents: Some(i64::MIN),
            count: 7,
            levels: vec![0.25],
        }];
        let actual: serde_json::Value = serde_json::from_str(&to_json(&records).unwrap()).unwrap();
        assert_eq!(
            actual,
            json!([{
                "sequence": "18446744073709551615", "cents": "-9223372036854775808", "count": 7, "levels": [0.25]
            }])
        );
    }
}
