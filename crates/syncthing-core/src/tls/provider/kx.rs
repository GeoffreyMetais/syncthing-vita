//! Key exchange groups (X25519, secp256r1, secp384r1) for `rustls`.

use rustls::crypto::{ActiveKeyExchange, SharedSecret, SupportedKxGroup};
use rustls::{Error, NamedGroup, PeerMisbehaved};

#[derive(Debug)]
pub struct X25519;

impl SupportedKxGroup for X25519 {
    fn name(&self) -> NamedGroup {
        NamedGroup::X25519
    }

    fn start(&self) -> Result<Box<dyn ActiveKeyExchange>, Error> {
        let priv_key = x25519_dalek::EphemeralSecret::random_from_rng(rand_core::OsRng);
        let pub_key = (&priv_key).into();
        Ok(Box::new(X25519KeyExchange { priv_key, pub_key }))
    }
}

struct X25519KeyExchange {
    priv_key: x25519_dalek::EphemeralSecret,
    pub_key: x25519_dalek::PublicKey,
}

impl ActiveKeyExchange for X25519KeyExchange {
    fn complete(self: Box<Self>, peer: &[u8]) -> Result<SharedSecret, Error> {
        let peer_array: [u8; 32] = peer
            .try_into()
            .map_err(|_| Error::from(PeerMisbehaved::InvalidKeyShare))?;
        Ok(self
            .priv_key
            .diffie_hellman(&peer_array.into())
            .as_ref()
            .into())
    }

    fn pub_key(&self) -> &[u8] {
        self.pub_key.as_bytes()
    }

    fn group(&self) -> NamedGroup {
        X25519.name()
    }
}

macro_rules! impl_ecdh_kx {
    ($group_ty:ident, $kx_ty:ident, $named_group:expr, $secret:ty, $public_key:ty) => {
        #[derive(Debug)]
        pub struct $group_ty;

        impl SupportedKxGroup for $group_ty {
            fn name(&self) -> NamedGroup {
                $named_group
            }

            fn start(&self) -> Result<Box<dyn ActiveKeyExchange>, Error> {
                use p256::elliptic_curve::Generate as _;
                let mut rng = super::OsRngAdapter;
                let priv_key = <$secret>::generate_from_rng(&mut rng);
                let pub_key: $public_key = (&priv_key).into();
                Ok(Box::new($kx_ty {
                    priv_key,
                    pub_key: pub_key.to_sec1_bytes(),
                }))
            }
        }

        struct $kx_ty {
            priv_key: $secret,
            pub_key: Box<[u8]>,
        }

        impl ActiveKeyExchange for $kx_ty {
            fn complete(self: Box<Self>, peer: &[u8]) -> Result<SharedSecret, Error> {
                let their_pub = <$public_key>::from_sec1_bytes(peer)
                    .map_err(|_| Error::from(PeerMisbehaved::InvalidKeyShare))?;
                Ok(self
                    .priv_key
                    .diffie_hellman(&their_pub)
                    .raw_secret_bytes()
                    .as_slice()
                    .into())
            }

            fn pub_key(&self) -> &[u8] {
                &self.pub_key
            }

            fn group(&self) -> NamedGroup {
                $group_ty.name()
            }
        }
    };
}

impl_ecdh_kx!(
    SecP256R1,
    SecP256R1KeyExchange,
    NamedGroup::secp256r1,
    p256::ecdh::EphemeralSecret,
    p256::PublicKey
);
impl_ecdh_kx!(
    SecP384R1,
    SecP384R1KeyExchange,
    NamedGroup::secp384r1,
    p384::ecdh::EphemeralSecret,
    p384::PublicKey
);

pub const ALL_KX_GROUPS: &[&dyn SupportedKxGroup] = &[&X25519, &SecP256R1, &SecP384R1];
