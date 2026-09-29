use crate::model::{DecodedValue, OPAQUE_VALUE_MARKER, OpaqueByteRange, OpaqueReason, OpaqueValue};
use serde::Serialize;
use serde::ser::{
    self, Impossible, SerializeMap, SerializeSeq, SerializeStruct, SerializeStructVariant,
    SerializeTuple, SerializeTupleStruct, SerializeTupleVariant,
};
use std::collections::BTreeMap;
use std::fmt::{self, Display};
use std::ops::Index;

pub(crate) type Value = DecodedValue;
pub(crate) type Map = BTreeMap<String, Value>;

impl DecodedValue {
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Self::String(value) => Some(value),
            _ => None,
        }
    }

    pub fn as_i64(&self) -> Option<i64> {
        match self {
            Self::Integer(value) => Some(*value),
            Self::Unsigned(value) => i64::try_from(*value).ok(),
            _ => None,
        }
    }

    pub fn as_u64(&self) -> Option<u64> {
        match self {
            Self::Unsigned(value) => Some(*value),
            Self::Integer(value) => u64::try_from(*value).ok(),
            _ => None,
        }
    }

    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Self::Float(value) => Some(*value),
            Self::Integer(value) => Some(*value as f64),
            Self::Unsigned(value) => Some(*value as f64),
            _ => None,
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Self::Bool(value) => Some(*value),
            _ => None,
        }
    }

    pub fn as_array(&self) -> Option<&[DecodedValue]> {
        match self {
            Self::Array(value) => Some(value),
            _ => None,
        }
    }

    pub fn as_object(&self) -> Option<&BTreeMap<String, DecodedValue>> {
        match self {
            Self::Object(value) => Some(value),
            _ => None,
        }
    }

    pub fn as_opaque(&self) -> Option<&OpaqueValue> {
        match self {
            Self::Opaque(value) => Some(value.as_ref()),
            _ => None,
        }
    }

    pub fn is_opaque(&self) -> bool {
        matches!(self, Self::Opaque(_))
    }

    pub fn get(&self, key: &str) -> Option<&DecodedValue> {
        self.as_object()?.get(key)
    }

    pub fn is_number(&self) -> bool {
        matches!(self, Self::Integer(_) | Self::Unsigned(_) | Self::Float(_))
    }

    pub fn is_string(&self) -> bool {
        matches!(self, Self::String(_))
    }

    pub fn is_null(&self) -> bool {
        matches!(self, Self::Null)
    }
}

impl PartialEq<&str> for DecodedValue {
    fn eq(&self, other: &&str) -> bool {
        self.as_str() == Some(*other)
    }
}

impl PartialEq<String> for DecodedValue {
    fn eq(&self, other: &String) -> bool {
        self.as_str() == Some(other.as_str())
    }
}

macro_rules! impl_integer_partial_eq {
    ($($integer:ty),* $(,)?) => {
        $(
            impl PartialEq<$integer> for DecodedValue {
                fn eq(&self, other: &$integer) -> bool {
                    self.as_i64() == i64::try_from(*other).ok()
                }
            }
        )*
    };
}

impl_integer_partial_eq!(i8, i16, i32, i64, u8, u16, u32, u64, usize);

impl PartialEq<f32> for DecodedValue {
    fn eq(&self, other: &f32) -> bool {
        self.as_f64() == Some(f64::from(*other))
    }
}

impl PartialEq<f64> for DecodedValue {
    fn eq(&self, other: &f64) -> bool {
        self.as_f64() == Some(*other)
    }
}

impl PartialEq<bool> for DecodedValue {
    fn eq(&self, other: &bool) -> bool {
        self.as_bool() == Some(*other)
    }
}

static NULL_VALUE: Value = Value::Null;

impl Index<&str> for DecodedValue {
    type Output = DecodedValue;

    fn index(&self, key: &str) -> &Self::Output {
        self.get(key).unwrap_or(&NULL_VALUE)
    }
}

impl Index<usize> for DecodedValue {
    type Output = DecodedValue;

