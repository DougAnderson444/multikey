// SPDX-License-Idnetifier: Apache-2.0
use crate::{
    error::{AttributesError, CipherError, ConversionsError, KdfError, SignError, VerifyError},
    AttrId, AttrView, Builder, CipherAttrView, ConvView, DataView, Error, FingerprintView,
    KdfAttrView, Multikey, SignView, VerifyView, Views,
};
use ed25519_dalek::{
    Signature, Signer, SigningKey, VerifyingKey, PUBLIC_KEY_LENGTH, SECRET_KEY_LENGTH,
};
use multicodec::Codec;
use multihash::{mh, Multihash};
use multisig::{ms, Multisig, Views as SigViews};
use multitrait::TryDecodeFrom;
use multiutil::Varuint;
use zeroize::Zeroizing;

pub(crate) struct View<'a> {
    mk: &'a Multikey,
}

impl<'a> TryFrom<&'a Multikey> for View<'a> {
    type Error = Error;

    fn try_from(mk: &'a Multikey) -> Result<Self, Self::Error> {
        Ok(Self { mk })
    }
}

impl<'a> AttrView for View<'a> {
    fn is_encrypted(&self) -> bool {
        if let Some(v) = self.mk.attributes.get(&AttrId::KeyIsEncrypted) {
            if let Ok((b, _)) = Varuint::<bool>::try_decode_from(v.as_slice()) {
                return b.to_inner();
            }
        }
        false
    }

    fn is_secret_key(&self) -> bool {
        self.mk.codec == Codec::Ed25519Priv
    }

    fn is_public_key(&self) -> bool {
        self.mk.codec == Codec::Ed25519Pub
    }

    fn is_secret_key_share(&self) -> bool {
        false
    }
}

impl<'a> DataView for View<'a> {
    /// For Ed25519Pub and Ed25519Priv Multikey values, the key data is stored
    /// using the AttrId::Data attribute id.
    fn key_bytes(&self) -> Result<Zeroizing<Vec<u8>>, Error> {
        let key = self
            .mk
            .attributes
            .get(&AttrId::KeyData)
            .ok_or(AttributesError::MissingKey)?;
        Ok(key.clone())
    }

    /// Check to see if this is a secret key before returning the key bytes
    fn secret_bytes(&self) -> Result<Zeroizing<Vec<u8>>, Error> {
        if !self.is_secret_key() {
            return Err(AttributesError::NotSecretKey(self.mk.codec).into());
        }
        if self.is_encrypted() {
            return Err(AttributesError::EncryptedKey.into());
        }
        self.key_bytes()
    }
}

impl<'a> CipherAttrView for View<'a> {
    fn cipher_codec(&self) -> Result<Codec, Error> {
        // try to look up the cipher codec in the multikey attributes
        let codec = self
            .mk
            .attributes
            .get(&AttrId::CipherCodec)
            .ok_or(CipherError::MissingCodec)?;
        Ok(Codec::try_from(codec.as_slice())?)
    }

    fn nonce_bytes(&self) -> Result<Zeroizing<Vec<u8>>, Error> {
        // try to look up the salt in the multikey attributes
        self.mk
            .attributes
            .get(&AttrId::CipherNonce)
            .ok_or(CipherError::MissingNonce.into())
            .cloned()
    }

    fn key_length(&self) -> Result<usize, Error> {
        // try to look up the cipher key length in the multikey attributes
        let key_length = self
            .mk
            .attributes
            .get(&AttrId::CipherKeyLen)
            .ok_or(CipherError::MissingKeyLen)?;
        Ok(Varuint::<usize>::try_from(key_length.as_slice())?.to_inner())
    }
}

impl<'a> KdfAttrView for View<'a> {
    fn kdf_codec(&self) -> Result<Codec, Error> {
        // try to look up the kdf codec in the multikey attributes
        let codec = self
            .mk
            .attributes
            .get(&AttrId::KdfCodec)
            .ok_or(KdfError::MissingCodec)?;
        Ok(Codec::try_from(codec.as_slice())?)
    }

