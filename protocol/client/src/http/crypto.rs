//! Pairing cryptography: AES-128-ECB with zero padding, SHA-1/SHA-256, and RSA or ECDSA
//! signatures with SHA-256, as moonlight-android's PairingManager uses them.

use super::Error;
use super::der::{self, CertificateParts};
use aes::cipher::{BlockDecrypt, BlockEncrypt, KeyInit};
use ring::rand::{SecureRandom, SystemRandom};
use ring::signature;

pub(crate) fn random<const N: usize>() -> [u8; N] {
    let mut bytes = [0; N];
    SystemRandom::new()
        .fill(&mut bytes)
        .expect("the system random number generator failed");
    bytes
}

/// Encrypts or decrypts block by block; the input is zero-padded to whole blocks.
pub(crate) fn aes_ecb(key: &[u8; 16], data: &[u8], encrypt: bool) -> Vec<u8> {
    let cipher = aes::Aes128::new(key.into());
    let mut out = data.to_vec();
    out.resize(data.len().div_ceil(16) * 16, 0);
    for block in out.as_chunks_mut::<16>().0 {
        let block = aes::Block::from_mut_slice(block);
        if encrypt {
            cipher.encrypt_block(block);
        } else {
            cipher.decrypt_block(block);
        }
    }
    out
}

/// The pairing hash: SHA-256 for GameStream generation 7 and later, SHA-1 before.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PairingHash {
    Sha1,
    Sha256,
}

impl PairingHash {
    pub fn for_generation(major: i32) -> Self {
        if major >= 7 { Self::Sha256 } else { Self::Sha1 }
    }

    pub fn len(self) -> usize {
        match self {
            Self::Sha1 => 20,
            Self::Sha256 => 32,
        }
    }

    pub fn hash(self, data: &[u8]) -> Vec<u8> {
        let algorithm = match self {
            Self::Sha1 => &ring::digest::SHA1_FOR_LEGACY_USE_ONLY,
            Self::Sha256 => &ring::digest::SHA256,
        };
        ring::digest::digest(algorithm, data).as_ref().to_vec()
    }
}

pub(crate) fn sha256(data: &[u8]) -> Vec<u8> {
    PairingHash::Sha256.hash(data)
}

/// RSA PKCS#1 v1.5 with SHA-256 using a PKCS#8 private key.
pub(crate) fn sign(pkcs8: &[u8], data: &[u8]) -> Result<Vec<u8>, Error> {
    let key = signature::RsaKeyPair::from_pkcs8(pkcs8)
        .map_err(|e| Error::InvalidIdentity(format!("unusable RSA private key: {e}")))?;
    let mut out = vec![0; key.public().modulus_len()];
    key.sign(
        &signature::RSA_PKCS1_SHA256,
        &SystemRandom::new(),
        data,
        &mut out,
    )
    .map_err(|_| Error::InvalidIdentity("RSA signing failed".into()))?;
    Ok(out)
}

fn verification_algorithm(
    certificate: &CertificateParts,
    certificate_sig_alg: Option<&[u8]>,
) -> Result<&'static dyn signature::VerificationAlgorithm, Error> {
    if certificate.key_algorithm == der::OID_RSA_ENCRYPTION {
        let (_, exponent) = der::rsa_public_key(&certificate.public_key)?;
        if exponent != der::RSA_PUBLIC_EXPONENT_65537 {
            return Err(Error::Malformed(
                "unsupported pairing certificate RSA exponent".into(),
            ));
        }
        if let Some(alg) = certificate_sig_alg
            && alg != der::OID_SHA256_WITH_RSA
        {
            return Err(Error::Malformed(
                "unsupported pairing certificate signature algorithm".into(),
            ));
        }
        Ok(&signature::RSA_PKCS1_2048_8192_SHA256)
    } else if certificate.key_algorithm == der::OID_EC_PUBLIC_KEY {
        match (certificate.key_parameters.as_deref(), certificate_sig_alg) {
            (Some(der::OID_P256), None | Some(der::OID_ECDSA_WITH_SHA256)) => {
                Ok(&signature::ECDSA_P256_SHA256_ASN1)
            }
            (Some(der::OID_P384), None | Some(der::OID_ECDSA_WITH_SHA256)) => {
                Ok(&signature::ECDSA_P384_SHA256_ASN1)
            }
            (Some(der::OID_P384), Some(der::OID_ECDSA_WITH_SHA384)) => {
                Ok(&signature::ECDSA_P384_SHA384_ASN1)
            }
            (Some(der::OID_P256 | der::OID_P384), Some(_)) => Err(Error::Malformed(
                "unsupported pairing certificate signature algorithm".into(),
            )),
            _ => Err(Error::Malformed(
                "unsupported pairing certificate curve".into(),
            )),
        }
    } else {
        Err(Error::Malformed(
            "unsupported pairing certificate key".into(),
        ))
    }
}

/// Verifies a certificate's X.509 self-signature over its raw `TBSCertificate` bytes and validates
/// its public key parameters (including RSA `e == 65537`).
pub(crate) fn verify_self_signature(certificate: &CertificateParts) -> Result<bool, Error> {
    let algorithm = verification_algorithm(certificate, Some(&certificate.signature_algorithm))?;
    Ok(
        signature::UnparsedPublicKey::new(algorithm, &certificate.public_key)
            .verify(&certificate.tbs, &certificate.signature)
            .is_ok(),
    )
}

/// Verifies a SHA-256 signature made by the certificate's key (RSA PKCS#1 v1.5 or ECDSA).
pub(crate) fn verify(
    certificate: &CertificateParts,
    data: &[u8],
    sig: &[u8],
) -> Result<bool, Error> {
    let algorithm = verification_algorithm(certificate, None)?;
    Ok(
        signature::UnparsedPublicKey::new(algorithm, &certificate.public_key)
            .verify(data, sig)
            .is_ok(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ecb_matches_the_fips_197_vector_and_pads_with_zeros() {
        let key: [u8; 16] = core::array::from_fn(|i| i as u8);
        let plain: Vec<u8> = (0..16).map(|i| (i * 0x11) as u8).collect();
        let cipher = aes_ecb(&key, &plain, true);
        assert_eq!(der::hex_encode(&cipher), "69C4E0D86A7B0430D8CDB78070B4C55A");
        assert_eq!(aes_ecb(&key, &cipher, false), plain);
        assert_eq!(aes_ecb(&key, &[1; 20], true).len(), 32);
    }

    #[test]
    fn hashes_have_the_pairing_lengths() {
        assert_eq!(PairingHash::for_generation(7), PairingHash::Sha256);
        assert_eq!(PairingHash::for_generation(6), PairingHash::Sha1);
        assert_eq!(PairingHash::Sha1.hash(b"abc").len(), 20);
        assert_eq!(
            der::hex_encode(&sha256(b"abc")),
            "BA7816BF8F01CFEA414140DE5DAE2223B00361A396177A9CB410FF61F20015AD"
        );
    }
}
