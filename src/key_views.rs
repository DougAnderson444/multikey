use crate::{Error, Multikey};
use multicodec::Codec;
use multihash::Multihash;
use multisig::Multisig;
use std::{cell::RefCell, rc::Rc};
use zeroize::Zeroizing;

// algorithms implement different sets of view
pub(crate) mod bcrypt; // KdfAttrView, KdfView
pub(crate) mod chacha20; // AttrView, ConversionsView, CipherView
pub(crate) mod ed25519; // AttributesView, ConversionsView, CipherView

///
/// Attributes views let you inquire about the Multikey and retrieve data
/// associated with the particular view.
///

/// trait for returning basic, general attributes about a Multikey
pub trait AttrView {
    /// is this key encrypted
    fn is_encrypted(&self) -> bool;
    /// is this key a public key that can be shared? for symmetric encryption
    /// codecs, this is always false. for public key encryption codecs, this
    /// is true if this key is the public key of the key pair.
    fn is_public_key(&self) -> bool;
    /// is this key one that should be kept secret? for symmetric encryption
    /// codecs, this is always true. for public key encryption codecs, this
    /// is true if this key is the secret key of the key pair.
    fn is_secret_key(&self) -> bool;
}

/// trait for viewing the cipher attributes in a Multikey
pub trait CipherAttrView {
    /// get the cipher codec from the viewed multikey
    fn cipher_codec(&self) -> Result<Codec, Error>;
    /// get the nonce bytes from the viewed multikey
    fn nonce_bytes(&self) -> Result<Zeroizing<Vec<u8>>, Error>;
    /// get the nonce lgnth from the viewed multikey
    fn nonce_length(&self) -> Result<usize, Error>;
    /// get the key length from the viewed multikey
    fn key_length(&self) -> Result<usize, Error>;
}

/// trait for viewing the kdf attributes in a Multikey
pub trait KdfAttrView {
    /// get the kdf codec from the viewed multikey
    fn kdf_codec(&self) -> Result<Codec, Error>;
    /// get the salt bytes from the viewed multikey
    fn salt_bytes(&self) -> Result<Zeroizing<Vec<u8>>, Error>;
    /// get the salt length from the viewed multikey
    fn salt_length(&self) -> Result<usize, Error>;
    /// get the number of rounds for the KDF function from the viewed multikey
    fn rounds(&self) -> Result<usize, Error>;
}

/// trait for returning the key data from a Multikey
pub trait KeyDataView {
    /// get the key bytes from the viewed Multikey. this is codec specific.
    fn key_bytes(&self) -> Result<Zeroizing<Vec<u8>>, Error>;
    /// get the bytes that constitutes the secret. this is codec specific and
    /// must return all of the bytes that should be encrypted to protect the
    /// secret part of the key
    fn secret_bytes(&self) -> Result<Zeroizing<Vec<u8>>, Error>;
}

///
/// The following key operations views are functions that generate new
/// Multikeys, Multihashes, or Multisigs from the viewed Multikey (self)
///

/// trait for encrypting and decrypting Multikeys
pub trait CipherView {
    /// decrypt the secret_bytes from the viewed Multikey using the codec and
    /// keys/data in the passed-in Multikey the result is a copy of the viewed
    /// Multikey but with the decrypted bytes stored under the Data attribute.
    fn decrypt(&self) -> Result<Multikey, Error>;
    /// encrypt the secret_bytes from the viewd Multikey using the codec and
    /// keys/data in the passed-in Multikey. the result is a copy of the viewed
    /// Multikey but with the encrypted bytes stored under the Data attribute.
    /// also, the EncryptionCodec and Nonce attributes are set to the values
    /// from the viewed Multikey so that the encrypted Multikey self describes
    /// the encryption cipher used to encrypt it.
    fn encrypt(&self) -> Result<Multikey, Error>;
}

/// trait for fingerpringing a Multikey
pub trait FingerprintView {
    /// get the fingerprint of the viewed Multikey using the passed-in hashing
    /// codec. the fingerprint is the hash of the secret key if it is a
    /// symmetric key and the hash of the public key if this key codec is a
    /// public key codec. if this key is the secret key of a public key codec,
    /// the public key will automatically be derived, if possible using a
    /// KeyConversionView, and the fingerprint will be generated from that.
    fn fingerprint(&self, hash: Codec) -> Result<Multihash, Error>;
}

/// trait for doing key derivation functions using a Multikey for the parameters
pub trait KdfView {
    /// derive a key. the result is a copy of the viewed Multikey with the
    /// derived key in the Data attribute. also, the KdfCodec, Salt, and
    /// Rounds attributes are set to the values from the passed-in Multikey.
    fn derive_key(&self, passphrase: &[u8]) -> Result<Multikey, Error>;
}

/// trait for converting a Multikey in various ways
pub trait KeyConvView {
    /// try to create a Multikey from this view that is the public key part of
    /// a key pair. this always fails for symmetric encryption codecs.
    fn to_public_key(&self) -> Result<Multikey, Error>;
}

/// trait for digially signing data using a multikey
pub trait SignView {
    /// try to create a Multisig by siging the passed-in data with the Multikey
    fn sign(&self, msg: &[u8], combined: bool) -> Result<Multisig, Error>;
}

/// trait for verifying digial signatures using a multikey
pub trait VerifyView {
    /// try to verify a Multisig using the Multikey
    fn verify(&self, sig: &Multisig, msg: Option<&[u8]>) -> Result<(), Error>;
}

/// trait for getting the other views
pub trait KeyViews {
    /// Provide a read-only view of the basic attributes in the viewed Multikey
    fn attr_view<'a>(&'a self) -> Result<Rc<RefCell<dyn AttrView + 'a>>, Error>;
    /// Provide a read-only view of the cipher attributes in the viewed Multikey
    fn cipher_attr_view<'a>(&'a self) -> Result<Rc<RefCell<dyn CipherAttrView + 'a>>, Error>;
    /// Provide a read-only view of the kdf attributes in the viewed Multikey
    fn kdf_attr_view<'a>(&'a self) -> Result<Rc<RefCell<dyn KdfAttrView + 'a>>, Error>;
    /// Provide a read-only view to key data in the viewed Multikey
    fn key_data_view<'a>(&'a self) -> Result<Rc<RefCell<dyn KeyDataView + 'a>>, Error>;
    /// Provide an interface to do encryption/decryption of the viewed Multikey
    fn cipher_view<'a>(
        &'a self,
        cipher: &'a Multikey,
    ) -> Result<Rc<RefCell<dyn CipherView + 'a>>, Error>;
    /// Provide an interface to do key conversions from the viewe Multikey
    fn fingerprint_view<'a>(&'a self) -> Result<Rc<RefCell<dyn FingerprintView + 'a>>, Error>;
    /// Provide an interface to do kdf operations from the viewed Multikey
    fn kdf_view<'a>(&'a self, kdf: &'a Multikey) -> Result<Rc<RefCell<dyn KdfView + 'a>>, Error>;
    /// Provide an interface to do key conversions from the viewe Multikey
    fn key_conv_view<'a>(&'a self) -> Result<Rc<RefCell<dyn KeyConvView + 'a>>, Error>;
    /// Provide an interface to sign a message and return a Multisig
    fn sign_view<'a>(&'a self) -> Result<Rc<RefCell<dyn SignView + 'a>>, Error>;
    /// Provide an interface to verify a Multisig and optional message
    fn verify_view<'a>(&'a self) -> Result<Rc<RefCell<dyn VerifyView + 'a>>, Error>;
}