    fn salt_bytes(&self) -> Result<Zeroizing<Vec<u8>>, Error> {
        // try to look up the salt in the multikey attributes
        self.mk
            .attributes
            .get(&AttrId::KdfSalt)
            .ok_or(KdfError::MissingSalt.into())
            .cloned()
    }

    fn rounds(&self) -> Result<usize, Error> {
        // try to look up the rounds in the multikey attributes
        let rounds = self
            .mk
            .attributes
            .get(&AttrId::KdfRounds)
            .ok_or(KdfError::MissingRounds)?;
        Ok(Varuint::<usize>::try_from(rounds.as_slice())?.to_inner())
    }
}

impl<'a> FingerprintView for View<'a> {
    fn fingerprint(&self, codec: Codec) -> Result<Multihash, Error> {
        let attr = self.mk.attr_view()?;
        if attr.is_secret_key() {
            // convert to a public key Multikey
            let pk = self.to_public_key()?;
            // get a conversions view on the public key
            let fp = pk.fingerprint_view()?;
            // get the fingerprint
            let f = fp.fingerprint(codec)?;
            Ok(f)
        } else {
            // get the key bytes
            let bytes = {
                let kd = self.mk.data_view()?;
                
                kd.key_bytes()?
            };
            // hash the key bytes using the given codec
            Ok(mh::Builder::new_from_bytes(codec, bytes)?.try_build()?)
        }
    }
}

impl<'a> ConvView for View<'a> {
    /// try to convert a secret key to a public key
    fn to_public_key(&self) -> Result<Multikey, Error> {
        // get the secret key bytes
        let secret_bytes = {
            let kd = self.mk.data_view()?;
            
            kd.secret_bytes()?
        };

        // build an Ed25519 signing key so that we can derive the verifying key
        let bytes: [u8; SECRET_KEY_LENGTH] = secret_bytes.as_slice()[..SECRET_KEY_LENGTH]
            .try_into()
            .map_err(|_| {
                ConversionsError::SecretKeyFailure("failed to get secret key bytes".to_string())
            })?;
        let secret_key = SigningKey::from_bytes(&bytes);
        // get the public key and build a Multikey out of it
        let public_key = secret_key.verifying_key();
        Builder::new(Codec::Ed25519Pub)
            .with_comment(&self.mk.comment)
            .with_key_bytes(public_key.as_bytes())
            .try_build()
    }

    /// try to convert a Multikey to an ssh_key::PublicKey
    fn to_ssh_public_key(&self) -> Result<ssh_key::PublicKey, Error> {
        let mut pk = self.mk.clone();
        if self.is_secret_key() {
            pk = self.to_public_key()?;
        }

        let key_bytes = {
            let kd = pk.data_view()?;
            
            kd.key_bytes()?
        };

        // get the key bytes in a fix length slice
        let bytes: [u8; PUBLIC_KEY_LENGTH] = key_bytes.as_slice()[..PUBLIC_KEY_LENGTH]
            .try_into()
            .map_err(|_| {
            ConversionsError::PublicKeyFailure("failed to get key bytes".to_string())
        })?;

        Ok(ssh_key::PublicKey::new(
            ssh_key::public::KeyData::Ed25519(ssh_key::public::Ed25519PublicKey(bytes)),
            pk.comment,
        ))
    }

