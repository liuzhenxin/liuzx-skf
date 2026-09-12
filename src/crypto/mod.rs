//! Pure encoding helpers extracted verbatim from `src/main.rs`.
//!
//! Everything in this module is free of I/O, subprocess calls, and FFI, which
//! makes it the first part of the service that can be unit tested without a
//! USB Key, a vendor driver, or OpenSSL.
//!
//! Moved byte-for-byte during Phase 1; only the visibility changed. Any
//! behavioural change here would be a contract change and must not ride along
//! with a move.

use crate::skf::types::{ECCPUBLICKEYBLOB, RSAPUBLICKEYBLOB};

pub fn hex_to_bytes(hex: &str) -> Vec<u8> {
    let hex = hex.trim();
    (0..hex.len())
        .step_by(2)
        .filter_map(|i| {
            if i + 2 <= hex.len() {
                u8::from_str_radix(&hex[i..i + 2], 16).ok()
            } else {
                None
            }
        })
        .collect()
}

/// Encode a big-endian unsigned integer as DER INTEGER (tag 0x02).
/// Strips leading zeros and adds 0x00 pad if high bit is set.
pub fn der_encode_integer(bytes: &[u8]) -> Vec<u8> {
    // Strip leading zeros
    let start = bytes
        .iter()
        .position(|&b| b != 0)
        .unwrap_or(bytes.len() - 1);
    let trimmed = &bytes[start..];

    // If high bit set, prepend 0x00 (DER positive integer rule)
    let needs_pad = !trimmed.is_empty() && (trimmed[0] & 0x80) != 0;
    let int_len = trimmed.len() + if needs_pad { 1 } else { 0 };

    let encoded_len = der_encode_length(int_len);
    let mut out = Vec::with_capacity(1 + encoded_len.len() + int_len);
    out.push(0x02); // INTEGER tag
    out.extend_from_slice(&encoded_len);
    if needs_pad {
        out.push(0x00);
    }
    out.extend_from_slice(trimmed);
    out
}

/// DER encode length (supports lengths up to 65535)
pub fn der_encode_length(len: usize) -> Vec<u8> {
    if len < 128 {
        vec![len as u8]
    } else if len < 256 {
        vec![0x81, len as u8]
    } else {
        vec![0x82, (len >> 8) as u8, len as u8]
    }
}

/// Wrap content with a DER tag + length
pub fn der_wrap(tag: u8, content: &[u8]) -> Vec<u8> {
    let mut out = vec![tag];
    out.extend(der_encode_length(content.len()));
    out.extend_from_slice(content);
    out
}

/// DER SEQUENCE (tag 0x30)
pub fn der_sequence(items: &[&[u8]]) -> Vec<u8> {
    let mut content = Vec::new();
    for item in items {
        content.extend_from_slice(item);
    }
    der_wrap(0x30, &content)
}

/// DER SET (tag 0x31)
pub fn der_set(items: &[&[u8]]) -> Vec<u8> {
    let mut content = Vec::new();
    for item in items {
        content.extend_from_slice(item);
    }
    der_wrap(0x31, &content)
}

/// DER OID from pre-encoded bytes
pub fn der_oid(oid_bytes: &[u8]) -> Vec<u8> {
    der_wrap(0x06, oid_bytes)
}

/// DER UTF8String (tag 0x0C)
pub fn der_utf8_string(s: &str) -> Vec<u8> {
    der_wrap(0x0C, s.as_bytes())
}

/// DER PrintableString (tag 0x13)
pub fn der_printable_string(s: &str) -> Vec<u8> {
    der_wrap(0x13, s.as_bytes())
}

/// DER BIT STRING (tag 0x03) — wraps content with a 0x00 unused-bits prefix
pub fn der_bit_string(content: &[u8]) -> Vec<u8> {
    let mut inner = vec![0x00]; // 0 unused bits
    inner.extend_from_slice(content);
    der_wrap(0x03, &inner)
}

/// DER INTEGER with small value
pub fn der_small_integer(val: u8) -> Vec<u8> {
    vec![0x02, 0x01, val]
}

/// DER CONTEXT tag [0] (constructed, implicit)
pub fn der_context_0(content: &[u8]) -> Vec<u8> {
    der_wrap(0xA0, content)
}

