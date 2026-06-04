//! Wire-format helper macros for transparent JsValue wrappers.

macro_rules! impl_js_value_wire {
    (for $ty:ty, field $field:ident) => {
        impl $crate::__rt::EncodeTypeDef for $ty {
            fn encode_type_def(encoder: &mut $crate::__rt::TypeDef) {
                <$crate::JsValue as $crate::__rt::EncodeTypeDef>::encode_type_def(encoder);
            }
        }

        impl $crate::__rt::BinaryEncode for $ty {
            fn encode(self, encoder: &mut $crate::__rt::EncodedData) {
                <$crate::JsValue as $crate::__rt::BinaryEncode>::encode(self.$field, encoder);
            }
        }

        impl $crate::__rt::JsRefEncode for $ty {
            fn js_ref(&self) -> $crate::__rt::JsRef {
                self.$field.js_ref()
            }
        }

        impl $crate::__rt::BinaryDecode for $ty {
            fn decode(
                decoder: &mut $crate::__rt::DecodedData,
            ) -> ::core::result::Result<Self, $crate::__rt::DecodeError> {
                <$crate::JsValue as $crate::__rt::BinaryDecode>::decode(decoder)
                    .map(::core::convert::Into::into)
            }
        }

        impl $crate::__rt::BatchableResult for $ty {
            fn try_placeholder(
                batch: &mut $crate::__rt::Runtime,
            ) -> ::core::option::Option<Self> {
                ::core::option::Option::Some(
                    <$crate::JsValue as $crate::__rt::BatchableResult>::try_placeholder(batch)?.into(),
                )
            }
        }
    };
    (impl<$($generics:ident),*> for $ty:ty, field $field:ident) => {
        impl<$($generics),*> $crate::__rt::EncodeTypeDef for $ty {
            fn encode_type_def(encoder: &mut $crate::__rt::TypeDef) {
                <$crate::JsValue as $crate::__rt::EncodeTypeDef>::encode_type_def(encoder);
            }
        }

        impl<$($generics),*> $crate::__rt::BinaryEncode for $ty {
            fn encode(self, encoder: &mut $crate::__rt::EncodedData) {
                <$crate::JsValue as $crate::__rt::BinaryEncode>::encode(self.$field, encoder);
            }
        }

        impl<$($generics),*> $crate::__rt::JsRefEncode for $ty {
            fn js_ref(&self) -> $crate::__rt::JsRef {
                self.$field.js_ref()
            }
        }

        impl<$($generics),*> $crate::__rt::BinaryDecode for $ty {
            fn decode(
                decoder: &mut $crate::__rt::DecodedData,
            ) -> ::core::result::Result<Self, $crate::__rt::DecodeError> {
                <$crate::JsValue as $crate::__rt::BinaryDecode>::decode(decoder)
                    .map(::core::convert::Into::into)
            }
        }

        impl<$($generics),*> $crate::__rt::BatchableResult for $ty {
            fn try_placeholder(
                batch: &mut $crate::__rt::Runtime,
            ) -> ::core::option::Option<Self> {
                ::core::option::Option::Some(
                    <$crate::JsValue as $crate::__rt::BatchableResult>::try_placeholder(batch)?.into(),
                )
            }
        }
    };
}