    /// try to convert a Multikey to an ssh_key::PrivateKey
    fn to_ssh_private_key(&self) -> Result<ssh_key::PrivateKey, Error> {
        let secret_bytes = {
            let kd = self.mk.data_view()?;
            
            kd.secret_bytes()?
        };

        // build an Ed25519 signing key so that we can derive the verifying key
        let bytes: [u8; SECRET_KEY_LENGTH] = secret_bytes.as_slice()[..SECRET_KEY_LENGTH]
            .try_into()
            .map_err(|_| {
                ConversionsError::SecretKeyFailure("failed to get secret key bytes".to_string())
            })?;

        let pk = self.to_public_key()?;
        let data = pk.data_view()?;
        let public_bytes: [u8; PUBLIC_KEY_LENGTH] = data.key_bytes()?.as_slice()
            [..PUBLIC_KEY_LENGTH]
            .try_into()
            .map_err(|_| {
                ConversionsError::PublicKeyFailure("failed to get public key bytes".to_string())
            })?;

        Ok(ssh_key::PrivateKey::new(
            ssh_key::private::KeypairData::Ed25519(ssh_key::private::Ed25519Keypair {
                public: ssh_key::public::Ed25519PublicKey(public_bytes),
                private: ssh_key::private::Ed25519PrivateKey::from_bytes(&bytes),
            }),
            self.mk.comment.clone(),
        )
        .map_err(|e| ConversionsError::Ssh(e.into()))?)
    }
}

impl<'a> SignView for View<'a> {
    /// try to create a Multisig by siging the passed-in data with the Multikey
    fn sign(&self, msg: &[u8], combined: bool, _scheme: Option<u8>) -> Result<Multisig, Error> {
        let attr = self.mk.attr_view()?;
        if !attr.is_secret_key() {
            return Err(SignError::NotSigningKey.into());
        }

        // get the secret key bytes
        let secret_bytes = {
            let kd = self.mk.data_view()?;
            
            kd.secret_bytes()?
        };

        let secret_key = {
            // build an Ed25519 signing key so that we can derive the verifying key
            let bytes: [u8; SECRET_KEY_LENGTH] = secret_bytes.as_slice()[..SECRET_KEY_LENGTH]
                .try_into()
                .map_err(|_| {
                    ConversionsError::SecretKeyFailure("failed to get secret key bytes".to_string())
                })?;
            
            SigningKey::from_bytes(&bytes)
        };

        // sign the data
        let signature = secret_key
            .try_sign(msg)
            .map_err(|e| SignError::SigningFailed(e.to_string()))?;

        let mut ms = ms::Builder::new(Codec::EddsaMsig).with_signature_bytes(&signature.to_bytes());
        if combined {
            ms = ms.with_message_bytes(&msg);
        }
        Ok(ms.try_build()?)
    }
}

impl<'a> VerifyView for View<'a> {
    /// try to verify a Multisig using the Multikey
    fn verify(&self, multisig: &Multisig, msg: Option<&[u8]>) -> Result<(), Error> {
        let attr = self.mk.attr_view()?;
        let pubmk = if attr.is_secret_key() {
            let kc = self.mk.conv_view()?;
            
            kc.to_public_key()?
        } else {
            self.mk.clone()
        };

        // get the secret key bytes
        let key_bytes = {
            let kd = pubmk.data_view()?;
            
            kd.key_bytes()?
        };

        // build an Ed25519 verifying key so that we can derive the verifying key
        let bytes: [u8; PUBLIC_KEY_LENGTH] = key_bytes.as_slice()[..PUBLIC_KEY_LENGTH]
            .try_into()
            .map_err(|_| {
            ConversionsError::PublicKeyFailure("failed to get public key bytes".to_string())
        })?;

        // create the verifying key
        let verifying_key = VerifyingKey::from_bytes(&bytes)
            .map_err(|e| ConversionsError::PublicKeyFailure(e.to_string()))?;

        // get the signature data
        let sv = multisig.data_view()?;
        let sig = sv.sig_bytes().map_err(|_| VerifyError::MissingSignature)?;

        // create the signature
        let sig = Signature::from_slice(sig.as_slice())
            .map_err(|e| VerifyError::BadSignature(e.to_string()))?;

        // get the message
        let msg = if let Some(msg) = msg {
            msg
        } else if !multisig.message.is_empty() {
            multisig.message.as_slice()
        } else {
            return Err(VerifyError::MissingMessage.into());
        };

        verifying_key
            .verify_strict(msg, &sig)
            .map_err(|e| VerifyError::BadSignature(e.to_string()))?;

        Ok(())
    }
}