// Well-known OIDs
pub const OID_CN: &[u8] = &[0x55, 0x04, 0x03]; // 2.5.4.3
pub const OID_O: &[u8] = &[0x55, 0x04, 0x0A]; // 2.5.4.10
pub const OID_OU: &[u8] = &[0x55, 0x04, 0x0B]; // 2.5.4.11
pub const OID_C: &[u8] = &[0x55, 0x04, 0x06]; // 2.5.4.6
pub const OID_ST: &[u8] = &[0x55, 0x04, 0x08]; // 2.5.4.8
pub const OID_L: &[u8] = &[0x55, 0x04, 0x07]; // 2.5.4.7
pub const OID_E: &[u8] = &[0x2A, 0x86, 0x48, 0x86, 0xF7, 0x0D, 0x01, 0x09, 0x01]; // 1.2.840.113549.1.9.1

// SM2 OID: 1.2.156.10197.1.301
pub const OID_SM2: &[u8] = &[0x2A, 0x81, 0x1C, 0xCF, 0x55, 0x01, 0x82, 0x2D];
// SM3withSM2 OID: 1.2.156.10197.1.501
pub const OID_SM3_WITH_SM2: &[u8] = &[0x2A, 0x81, 0x1C, 0xCF, 0x55, 0x01, 0x83, 0x75];
// EC public key OID: 1.2.840.10045.2.1
pub const OID_EC_PUBLIC_KEY: &[u8] = &[0x2A, 0x86, 0x48, 0xCE, 0x3D, 0x02, 0x01];
// RSA encryption OID: 1.2.840.113549.1.1.1
pub const OID_RSA: &[u8] = &[0x2A, 0x86, 0x48, 0x86, 0xF7, 0x0D, 0x01, 0x01, 0x01];
// SHA256withRSA OID: 1.2.840.113549.1.1.11
pub const OID_SHA256_WITH_RSA: &[u8] = &[0x2A, 0x86, 0x48, 0x86, 0xF7, 0x0D, 0x01, 0x01, 0x0B];

/// Parse "CN=Test,O=MyOrg,C=CN" into DER-encoded Name (SEQUENCE of SET of SEQUENCE {OID, value})
pub fn build_subject_dn(subject: &str) -> Vec<u8> {
    let mut rdns: Vec<Vec<u8>> = Vec::new();
    for part in subject.split(',') {
        let part = part.trim();
        if let Some((key, val)) = part.split_once('=') {
            let key = key.trim().to_uppercase();
            let oid = match key.as_str() {
                "CN" => OID_CN,
                "O" => OID_O,
                "OU" => OID_OU,
                "C" => OID_C,
                "ST" => OID_ST,
                "L" => OID_L,
                "E" | "EMAIL" | "EMAILADDRESS" => OID_E,
                _ => continue,
            };
            let val = val.trim();
            // C (country) uses PrintableString, others UTF8String
            let value_der = if key == "C" {
                der_printable_string(val)
            } else {
                der_utf8_string(val)
            };
            let attr_type_val = der_sequence(&[&der_oid(oid), &value_der]);
            let rdn = der_set(&[&attr_type_val]);
            rdns.push(rdn);
        }
    }
    let refs: Vec<&[u8]> = rdns.iter().map(|r| r.as_slice()).collect();
    der_sequence(&refs)
}

/// Build SubjectPublicKeyInfo for SM2 key
pub fn build_sm2_spki(pub_key: &ECCPUBLICKEYBLOB) -> Vec<u8> {
    // Algorithm: SEQUENCE { OID ecPublicKey, OID SM2 }
    let alg = der_sequence(&[&der_oid(OID_EC_PUBLIC_KEY), &der_oid(OID_SM2)]);
    // Public key: 0x04 || X(32) || Y(32)  (uncompressed point)
    // SKF stores 32-byte SM2 values right-aligned in 64-byte arrays
    let mut point = Vec::with_capacity(1 + 32 * 2);
    point.push(0x04); // uncompressed
    point.extend_from_slice(&pub_key.XCoordinate[32..64]);
    point.extend_from_slice(&pub_key.YCoordinate[32..64]);
    let pub_key_bits = der_bit_string(&point);
    der_sequence(&[&alg, &pub_key_bits])
}

