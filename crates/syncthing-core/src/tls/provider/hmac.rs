//! HMAC-SHA256 and HMAC-SHA384 providers for `rustls`.

use digest::OutputSizeUser;
use hmac::{KeyInit, Mac};
use rustls::crypto::hmac::{Hmac, Key, Tag};
use sha2::{Sha256, Sha384};

macro_rules! impl_hmac {
    ($const_name:ident, $provider_ty:ident, $key_ty:ident, $ty:ty) => {
        struct $provider_ty;

        impl Hmac for $provider_ty {
            fn with_key(&self, key: &[u8]) -> Box<dyn Key> {
                Box::new($key_ty(
                    hmac::Hmac::<$ty>::new_from_slice(key)
                        .expect("HMAC accepts keys of any length"),
                ))
            }

            fn hash_output_len(&self) -> usize {
                <$ty as OutputSizeUser>::output_size()
            }
        }

        struct $key_ty(hmac::Hmac<$ty>);

        impl Key for $key_ty {
            fn sign_concat(&self, first: &[u8], middle: &[&[u8]], last: &[u8]) -> Tag {
                let mut ctx = self.0.clone();
                ctx.update(first);
                for m in middle {
                    ctx.update(m);
                }
                ctx.update(last);
                Tag::new(&ctx.finalize().into_bytes()[..])
            }

            fn tag_len(&self) -> usize {
                <$ty as OutputSizeUser>::output_size()
            }
        }

        pub const $const_name: &dyn Hmac = &$provider_ty;
    };
}

impl_hmac!(SHA256, HmacSha256, HmacKeySha256, Sha256);
impl_hmac!(SHA384, HmacSha384, HmacKeySha384, Sha384);
