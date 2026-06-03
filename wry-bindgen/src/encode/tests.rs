#[cfg(all(test, feature = "enable-interning"))]
mod tests {
    use alloc::vec;

    use crate::EncodedData;
    use crate::encode::BinaryEncode;

    #[test]
    fn interned_strings_encode_as_cached_heap_refs() {
        crate::intern::insert_for_test("cached", 0x0000_0001_0000_0002);

        let mut cached = EncodedData::default();
        "cached".encode(&mut cached);
        assert_eq!(
            cached.u32_buf,
            vec![crate::ipc::CACHED_STRING_SENTINEL, 2, 1]
        );
        assert!(cached.str_buf.is_empty());

        let mut inline = EncodedData::default();
        "uncached".encode(&mut inline);
        assert_eq!(inline.u32_buf, vec![8]);
        assert_eq!(inline.str_buf, b"uncached".to_vec());

        crate::intern::unintern("cached");
    }
}

#[cfg(test)]
mod decode_error_tests {
    use alloc::{string::String, vec::Vec};

    use crate::encode::BinaryDecode;
    use crate::{Clamped, DecodeError, DecodedData, EncodedData};

    // A real Rust<->JS round-trip never produces a truncated or malformed buffer,
    // so the `take_*()?` failure branches in every `decode` are only reachable by
    // feeding a `Decoder` a short/garbage buffer directly. These cover those paths.

    #[test]
    fn primitive_decode_errors_on_empty_buffer() {
        macro_rules! assert_decode_err {
            ($($t:ty),* $(,)?) => {$({
                // header only, every value sub-buffer empty
                let bytes = EncodedData::default().to_bytes();
                let mut d = DecodedData::from_bytes(&bytes).unwrap();
                assert!(
                    <$t as BinaryDecode>::decode(&mut d).is_err(),
                    "expected decode error for {}", stringify!($t)
                );
            })*};
        }
        assert_decode_err!(
            u8, u16, u32, u64, u128, i8, i16, i32, i64, i128, f32, f64, usize, isize, bool, char,
            String,
        );
    }

    #[test]
    fn char_decode_rejects_invalid_scalar_value() {
        let mut enc = EncodedData::default();
        enc.push_u32(0x11_0000); // one past the maximum Unicode scalar value
        let bytes = enc.to_bytes();
        let mut d = DecodedData::from_bytes(&bytes).unwrap();
        assert!(matches!(char::decode(&mut d), Err(DecodeError::Custom(_))));
    }

    #[test]
    fn string_decode_errors_on_truncation_and_bad_utf8() {
        // length header claims 5 bytes, but the string buffer is empty
        let mut enc = EncodedData::default();
        enc.push_u32(5);
        let bytes = enc.to_bytes();
        let mut d = DecodedData::from_bytes(&bytes).unwrap();
        assert!(matches!(
            String::decode(&mut d),
            Err(DecodeError::StringBufferTooShort { .. })
        ));

        // length header says 2 bytes, body is invalid UTF-8
        let mut enc = EncodedData::default();
        enc.push_u32(2);
        enc.str_buf.extend_from_slice(&[0xff, 0xfe]);
        let bytes = enc.to_bytes();
        let mut d = DecodedData::from_bytes(&bytes).unwrap();
        assert!(matches!(
            String::decode(&mut d),
            Err(DecodeError::InvalidUtf8 { .. })
        ));
    }

    #[test]
    fn clamped_decode_errors_when_buffer_truncated() {
        // length header claims 3 bytes, only 1 present
        let mut enc = EncodedData::default();
        enc.push_u32(3);
        enc.push_u8(1);
        let bytes = enc.to_bytes();
        let mut d = DecodedData::from_bytes(&bytes).unwrap();
        assert!(Clamped::<Vec<u8>>::decode(&mut d).is_err());
    }
}