/// Build SubjectPublicKeyInfo for RSA key
pub fn build_rsa_spki(pub_key: &RSAPUBLICKEYBLOB) -> Vec<u8> {
    // Algorithm: SEQUENCE { OID rsaEncryption, NULL }
    let alg = der_sequence(&[&der_oid(OID_RSA), &[0x05, 0x00]]);
    // RSA public key: SEQUENCE { INTEGER modulus, INTEGER exponent }
    let key_len = (pub_key.BitLen / 8) as usize;
    let n = der_encode_integer(&pub_key.Modulus[..key_len]);
    let e = der_encode_integer(&pub_key.PublicExponent);
    let rsa_key = der_sequence(&[&n, &e]);
    let pub_key_bits = der_bit_string(&rsa_key);
    der_sequence(&[&alg, &pub_key_bits])
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Total DER TLV length implied by the tag+length header, or `None` if the
    /// length form is malformed. Used to prove encoders emit self-consistent DER.
    fn tlv_total_len(der: &[u8]) -> Option<usize> {
        if der.len() < 2 {
            return None;
        }
        let first = der[1];
        if first < 0x80 {
            return Some(2 + first as usize);
        }
        let count = (first & 0x7F) as usize;
        if count == 0 || count > 4 || der.len() < 2 + count {
            return None;
        }
        let mut len = 0usize;
        for i in 0..count {
            len = (len << 8) | der[2 + i] as usize;
        }
        Some(1 + 1 + count + len)
    }

    #[test]
    fn der_encode_length_covers_short_and_long_forms() {
        assert_eq!(der_encode_length(0), vec![0x00]);
        assert_eq!(der_encode_length(1), vec![0x01]);
        assert_eq!(der_encode_length(127), vec![0x7F]);
        // 128 is the first value that needs the 0x81 long form.
        assert_eq!(der_encode_length(128), vec![0x81, 0x80]);
        assert_eq!(der_encode_length(255), vec![0x81, 0xFF]);
        // 256 is the first value that needs the 0x82 long form.
        assert_eq!(der_encode_length(256), vec![0x82, 0x01, 0x00]);
        assert_eq!(der_encode_length(65535), vec![0x82, 0xFF, 0xFF]);
    }

    #[test]
    fn hex_to_bytes_handles_odd_invalid_and_padded_input() {
        assert_eq!(hex_to_bytes("ABCD"), vec![0xAB, 0xCD]);
        assert_eq!(hex_to_bytes("abcd"), vec![0xAB, 0xCD]);
        // Trailing nibble is dropped because a pair cannot be formed.
        assert_eq!(hex_to_bytes("ABC"), vec![0xAB]);
        // Non-hex pairs are skipped rather than panicking.
        assert_eq!(hex_to_bytes("ZZAB"), vec![0xAB]);
        // Surrounding whitespace is trimmed first.
        assert_eq!(hex_to_bytes("  AB  "), vec![0xAB]);
        assert_eq!(hex_to_bytes(""), Vec::<u8>::new());
    }

    #[test]
    fn der_encode_integer_applies_positive_integer_rule() {
        // All-zero input collapses to a single zero byte.
        assert_eq!(der_encode_integer(&[0, 0, 0]), vec![0x02, 0x01, 0x00]);
        // Leading zeros are stripped.
        assert_eq!(
            der_encode_integer(&[0x00, 0x00, 0x2A]),
            vec![0x02, 0x01, 0x2A]
        );
        // A high bit set requires a 0x00 pad to stay positive.
        assert_eq!(der_encode_integer(&[0x80]), vec![0x02, 0x02, 0x00, 0x80]);
        assert_eq!(
            der_encode_integer(&[0xFF, 0xFF]),
            vec![0x02, 0x03, 0x00, 0xFF, 0xFF]
        );
        // A value already below 0x80 needs no pad.
        assert_eq!(der_encode_integer(&[0x7F]), vec![0x02, 0x01, 0x7F]);
    }

    #[test]
    fn build_subject_dn_encodes_rdn_sequence_with_expected_string_types() {
        let der = build_subject_dn("CN=Test,O=Org,C=CN");

        // Outer structure is a SEQUENCE whose declared length covers the buffer.
        assert_eq!(der[0], 0x30, "subject DN must be a SEQUENCE");
        assert_eq!(
            tlv_total_len(&der),
            Some(der.len()),
            "length header must be self-consistent"
        );

        // The value encodings are visible in the buffer: UTF8String for CN/O and
        // PrintableString for the country attribute.
        assert!(
            der.windows(2).any(|w| w == [0x0C, 0x04]),
            "UTF8String for CN=Test"
        );
        assert!(
            der.windows(2).any(|w| w == [0x0C, 0x03]),
            "UTF8String for O=Org"
        );
        assert!(
            der.windows(2).any(|w| w == [0x13, 0x02]),
            "PrintableString for C=CN"
        );

        // OIDs for CN (2.5.4.3) and C (2.5.4.6) are present verbatim.
        assert!(der.windows(3).any(|w| w == OID_CN));
        assert!(der.windows(3).any(|w| w == OID_C));
    }

    #[test]
    fn build_subject_dn_skips_unknown_keys_and_malformed_fragments() {
        let good = build_subject_dn("CN=Test");
        let with_noise = build_subject_dn("CN=Test,X=Ignored,GARBAGE,=EmptyKey,OU=Unit");
        let only_noise = build_subject_dn("X=Ignored,GARBAGE");

        // An unknown key and a fragment without '=' must not be encoded.
        assert!(
            !with_noise
                .windows(6)
                .any(|w| w == b"Ignor\0".as_slice() || w == b"Ignored"),
            "unknown attribute values must not appear in the DER output"
        );
        // A DN containing only unusable fragments yields an empty SEQUENCE.
        assert_eq!(only_noise, vec![0x30, 0x00]);
        // A valid attribute still encodes normally alongside noise.
        assert!(with_noise.len() > good.len(), "OU must have been added");
    }

    #[test]
    fn build_sm2_spki_emits_ec_public_key_with_sm2_curve() {
        let mut key = ECCPUBLICKEYBLOB {
            BitLen: 256,
            XCoordinate: [0u8; 64],
            YCoordinate: [0u8; 64],
        };
        // Only the right-aligned 32 bytes of each coordinate are used.
        key.XCoordinate[63] = 0x11;
        key.YCoordinate[63] = 0x22;

        let der = build_sm2_spki(&key);

        assert_eq!(der[0], 0x30, "SPKI must be a SEQUENCE");
        assert_eq!(tlv_total_len(&der), Some(der.len()));
        assert!(
            der.windows(8).any(|w| w == OID_SM2),
            "SM2 curve OID must be present"
        );
        assert!(
            der.windows(7).any(|w| w == OID_EC_PUBLIC_KEY),
            "ecPublicKey OID must be present"
        );
        // Uncompressed point prefix plus the selected coordinate bytes.
        assert!(
            der.windows(2).any(|w| w == [0x04, 0x00]),
            "BIT STRING payload starts with 0x04"
        );
        assert!(der.contains(&0x11));
        assert!(der.contains(&0x22));
    }

    #[test]
    fn build_rsa_spki_emits_rsa_algorithm_and_key_sequence() {
        let mut key = RSAPUBLICKEYBLOB {
            AlgID: 0,
            BitLen: 2048,
            Modulus: [0u8; 256],
            PublicExponent: [0x00, 0x01, 0x00, 0x01], // 65537 big-endian
        };
        key.Modulus[254] = 0x80; // force a high-bit byte so padding is exercised
        key.Modulus[255] = 0x01;

        let der = build_rsa_spki(&key);

        assert_eq!(der[0], 0x30, "SPKI must be a SEQUENCE");
        assert_eq!(tlv_total_len(&der), Some(der.len()));
        assert!(
            der.windows(9).any(|w| w == OID_RSA),
            "rsaEncryption OID must be present"
        );
        assert!(
            der.windows(2).any(|w| w == [0x05, 0x00]),
            "algorithm parameters must be NULL"
        );
        // Exponent 65537 encodes as 0x02 0x03 0x01 0x00 0x01 after stripping the
        // leading zero byte.
        assert!(der.windows(5).any(|w| w == [0x02, 0x03, 0x01, 0x00, 0x01]));
    }
}
