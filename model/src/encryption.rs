// Portal Hardware Wallet model library
// 
// Copyright (c) 2024 Alekos Filini
//
// This file is licensed under the Apache License, Version 2.0 <LICENSE-APACHE
// or http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your option.
// You may not use this file except in accordance with one or both of these
// licenses.

use core::ops::Deref;

use bitcoin::hashes::{sha256, Hash, HashEngine};
use bitcoin::secp256k1::{ecdh::SharedSecret, PublicKey, Secp256k1, SecretKey, SignOnly};

pub use noise_rust_crypto::sensitive::Sensitive;
use noise_rust_crypto::Aes256Gcm;

pub const NOISE_PROLOGUE: &'static [u8] = b"nfc-hardware-signer";

pub struct SecpDH;

impl noise_protocol::DH for SecpDH {
    type Key = Sensitive<[u8; 32]>;
    type Pubkey = [u8; 64];
    type Output = Sensitive<[u8; 32]>;

    fn name() -> &'static str {
        "Secp256k1 Diffie-Hellman"
    }
    fn genkey() -> Self::Key {
        panic!("No RNG available")
    }
    fn pubkey(seckey: &Self::Key) -> Self::Pubkey {
        let seckey = SecretKey::from_slice(seckey.deref()).expect("Valid secret key");
        let mut pubkey = [0; 64];
        (pubkey[..33]).copy_from_slice(
            &seckey
                .public_key::<SignOnly>(&Secp256k1::gen_new())
                .serialize(),
        );

        pubkey
    }
    fn dh(seckey: &Self::Key, pubkey: &Self::Pubkey) -> Result<Self::Output, ()> {
        let seckey = SecretKey::from_slice(seckey.deref()).expect("Valid secret key");
        let pubkey = PublicKey::from_slice(&pubkey[..33]).map_err(|_| ())?;

        let secret_bytes = SharedSecret::new(&pubkey, &seckey).secret_bytes();
        Ok(Sensitive::from(From::from(secret_bytes)))
    }
}

#[derive(Default)]
pub struct BitcoinHashesSha256(sha256::HashEngine);

impl noise_protocol::Hash for BitcoinHashesSha256 {
    type Block = [u8; 64];
    type Output = Sensitive<[u8; 32]>;

    fn name() -> &'static str {
        "bitcoin-hashes SHA256"
    }
    fn input(&mut self, data: &[u8]) {
        self.0.input(data)
    }
    fn result(&mut self) -> Self::Output {
        let hash = sha256::Hash::from_engine(self.0.clone()).into_inner();
        Sensitive::from(From::from(hash))
    }
}

#[inline]
pub fn wrap_sensitive(bytes: [u8; 32]) -> Sensitive<[u8; 32]> {
    Sensitive::from(From::from(bytes))
}

pub type CipherState = noise_protocol::CipherState<Aes256Gcm>;
pub type HandshakeState = noise_protocol::HandshakeState<SecpDH, Aes256Gcm, BitcoinHashesSha256>;

pub fn handhake_state_initiator(ephemeral_key: Sensitive<[u8; 32]>) -> HandshakeState {
    HandshakeState::new(
        noise_protocol::patterns::noise_nn(),
        true,
        NOISE_PROLOGUE,
        None,
        Some(ephemeral_key),
        None,
        None,
    )
}
pub fn handhake_state_responder(ephemeral_key: Sensitive<[u8; 32]>) -> HandshakeState {
    HandshakeState::new(
        noise_protocol::patterns::noise_nn(),
        false,
        NOISE_PROLOGUE,
        None,
        Some(ephemeral_key),
        None,
        None,
    )
}