    fn index(&self, index: usize) -> &Self::Output {
        self.as_array()
            .and_then(|values| values.get(index))
            .unwrap_or(&NULL_VALUE)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ValueError(String);

impl Display for ValueError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for ValueError {}

impl ser::Error for ValueError {
    fn custom<T: Display>(message: T) -> Self {
        Self(message.to_string())
    }
}

pub(crate) fn to_value<T>(value: &T) -> Result<Value, ValueError>
where
    T: Serialize + ?Sized,
{
    value.serialize(ValueSerializer)
}

struct ValueSerializer;

impl ser::Serializer for ValueSerializer {
    type Ok = Value;
    type Error = ValueError;
    type SerializeSeq = SequenceSerializer;
    type SerializeTuple = SequenceSerializer;
    type SerializeTupleStruct = SequenceSerializer;
    type SerializeTupleVariant = TupleVariantSerializer;
    type SerializeMap = MapSerializer;
    type SerializeStruct = MapSerializer;
    type SerializeStructVariant = StructVariantSerializer;

    fn serialize_bool(self, value: bool) -> Result<Self::Ok, Self::Error> {
        Ok(Value::Bool(value))
    }

    fn serialize_i8(self, value: i8) -> Result<Self::Ok, Self::Error> {
        self.serialize_i64(i64::from(value))
    }

    fn serialize_i16(self, value: i16) -> Result<Self::Ok, Self::Error> {
        self.serialize_i64(i64::from(value))
    }

    fn serialize_i32(self, value: i32) -> Result<Self::Ok, Self::Error> {
        self.serialize_i64(i64::from(value))
    }

    fn serialize_i64(self, value: i64) -> Result<Self::Ok, Self::Error> {
        Ok(Value::Integer(value))
    }

    fn serialize_i128(self, value: i128) -> Result<Self::Ok, Self::Error> {
        i64::try_from(value)
            .map(Value::Integer)
            .map_err(|_| ValueError("i128 is outside the decoded integer range".into()))
    }

    fn serialize_u8(self, value: u8) -> Result<Self::Ok, Self::Error> {
        self.serialize_u64(u64::from(value))
    }

    fn serialize_u16(self, value: u16) -> Result<Self::Ok, Self::Error> {
        self.serialize_u64(u64::from(value))
    }

    fn serialize_u32(self, value: u32) -> Result<Self::Ok, Self::Error> {
        self.serialize_u64(u64::from(value))
    }

    fn serialize_u64(self, value: u64) -> Result<Self::Ok, Self::Error> {
        Ok(Value::Unsigned(value))
    }

    fn serialize_u128(self, value: u128) -> Result<Self::Ok, Self::Error> {
        u64::try_from(value)
            .map(Value::Unsigned)
            .map_err(|_| ValueError("u128 is outside the decoded unsigned range".into()))
    }

    fn serialize_f32(self, value: f32) -> Result<Self::Ok, Self::Error> {
        self.serialize_f64(f64::from(value))
    }

    fn serialize_f64(self, value: f64) -> Result<Self::Ok, Self::Error> {
        Ok(Value::Float(value))
    }

    fn serialize_char(self, value: char) -> Result<Self::Ok, Self::Error> {
        Ok(Value::String(value.to_string()))
    }

    fn serialize_str(self, value: &str) -> Result<Self::Ok, Self::Error> {
        Ok(Value::String(value.to_owned()))
    }

    fn serialize_bytes(self, value: &[u8]) -> Result<Self::Ok, Self::Error> {
        Ok(Value::Array(
            value
                .iter()
                .map(|byte| Value::Unsigned(u64::from(*byte)))
                .collect(),
        ))
    }

    fn serialize_none(self) -> Result<Self::Ok, Self::Error> {
        self.serialize_unit()
    }

    fn serialize_some<T>(self, value: &T) -> Result<Self::Ok, Self::Error>
    where
        T: Serialize + ?Sized,
    {
        value.serialize(self)
    }

    fn serialize_unit(self) -> Result<Self::Ok, Self::Error> {
        Ok(Value::Null)
    }

    fn serialize_unit_struct(self, _name: &'static str) -> Result<Self::Ok, Self::Error> {
        self.serialize_unit()
    }

    fn serialize_unit_variant(
        self,
        _name: &'static str,
        _variant_index: u32,
        variant: &'static str,
    ) -> Result<Self::Ok, Self::Error> {
        self.serialize_str(variant)
    }

    fn serialize_newtype_struct<T>(
        self,
        name: &'static str,
        value: &T,
    ) -> Result<Self::Ok, Self::Error>
    where
        T: Serialize + ?Sized,
    {
        let inner = value.serialize(self)?;
        if name == OPAQUE_VALUE_MARKER {
            return opaque_from_value(&inner).map(Value::opaque);
        }
        Ok(inner)
    }

    fn serialize_newtype_variant<T>(
        self,
        _name: &'static str,
        _variant_index: u32,
        variant: &'static str,
        value: &T,
    ) -> Result<Self::Ok, Self::Error>
    where
        T: Serialize + ?Sized,
    {
        let mut object = Map::new();
        object.insert(variant.to_owned(), to_value(value)?);
        Ok(Value::Object(object))
    }

    fn serialize_seq(self, len: Option<usize>) -> Result<Self::SerializeSeq, Self::Error> {
        Ok(SequenceSerializer::new(len))
    }

    fn serialize_tuple(self, len: usize) -> Result<Self::SerializeTuple, Self::Error> {
        Ok(SequenceSerializer::new(Some(len)))
    }

    fn serialize_tuple_struct(
        self,
        _name: &'static str,
        len: usize,
    ) -> Result<Self::SerializeTupleStruct, Self::Error> {
        Ok(SequenceSerializer::new(Some(len)))
    }

    fn serialize_tuple_variant(
        self,
        _name: &'static str,
        _variant_index: u32,
        variant: &'static str,
        len: usize,
    ) -> Result<Self::SerializeTupleVariant, Self::Error> {
        Ok(TupleVariantSerializer {
            variant,
            values: Vec::with_capacity(len),
        })
    }

    fn serialize_map(self, len: Option<usize>) -> Result<Self::SerializeMap, Self::Error> {
        Ok(MapSerializer::new(len))
    }

    fn serialize_struct(
        self,
        _name: &'static str,
        len: usize,
    ) -> Result<Self::SerializeStruct, Self::Error> {
        Ok(MapSerializer::new(Some(len)))
    }

    fn serialize_struct_variant(
        self,
        _name: &'static str,
        _variant_index: u32,
        variant: &'static str,
        _len: usize,
    ) -> Result<Self::SerializeStructVariant, Self::Error> {
        Ok(StructVariantSerializer {
            variant,
            values: Map::new(),
        })
    }

    fn collect_str<T>(self, value: &T) -> Result<Self::Ok, Self::Error>
    where
        T: Display + ?Sized,
    {
        self.serialize_str(&value.to_string())
    }
}

/// Rebuilds an [`OpaqueValue`] from the object its wire shape serialised to.
fn opaque_from_value(value: &Value) -> Result<OpaqueValue, ValueError> {
    let malformed = || ValueError("malformed @opaque value".into());
    let reason = value
        .get("@opaque")
        .and_then(Value::as_str)
        .and_then(OpaqueReason::from_code)
        .ok_or_else(malformed)?;
    let message = value
        .get("reason")
        .and_then(Value::as_str)
        .ok_or_else(malformed)?
        .to_owned();
    let range = value.get("byte_range").ok_or_else(malformed)?;
    let number = |key: &str| range.get(key).and_then(Value::as_u64).ok_or_else(malformed);
    Ok(OpaqueValue {
        reason,
        message,
        type_name: value.get("type").and_then(Value::as_str).map(str::to_owned),
        byte_range: OpaqueByteRange {
            start: number("start")?,
            end: number("end")?,
            size: number("size")?,
            preview: range
                .get("preview")
                .and_then(Value::as_str)
                .ok_or_else(malformed)?
                .to_owned(),
        },
    })
}

pub(crate) struct SequenceSerializer {
    values: Vec<Value>,
}

impl SequenceSerializer {
    fn new(len: Option<usize>) -> Self {
        Self {
            values: Vec::with_capacity(len.unwrap_or_default()),
        }
    }
}

impl SerializeSeq for SequenceSerializer {
    type Ok = Value;
    type Error = ValueError;

    fn serialize_element<T>(&mut self, value: &T) -> Result<(), Self::Error>
    where
        T: Serialize + ?Sized,
    {
        self.values.push(to_value(value)?);
        Ok(())
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        Ok(Value::Array(self.values))
    }
}

impl SerializeTuple for SequenceSerializer {
    type Ok = Value;
    type Error = ValueError;

    fn serialize_element<T>(&mut self, value: &T) -> Result<(), Self::Error>
    where
        T: Serialize + ?Sized,
    {
        SerializeSeq::serialize_element(self, value)
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        SerializeSeq::end(self)
    }
}

impl SerializeTupleStruct for SequenceSerializer {
    type Ok = Value;
    type Error = ValueError;

    fn serialize_field<T>(&mut self, value: &T) -> Result<(), Self::Error>
    where
        T: Serialize + ?Sized,
    {
        SerializeSeq::serialize_element(self, value)
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        SerializeSeq::end(self)
    }
}

pub(crate) struct TupleVariantSerializer {
    variant: &'static str,
    values: Vec<Value>,
}

impl SerializeTupleVariant for TupleVariantSerializer {
    type Ok = Value;
    type Error = ValueError;

    fn serialize_field<T>(&mut self, value: &T) -> Result<(), Self::Error>
    where
        T: Serialize + ?Sized,
    {
        self.values.push(to_value(value)?);
        Ok(())
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        let mut object = Map::new();
        object.insert(self.variant.to_owned(), Value::Array(self.values));
        Ok(Value::Object(object))
    }
}

pub(crate) struct MapSerializer {
    values: Map,
    next_key: Option<String>,
}

impl MapSerializer {
    fn new(_len: Option<usize>) -> Self {
        Self {
            values: Map::new(),
            next_key: None,
        }
    }
}

impl SerializeMap for MapSerializer {
    type Ok = Value;
    type Error = ValueError;

    fn serialize_key<T>(&mut self, key: &T) -> Result<(), Self::Error>
    where
        T: Serialize + ?Sized,
    {
        self.next_key = Some(key.serialize(MapKeySerializer)?);
        Ok(())
    }

    fn serialize_value<T>(&mut self, value: &T) -> Result<(), Self::Error>
    where
        T: Serialize + ?Sized,
    {
        let key = self
            .next_key
            .take()
            .ok_or_else(|| ValueError("map value serialized before its key".into()))?;
        self.values.insert(key, to_value(value)?);
        Ok(())
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        if self.next_key.is_some() {
            return Err(ValueError("map ended before serializing a value".into()));
        }
        Ok(Value::Object(self.values))
    }
}

impl SerializeStruct for MapSerializer {
    type Ok = Value;
    type Error = ValueError;

    fn serialize_field<T>(&mut self, key: &'static str, value: &T) -> Result<(), Self::Error>
    where
        T: Serialize + ?Sized,
    {
        self.values.insert(key.to_owned(), to_value(value)?);
        Ok(())
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        Ok(Value::Object(self.values))
    }
}

pub(crate) struct StructVariantSerializer {
    variant: &'static str,
    values: Map,
}

impl SerializeStructVariant for StructVariantSerializer {
    type Ok = Value;
    type Error = ValueError;

    fn serialize_field<T>(&mut self, key: &'static str, value: &T) -> Result<(), Self::Error>
    where
        T: Serialize + ?Sized,
    {
        self.values.insert(key.to_owned(), to_value(value)?);
        Ok(())
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        let mut object = Map::new();
        object.insert(self.variant.to_owned(), Value::Object(self.values));
        Ok(Value::Object(object))
    }
}

struct MapKeySerializer;

impl ser::Serializer for MapKeySerializer {
    type Ok = String;
    type Error = ValueError;
    type SerializeSeq = Impossible<String, ValueError>;
    type SerializeTuple = Impossible<String, ValueError>;
    type SerializeTupleStruct = Impossible<String, ValueError>;
    type SerializeTupleVariant = Impossible<String, ValueError>;
    type SerializeMap = Impossible<String, ValueError>;
    type SerializeStruct = Impossible<String, ValueError>;
    type SerializeStructVariant = Impossible<String, ValueError>;

    fn serialize_str(self, value: &str) -> Result<Self::Ok, Self::Error> {
        Ok(value.to_owned())
    }

    fn serialize_char(self, value: char) -> Result<Self::Ok, Self::Error> {
        Ok(value.to_string())
    }

    fn serialize_bool(self, value: bool) -> Result<Self::Ok, Self::Error> {
        Ok(value.to_string())
    }

    fn serialize_i8(self, value: i8) -> Result<Self::Ok, Self::Error> {
        Ok(value.to_string())
    }

    fn serialize_i16(self, value: i16) -> Result<Self::Ok, Self::Error> {
        Ok(value.to_string())
    }

    fn serialize_i32(self, value: i32) -> Result<Self::Ok, Self::Error> {
        Ok(value.to_string())
    }

    fn serialize_i64(self, value: i64) -> Result<Self::Ok, Self::Error> {
        Ok(value.to_string())
    }

    fn serialize_i128(self, value: i128) -> Result<Self::Ok, Self::Error> {
        Ok(value.to_string())
    }

    fn serialize_u8(self, value: u8) -> Result<Self::Ok, Self::Error> {
        Ok(value.to_string())
    }

    fn serialize_u16(self, value: u16) -> Result<Self::Ok, Self::Error> {
        Ok(value.to_string())
    }

    fn serialize_u32(self, value: u32) -> Result<Self::Ok, Self::Error> {
        Ok(value.to_string())
    }

    fn serialize_u64(self, value: u64) -> Result<Self::Ok, Self::Error> {
        Ok(value.to_string())
    }

    fn serialize_u128(self, value: u128) -> Result<Self::Ok, Self::Error> {
        Ok(value.to_string())
    }

    fn serialize_f32(self, value: f32) -> Result<Self::Ok, Self::Error> {
        Ok(value.to_string())
    }

    fn serialize_f64(self, value: f64) -> Result<Self::Ok, Self::Error> {
        Ok(value.to_string())
    }

    fn serialize_unit_variant(
        self,
        _name: &'static str,
        _variant_index: u32,
        variant: &'static str,
    ) -> Result<Self::Ok, Self::Error> {
        Ok(variant.to_owned())
    }

    fn serialize_newtype_struct<T>(
        self,
        _name: &'static str,
        value: &T,
    ) -> Result<Self::Ok, Self::Error>
    where
        T: Serialize + ?Sized,
    {
        value.serialize(self)
    }

    fn collect_str<T>(self, value: &T) -> Result<Self::Ok, Self::Error>
    where
        T: Display + ?Sized,
    {
        Ok(value.to_string())
    }

    fn serialize_bytes(self, _value: &[u8]) -> Result<Self::Ok, Self::Error> {
        Err(ValueError("map keys must be scalar values".into()))
    }

    fn serialize_none(self) -> Result<Self::Ok, Self::Error> {
        Err(ValueError("map keys cannot be null".into()))
    }

    fn serialize_some<T>(self, value: &T) -> Result<Self::Ok, Self::Error>
    where
        T: Serialize + ?Sized,
    {
        value.serialize(self)
    }

    fn serialize_unit(self) -> Result<Self::Ok, Self::Error> {
        Err(ValueError("map keys cannot be null".into()))
    }

    fn serialize_unit_struct(self, _name: &'static str) -> Result<Self::Ok, Self::Error> {
        self.serialize_unit()
    }

    fn serialize_newtype_variant<T>(
        self,
        _name: &'static str,
        _variant_index: u32,
        _variant: &'static str,
        _value: &T,
    ) -> Result<Self::Ok, Self::Error>
    where
        T: Serialize + ?Sized,
    {
        Err(ValueError("map keys must be scalar values".into()))
    }

    fn serialize_seq(self, _len: Option<usize>) -> Result<Self::SerializeSeq, Self::Error> {
        Err(ValueError("map keys must be scalar values".into()))
    }

    fn serialize_tuple(self, _len: usize) -> Result<Self::SerializeTuple, Self::Error> {
        Err(ValueError("map keys must be scalar values".into()))
    }

    fn serialize_tuple_struct(
        self,
        _name: &'static str,
        _len: usize,
    ) -> Result<Self::SerializeTupleStruct, Self::Error> {
        Err(ValueError("map keys must be scalar values".into()))
    }

    fn serialize_tuple_variant(
        self,
        _name: &'static str,
        _variant_index: u32,
        _variant: &'static str,
        _len: usize,
    ) -> Result<Self::SerializeTupleVariant, Self::Error> {
        Err(ValueError("map keys must be scalar values".into()))
    }

    fn serialize_map(self, _len: Option<usize>) -> Result<Self::SerializeMap, Self::Error> {
        Err(ValueError("map keys must be scalar values".into()))
    }

    fn serialize_struct(
        self,
        _name: &'static str,
        _len: usize,
    ) -> Result<Self::SerializeStruct, Self::Error> {
        Err(ValueError("map keys must be scalar values".into()))
    }

    fn serialize_struct_variant(
        self,
        _name: &'static str,
        _variant_index: u32,
        _variant: &'static str,
        _len: usize,
    ) -> Result<Self::SerializeStructVariant, Self::Error> {
        Err(ValueError("map keys must be scalar values".into()))
    }
}

macro_rules! decoded_value_internal {
    (@array [$($elems:expr,)*]) => {
        vec![$($elems,)*]
    };
    (@array [$($elems:expr),*]) => {
        vec![$($elems),*]
    };
    (@array [$($elems:expr,)*] null $($rest:tt)*) => {
        $crate::structured_value::decoded_value_internal!(@array [$($elems,)* $crate::structured_value::decoded_value_internal!(null)] $($rest)*)
    };
    (@array [$($elems:expr,)*] true $($rest:tt)*) => {
        $crate::structured_value::decoded_value_internal!(@array [$($elems,)* $crate::structured_value::decoded_value_internal!(true)] $($rest)*)
    };
    (@array [$($elems:expr,)*] false $($rest:tt)*) => {
        $crate::structured_value::decoded_value_internal!(@array [$($elems,)* $crate::structured_value::decoded_value_internal!(false)] $($rest)*)
    };
    (@array [$($elems:expr,)*] [$($array:tt)*] $($rest:tt)*) => {
        $crate::structured_value::decoded_value_internal!(@array [$($elems,)* $crate::structured_value::decoded_value_internal!([$($array)*])] $($rest)*)
    };
    (@array [$($elems:expr,)*] {$($map:tt)*} $($rest:tt)*) => {
        $crate::structured_value::decoded_value_internal!(@array [$($elems,)* $crate::structured_value::decoded_value_internal!({$($map)*})] $($rest)*)
    };
    (@array [$($elems:expr,)*] $next:expr, $($rest:tt)*) => {
        $crate::structured_value::decoded_value_internal!(@array [$($elems,)* $crate::structured_value::decoded_value_internal!($next),] $($rest)*)
    };
    (@array [$($elems:expr,)*] $last:expr) => {
        $crate::structured_value::decoded_value_internal!(@array [$($elems,)* $crate::structured_value::decoded_value_internal!($last)])
    };
    (@array [$($elems:expr),*] , $($rest:tt)*) => {
        $crate::structured_value::decoded_value_internal!(@array [$($elems,)*] $($rest)*)
    };
    (@array [$($elems:expr),*] $unexpected:tt $($rest:tt)*) => {
        $crate::structured_value::decoded_value_unexpected!($unexpected)
    };

    (@object $object:ident () () ()) => {};
    (@object $object:ident [$($key:tt)+] ($value:expr) , $($rest:tt)*) => {
        let _ = $object.insert(($($key)+).into(), $value);
        $crate::structured_value::decoded_value_internal!(@object $object () ($($rest)*) ($($rest)*));
    };
    (@object $object:ident [$($key:tt)+] ($value:expr) $unexpected:tt $($rest:tt)*) => {
        $crate::structured_value::decoded_value_unexpected!($unexpected);
    };
    (@object $object:ident [$($key:tt)+] ($value:expr)) => {
        let _ = $object.insert(($($key)+).into(), $value);
    };
    (@object $object:ident ($($key:tt)+) (: null $($rest:tt)*) $copy:tt) => {
        $crate::structured_value::decoded_value_internal!(@object $object [$($key)+] ($crate::structured_value::decoded_value_internal!(null)) $($rest)*);
    };
    (@object $object:ident ($($key:tt)+) (: true $($rest:tt)*) $copy:tt) => {
        $crate::structured_value::decoded_value_internal!(@object $object [$($key)+] ($crate::structured_value::decoded_value_internal!(true)) $($rest)*);
    };
    (@object $object:ident ($($key:tt)+) (: false $($rest:tt)*) $copy:tt) => {
        $crate::structured_value::decoded_value_internal!(@object $object [$($key)+] ($crate::structured_value::decoded_value_internal!(false)) $($rest)*);
    };
    (@object $object:ident ($($key:tt)+) (: [$($array:tt)*] $($rest:tt)*) $copy:tt) => {
        $crate::structured_value::decoded_value_internal!(@object $object [$($key)+] ($crate::structured_value::decoded_value_internal!([$($array)*])) $($rest)*);
    };
    (@object $object:ident ($($key:tt)+) (: {$($map:tt)*} $($rest:tt)*) $copy:tt) => {
        $crate::structured_value::decoded_value_internal!(@object $object [$($key)+] ($crate::structured_value::decoded_value_internal!({$($map)*})) $($rest)*);
    };
    (@object $object:ident ($($key:tt)+) (: $value:expr , $($rest:tt)*) $copy:tt) => {
        $crate::structured_value::decoded_value_internal!(@object $object [$($key)+] ($crate::structured_value::decoded_value_internal!($value)) , $($rest)*);
    };
    (@object $object:ident ($($key:tt)+) (: $value:expr) $copy:tt) => {
        $crate::structured_value::decoded_value_internal!(@object $object [$($key)+] ($crate::structured_value::decoded_value_internal!($value)));
    };
    (@object $object:ident ($($key:tt)+) (:) $copy:tt) => {
        $crate::structured_value::decoded_value_internal!();
    };
    (@object $object:ident ($($key:tt)+) () $copy:tt) => {
        $crate::structured_value::decoded_value_internal!();
    };
    (@object $object:ident () (: $($rest:tt)*) ($colon:tt $($copy:tt)*)) => {
        $crate::structured_value::decoded_value_unexpected!($colon);
    };
    (@object $object:ident ($($key:tt)*) (, $($rest:tt)*) ($comma:tt $($copy:tt)*)) => {
        $crate::structured_value::decoded_value_unexpected!($comma);
    };
    (@object $object:ident () (($key:expr) : $($rest:tt)*) $copy:tt) => {
        $crate::structured_value::decoded_value_internal!(@object $object ($key) (: $($rest)*) (: $($rest)*));
    };
    (@object $object:ident ($($key:tt)*) (: $($unexpected:tt)+) $copy:tt) => {
        $crate::structured_value::decoded_value_expect_expr_comma!($($unexpected)+);
    };
    (@object $object:ident ($($key:tt)*) ($tt:tt $($rest:tt)*) $copy:tt) => {
        $crate::structured_value::decoded_value_internal!(@object $object ($($key)* $tt) ($($rest)*) ($($rest)*));
    };

    (null) => { $crate::model::DecodedValue::Null };
    (true) => { $crate::model::DecodedValue::Bool(true) };
    (false) => { $crate::model::DecodedValue::Bool(false) };
    ([]) => { $crate::model::DecodedValue::Array(vec![]) };
    ([ $($tt:tt)+ ]) => {
        $crate::model::DecodedValue::Array($crate::structured_value::decoded_value_internal!(@array [] $($tt)+))
    };
    ({}) => { $crate::model::DecodedValue::Object($crate::structured_value::Map::new()) };
    ({ $($tt:tt)+ }) => {
        $crate::model::DecodedValue::Object({
            let mut object = $crate::structured_value::Map::new();
            $crate::structured_value::decoded_value_internal!(@object object () ($($tt)+) ($($tt)+));
            object
        })
    };
    ($other:expr) => {
        $crate::structured_value::to_value(&$other).expect("value must serialize to DecodedValue")
    };
}

#[allow(unused_macros)]
macro_rules! decoded_value_unexpected {
    () => {};
}

#[allow(unused_macros)]
macro_rules! decoded_value_expect_expr_comma {
    ($e:expr , $($tt:tt)*) => {};
}

#[allow(unused_imports)]
pub(crate) use decoded_value_expect_expr_comma;
pub(crate) use decoded_value_internal as json;
pub(crate) use decoded_value_internal;
#[allow(unused_imports)]
pub(crate) use decoded_value_unexpected;

#[cfg(test)]
mod tests {
    use super::*;

    // Every decoded property value is one of these, and a large asset holds tens of
    // millions of them, so the size of the enum multiplies straight into memory and
    // CPU time. A variant that stores a large payload inline makes every value that
    // big; keep such payloads boxed.
    #[test]
    fn a_decoded_value_stays_small() {
        let size = std::mem::size_of::<DecodedValue>();
        assert!(
            size <= 32,
            "DecodedValue is {size} bytes (32 when this guard was written); box the variant that grew it"
        );
    }

    #[derive(Serialize)]
    struct Example {
        name: &'static str,
        count: u32,
    }

    #[test]
    fn macro_and_serializer_build_decoded_values() {
        let value = json!({
            "flag": true,
            "nested": [1, "two", null],
            "typed": Example { name: "asset", count: 3 },
        });

        assert_eq!(value["flag"].as_bool(), Some(true));
        assert_eq!(value["nested"].as_array().unwrap().len(), 3);
        assert_eq!(value["typed"]["name"].as_str(), Some("asset"));
        assert_eq!(value["typed"]["count"].as_u64(), Some(3));
    }

    fn opaque_example() -> OpaqueValue {
        OpaqueValue {
            reason: OpaqueReason::PayloadTail,
            message: "3 bytes follow the decoded fields".into(),
            type_name: Some("Example".into()),
            byte_range: OpaqueByteRange {
                start: 10,
                end: 13,
                size: 3,
                preview: "aabbcc".into(),
            },
        }
    }

    #[test]
    fn an_opaque_value_survives_the_json_macro() {
        let value = json!({ "a": [DecodedValue::opaque(opaque_example())] });

        assert_eq!(value["a"][0].as_opaque(), Some(&opaque_example()));
        assert!(value["a"][0].is_opaque());
        assert!(value["a"][0].as_object().is_none());
        assert!(value["a"][0].get("reason").is_none());
    }

    #[test]
    fn an_opaque_value_renders_and_parses_back_through_its_wire_shape() {
        let value = DecodedValue::opaque(opaque_example());
        let rendered = serde_json_crate::to_value(&value).unwrap();

        assert_eq!(
            rendered,
            serde_json_crate::json!({
                "@opaque": "payload_tail",
                "reason": "3 bytes follow the decoded fields",
                "type": "Example",
                "byte_range": { "start": 10, "end": 13, "size": 3, "preview": "aabbcc" },
            })
        );
        let parsed: DecodedValue = serde_json_crate::from_value(rendered).unwrap();
        assert_eq!(parsed, value);

        let mut untyped = opaque_example();
        untyped.type_name = None;
        let rendered = serde_json_crate::to_value(DecodedValue::opaque(untyped)).unwrap();
        assert!(rendered.get("type").is_none());
    }

    #[test]
    fn an_object_with_extra_keys_is_not_mistaken_for_an_opaque_value() {
        let rendered = serde_json_crate::json!({
            "@opaque": "payload_tail",
            "reason": "x",
            "byte_range": { "start": 0, "end": 0, "size": 0, "preview": "" },
            "extra": 1,
        });
        let parsed: DecodedValue = serde_json_crate::from_value(rendered).unwrap();
        assert!(!parsed.is_opaque());
        assert!(parsed.as_object().is_some());
    }
}
