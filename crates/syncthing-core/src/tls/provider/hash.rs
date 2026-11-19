//! SHA-256 and SHA-384 hash providers for `rustls`.

use digest::{Digest, OutputSizeUser};
use rustls::crypto::hash::{self, Context, Hash, HashAlgorithm, Output};
use sha2::{Sha256, Sha384};

macro_rules! impl_hash {
    ($const_name:ident, $provider_ty:ident, $ctx_ty:ident, $ty:ty, $algo:expr) => {
        struct $provider_ty;

        impl Hash for $provider_ty {
            fn start(&self) -> Box<dyn Context> {
                Box::new($ctx_ty(<$ty>::new()))
            }

            fn hash(&self, data: &[u8]) -> Output {
                Output::new(&<$ty>::digest(data)[..])
            }

            fn output_len(&self) -> usize {
                <$ty as OutputSizeUser>::output_size()
            }

            fn algorithm(&self) -> HashAlgorithm {
                $algo
            }
        }

        struct $ctx_ty($ty);

        impl Context for $ctx_ty {
            fn fork_finish(&self) -> Output {
                Output::new(&self.0.clone().finalize()[..])
            }

            fn fork(&self) -> Box<dyn Context> {
                Box::new($ctx_ty(self.0.clone()))
            }

            fn finish(self: Box<Self>) -> Output {
                Output::new(&self.0.finalize()[..])
            }

            fn update(&mut self, data: &[u8]) {
                self.0.update(data);
            }
        }

        pub const $const_name: &dyn hash::Hash = &$provider_ty;
    };
}

impl_hash!(
    SHA256,
    HashSha256,
    HashCtxSha256,
    Sha256,
    HashAlgorithm::SHA256
);
impl_hash!(
    SHA384,
    HashSha384,
    HashCtxSha384,
    Sha384,
    HashAlgorithm::SHA384
);
