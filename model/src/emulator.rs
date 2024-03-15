// Portal Hardware Wallet model library
// 
// Copyright (c) 2024 Alekos Filini
//
// This file is licensed under the Apache License, Version 2.0 <LICENSE-APACHE
// or http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your option.
// You may not use this file except in accordance with one or both of these
// licenses.

use noise_protocol::CipherState;

#[derive(Debug)]
pub enum CardMessage {
    Display(alloc::vec::Vec<u16>),
    Nfc(alloc::vec::Vec<u8>),
    WriteFlash(alloc::vec::Vec<u8>),
    ReadFlash,
    Tick,
    FinishBoot,
    FlushDisplay,
}

#[cfg(feature = "stm32")]
impl CardMessage {
    pub fn write_to(self) -> alloc::boxed::Box<dyn Iterator<Item = u8>> {
        match self {
            CardMessage::Display(pixels) => alloc::boxed::Box::new(
                [0x00]
                    .into_iter()
                    .chain(u16::to_be_bytes(pixels.len() as u16 * 2).into_iter())
                    .chain(
                        pixels
                            .into_iter()
                            .map(|v| [((v & 0xFF00) >> 8) as u8, (v & 0xFF) as u8])
                            .flatten(),
                    ),
            ),
            CardMessage::Nfc(reply) => alloc::boxed::Box::new(
                [0x01]
                    .into_iter()
                    .chain(u16::to_be_bytes(reply.len() as _).into_iter())
                    .chain(reply.into_iter()),
            ),
            CardMessage::Tick => alloc::boxed::Box::new([0x02].into_iter()),
            CardMessage::WriteFlash(data) => alloc::boxed::Box::new(
                [0x03]
                    .into_iter()
                    .chain(u16::to_be_bytes(data.len() as _).into_iter())
                    .chain(data.into_iter()),
            ),
            CardMessage::ReadFlash => alloc::boxed::Box::new([0x04].into_iter()),
            CardMessage::FinishBoot => alloc::boxed::Box::new([0x05].into_iter()),
            CardMessage::FlushDisplay => alloc::boxed::Box::new([0x06].into_iter()),
        }
    }
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub enum EmulatorMessage {
    Tsc(bool),
    Nfc(alloc::vec::Vec<u8>),
    FlashContent(alloc::vec::Vec<u8>),
    Reset,
}

impl EmulatorMessage {
    pub fn from_request<C: noise_protocol::Cipher>(
        req: &super::Request,
        cipher: &mut CipherState<C>,
    ) -> Self {
        let msg = crate::Message::new_serialize(req, cipher).unwrap();
        EmulatorMessage::Nfc(msg.data().to_vec())
    }

    pub fn encode(&self) -> alloc::vec::Vec<u8> {
        match self {
            EmulatorMessage::Tsc(v) => {
                alloc::vec![0x01, 0x00, 0x01, if *v { 0x01 } else { 0x00 }]
            }
            EmulatorMessage::Nfc(req) => {
                let mut v = alloc::vec![0x02];
                v.extend_from_slice(&u16::to_be_bytes(req.len() as u16));
                v.extend_from_slice(&req);
                v
            }
            EmulatorMessage::FlashContent(data) => {
                let mut v = alloc::vec![0x03];
                v.extend_from_slice(&u16::to_be_bytes(data.len() as u16));
                v.extend_from_slice(&data);
                v
            }
            EmulatorMessage::Reset => {
                alloc::vec![0x04]
            }
        }
    }

    pub fn to_string(&self) -> alloc::string::String {
        #[allow(unused_imports)]
        use alloc::string::ToString;

        match self {
            EmulatorMessage::Tsc(v) => alloc::format!("Tsc({})", v),
            EmulatorMessage::Reset => "Reset".to_string(),
            EmulatorMessage::Nfc(bytes) => alloc::format!("Nfc({:02X?})", bytes),
            EmulatorMessage::FlashContent(_) => "FlashContent(...)".to_string(),
        }
    }
}
