// Portal Hardware Wallet firmware and supporting software libraries
// 
// Copyright (C) 2024 Alekos Filini
// 
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
// 
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
// 
// You should have received a copy of the GNU General Public License
// along with this program.  If not, see <https://www.gnu.org/licenses/>.

use core::str::FromStr;

use alloc::string::ToString;
use futures::prelude::*;

use rand::RngCore;

use gui::{ConfirmPairCodePage, SingleLineTextPage};
use model::{
    Entropy, ExtendedKey, InitializedConfig, MultisigKey, ScriptType, UnlockedConfig,
    UnverifiedConfig, WalletDescriptor,
};

use bdk::bitcoin::util::bip32;
use bdk::bitcoin::Network;
use bdk::descriptor::{DescriptorXKey, IntoWalletDescriptor};
use bdk::keys::bip39::Mnemonic;
use bdk::keys::{
    DescriptorKey, DescriptorPublicKey, DescriptorSecretKey, ScriptContext, ValidNetworks,
};

use gui::{
    GeneratingMnemonicPage, ImportingMnemonicPage, LoadingPage, MnemonicPage, Page, WelcomePage,
};
use model::{Config, DeviceInfo};

use super::*;
use crate::config;
use crate::Error;

fn map_err_config<X>(_: X) -> config::ConfigError {
    config::ConfigError::CorruptedConfig
}

// Ignore the network check on each key: we fully control the network of our
// wallet, so it should always be coherent.
// This saves ~58KB !
struct SkipNetworkChecks(bdk::template::DescriptorTemplateOut);

impl IntoWalletDescriptor for SkipNetworkChecks {
    fn into_wallet_descriptor(
        self,
        _secp: &bdk::bitcoin::secp256k1::Secp256k1<bdk::bitcoin::secp256k1::All>,
        _network: Network,
    ) -> Result<
        (bdk::descriptor::ExtendedDescriptor, bdk::keys::KeyMap),
        bdk::descriptor::DescriptorError,
    > {
        Ok((self.0 .0, self.0 .1))
    }
}

fn build_bdk_descriptor(
    xprv: &bip32::ExtendedPrivKey,
    descriptor: model::WalletDescriptor,
    keychain: bdk::KeychainKind,
) -> Result<bdk::descriptor::template::DescriptorTemplateOut, Error> {
    fn extend_path(
        path: bip32::DerivationPath,
        keychain: bdk::KeychainKind,
    ) -> bip32::DerivationPath {
        let index = if keychain == bdk::KeychainKind::External {
            0
        } else {
            1
        };

        path.extend(&[bip32::ChildNumber::Normal { index }])
    }

    fn make_local_key<Ctx: ScriptContext>(
        derivation_path: bip32::DerivationPath,
        xprv: &bip32::ExtendedPrivKey,
        keychain: bdk::KeychainKind,
    ) -> DescriptorKey<Ctx> {
        let secp = secp256k1::Secp256k1::new();

        let split_position = derivation_path
            .into_iter()
            .rev()
            .take_while(|c| c.is_normal())
            .count();
        let origin_path = derivation_path[..split_position].into();
        let derivation_path = derivation_path[split_position..].into();

        bdk::keys::DescriptorKey::from_secret(
            DescriptorSecretKey::XPrv(DescriptorXKey {
                origin: Some((xprv.fingerprint(&secp), origin_path)),
                xkey: *xprv,
                derivation_path: extend_path(derivation_path, keychain),
                wildcard: bdk::descriptor::Wildcard::Unhardened,
            }),
            ValidNetworks::new(),
        )
    }

    match (descriptor.variant, descriptor.script_type) {
        (model::DescriptorVariant::SingleSig(path), ScriptType::NativeSegwit) => Ok(
            bdk::descriptor!(wpkh(make_local_key(path.into(), xprv, keychain)))?,
        ),
        (model::DescriptorVariant::SingleSig(path), ScriptType::WrappedSegwit) => Ok(
            bdk::descriptor!(sh(wpkh(make_local_key(path.into(), xprv, keychain))))?,
        ),
        (model::DescriptorVariant::SingleSig(path), ScriptType::Legacy) => Ok(bdk::descriptor!(
            pkh(make_local_key(path.into(), xprv, keychain))
        )?),

        (
            model::DescriptorVariant::MultiSig {
                threshold,
                keys,
                is_sorted,
            },
            script_type,
        ) => {
            fn get_keys_vector<Ctx: ScriptContext>(
                keys: alloc::vec::Vec<MultisigKey>,
                xprv: &bip32::ExtendedPrivKey,
                keychain: bdk::KeychainKind,
            ) -> alloc::vec::Vec<DescriptorKey<Ctx>> {
                keys.into_iter()
                    .map(|key| match key {
                        MultisigKey::Local(path) => {
                            make_local_key(path.clone().into(), xprv, keychain)
                        }
                        MultisigKey::External(ExtendedKey { origin, key, path }) => {
                            bdk::keys::DescriptorKey::from_public(
                                DescriptorPublicKey::XPub(DescriptorXKey {
                                    origin: origin.map(|(fingerprint, path)| {
                                        (fingerprint.into(), path.into())
                                    }),
                                    xkey: key
                                        .as_xpub()
                                        .expect("The key was checked when setting the config"),
                                    derivation_path: extend_path(path.into(), keychain),
                                    wildcard: bdk::descriptor::Wildcard::Unhardened,
                                }),
                                ValidNetworks::new(),
                            )
                        }
                    })
                    .collect()
            }

            // Unfortunately we have to duplicate this piece of code because we can't create a fragment for a "sortedmulti"
            if is_sorted {
                let keys = get_keys_vector(keys, xprv, keychain);

                match script_type {
                    ScriptType::NativeSegwit => {
                        Ok(bdk::descriptor!(wsh(sortedmulti_vec(threshold, keys)))?)
                    }
                    ScriptType::WrappedSegwit => {
                        Ok(bdk::descriptor!(sh(wsh(sortedmulti_vec(threshold, keys))))?)
                    }
                    ScriptType::Legacy => Err(Error::Config(config::ConfigError::CorruptedConfig)),
                }
            } else {
                return Err(Error::Wallet);

                // This adds way too much size to the binary, it needs to be investigated further...

                // match script_type {
                //     ScriptType::NativeSegwit => Ok(bdk::descriptor!(wsh(multi_vec(
                //         threshold,
                //         get_keys_vector(keys, xprv, keychain)
                //     )))?),
                //     ScriptType::WrappedSegwit => Ok(bdk::descriptor!(sh(wsh(multi_vec(
                //         threshold,
                //         get_keys_vector(keys, xprv, keychain)
                //     ))))?),
                //     ScriptType::Legacy => Ok(bdk::descriptor!(sh(multi_vec(
                //         threshold,
                //         get_keys_vector(keys, xprv, keychain)
                //     )))?),
                // }
            }
        }
    }
}

pub(super) fn make_wallet_from_xprv(
    xprv: bip32::ExtendedPrivKey,
    network: Network,
    config: model::UnlockedConfig,
) -> Result<PortalWallet, Error> {
    let descriptor_external = SkipNetworkChecks(build_bdk_descriptor(
        &xprv,
        config.secret.descriptor.clone(),
        bdk::KeychainKind::External,
    )?);
    let descriptor_internal = SkipNetworkChecks(build_bdk_descriptor(
        &xprv,
        config.secret.descriptor.clone(),
        bdk::KeychainKind::Internal,
    )?);

    let wallet = bdk::Wallet::new(descriptor_external, Some(descriptor_internal), (), network)?;

    Ok(PortalWallet::new(wallet, xprv, config))
}

pub async fn handle_por(peripherals: &mut HandlerPeripherals) -> Result<CurrentState, Error> {
    let page = LoadingPage::new();
    page.init_display(&mut peripherals.display)?;
    page.draw_to(&mut peripherals.display)?;
    peripherals.display.flush()?;

    let config = match config::read_config(&mut peripherals.flash).await {
        Ok(config) => config,
        Err(e) => {
            log::warn!("Config error: {:?}", e);
            return Ok(CurrentState::Init);
        }
    };
    match config {
        Config::Initialized(InitializedConfig {
            secret: model::MaybeEncrypted::Unencrypted(secret),
            network,
            ..
        }) => {
            log::debug!("Unencrypted config loaded");

            let xprv = secret.cached_xprv.as_xprv().map_err(map_err_config)?;
            Ok(CurrentState::Idle {
                wallet: Rc::new(make_wallet_from_xprv(
                    xprv,
                    network,
                    UnlockedConfig::from_secret_data_unencrypted(secret, network),
                )?),
            })
        }
        Config::Initialized(
            initialized @ InitializedConfig {
                secret: model::MaybeEncrypted::Encrypted { .. },
                ..
            },
        ) => Ok(CurrentState::Locked {
            config: initialized,
        }),
        Config::Unverified(unverified) => Ok(CurrentState::UnverifiedConfig { config: unverified }),
    }
}

#[cfg(feature = "device")]
fn read_serial() -> alloc::string::String {
    const OPTION_BYTES: usize = 0x1FFF_7000;
    const SERIAL_OFFSET: usize = 4;
    const SERIAL_LEN: usize = 20;

    let mut buf = [0u8; SERIAL_LEN];
    let mut address = (OPTION_BYTES + SERIAL_OFFSET) as *const u8;

    for i in 0..SERIAL_LEN {
        unsafe {
            buf[i] = core::ptr::read(address);
            address = address.add(1);
        }
    }

    if buf[0] == 0xFF || buf[0] == 0x00 {
        alloc::string::String::from("NO_SERIAL")
    } else {
        buf.into_iter()
            .take_while(|v| *v != 0x00)
            .map(|v| v as char)
            .collect()
    }
}
#[cfg(feature = "emulator")]
fn read_serial() -> alloc::string::String {
    Default::default()
}

pub async fn handle_init(
    mut events: impl Stream<Item = Event> + Unpin,
    peripherals: &mut HandlerPeripherals,
) -> Result<CurrentState, Error> {
    let serial = read_serial();

    let page = WelcomePage::new(&serial);
    page.init_display(&mut peripherals.display)?;
    page.draw_to(&mut peripherals.display)?;
    peripherals.display.flush()?;

    let events = only_requests(&mut events);
    pin_mut!(events);

    loop {
        match events.next().await {
            Some(model::Request::GetInfo) => {
                peripherals
                    .nfc
                    .send(model::Reply::Info(DeviceInfo::new_locked_uninitialized(env!("CARGO_PKG_VERSION"))))
                    .await
                    .unwrap();
                peripherals.nfc_finished.recv().await.unwrap();
                continue;
            }
            Some(model::Request::GenerateMnemonic {
                num_words,
                network,
                password,
            }) => {
                break Ok(CurrentState::GenerateSeed {
                    num_words,
                    network,
                    password,
                });
            }
            Some(model::Request::SetMnemonic {
                mnemonic,
                network,
                password,
            }) => {
                break Ok(CurrentState::ImportSeed {
                    mnemonic,
                    network,
                    password,
                });
            }
            #[cfg(feature = "emulator")]
            Some(model::Request::BeginFwUpdate(header)) => {
                break Ok(CurrentState::UpdatingFw { header });
            }
            Some(_) => {
                peripherals
                    .nfc
                    .send(model::Reply::UnexpectedMessage)
                    .await
                    .unwrap();
                peripherals.nfc_finished.recv().await.unwrap();
                continue;
            }
            _ => unreachable!(),
        }
    }
}

pub async fn handle_locked(
    config: InitializedConfig,
    mut events: impl Stream<Item = Event> + Unpin,
    peripherals: &mut HandlerPeripherals,
) -> Result<CurrentState, Error> {
    let page = SingleLineTextPage::new("LOCKED");
    page.init_display(&mut peripherals.display)?;
    page.draw_to(&mut peripherals.display)?;
    peripherals.display.flush()?;

    let events = only_requests(&mut events);
    pin_mut!(events);

    loop {
        match events.next().await {
            Some(model::Request::GetInfo) => {
                peripherals
                    .nfc
                    .send(model::Reply::Info(DeviceInfo::new_locked_initialized(
                        config.network,
                        env!("CARGO_PKG_VERSION")
                    )))
                    .await
                    .unwrap();
                peripherals.nfc_finished.recv().await.unwrap();
                continue;
            }
            Some(model::Request::Unlock { password }) => {
                if !config.pair_code.check(&password) {
                    peripherals
                        .nfc
                        .send(model::Reply::WrongPassword)
                        .await
                        .unwrap();
                    peripherals.nfc_finished.recv().await.unwrap();
                    continue;
                }

                let page = LoadingPage::new();
                page.init_display(&mut peripherals.display)?;
                page.draw_to(&mut peripherals.display)?;
                peripherals.display.flush()?;

                let unlocked = config
                    .unlock(&password)
                    .map_err(|_| Error::Config(config::ConfigError::CorruptedConfig))?;
                let xprv = unlocked
                    .secret
                    .cached_xprv
                    .as_xprv()
                    .map_err(map_err_config)?;
                peripherals.nfc.send(model::Reply::Ok).await.unwrap();

                break Ok(CurrentState::Idle {
                    wallet: Rc::new(make_wallet_from_xprv(xprv, unlocked.network, unlocked)?),
                });
            }
            Some(_) => {
                peripherals.nfc.send(model::Reply::Locked).await.unwrap();
                peripherals.nfc_finished.recv().await.unwrap();
                continue;
            }
            _ => unreachable!(),
        }
    }
}

pub async fn display_mnemonic(
    config: UnverifiedConfig,
    mut events: impl Stream<Item = Event> + Unpin,
    peripherals: &mut HandlerPeripherals,
) -> Result<CurrentState, Error> {
    peripherals.tsc_enabled.enable();

    let mnemonic = Mnemonic::from_entropy(&config.entropy.bytes).map_err(map_err_config)?;
    let mnemonic_str = mnemonic.word_iter().collect::<alloc::vec::Vec<_>>();
    for (chunk_index, words) in mnemonic_str.chunks(2).enumerate() {
        let mut page = MnemonicPage::new((chunk_index * 2) as u8, &words);
        page.init_display(&mut peripherals.display)?;
        page.draw_to(&mut peripherals.display)?;
        peripherals.display.flush()?;

        manage_confirmation_loop(&mut events, peripherals, &mut page).await?;

        // TODO: store checkpoint?
    }

    if let Some(pair_code) = &config.pair_code {
        let mut page = ConfirmPairCodePage::new(pair_code);
        page.init_display(&mut peripherals.display)?;
        page.draw_to(&mut peripherals.display)?;
        peripherals.display.flush()?;

        manage_confirmation_loop(&mut events, peripherals, &mut page).await?;
    }

    // TODO: show loading screen here

    let mut salt = [0; 8];
    peripherals.rng.fill_bytes(&mut salt);

    let network = config.network;
    let (initialized, unlocked, xprv) = config.upgrade(salt);
    config::write_config(&mut peripherals.flash, &Config::Initialized(initialized)).await?;

    peripherals.nfc.send(model::Reply::Ok).await.unwrap();
    peripherals.nfc_finished.recv().await.unwrap();

    Ok(CurrentState::Idle {
        wallet: Rc::new(make_wallet_from_xprv(xprv, network, unlocked)?),
    })
}

async fn save_unverified_config(
    unverified_config: UnverifiedConfig,
    peripherals: &mut HandlerPeripherals,
) -> Result<UnverifiedConfig, Error> {
    let config = Config::Unverified(unverified_config);
    config::write_config(&mut peripherals.flash, &config).await?;
    let unverified_config = match config {
        Config::Unverified(c) => c,
        _ => unreachable!(),
    };

    Ok(unverified_config)
}

pub async fn handle_generate_seed(
    num_words: model::NumWordsMnemonic,
    network: Network,
    password: Option<&str>,
    events: impl Stream<Item = Event> + Unpin,
    peripherals: &mut HandlerPeripherals,
) -> Result<CurrentState, Error> {
    let page = GeneratingMnemonicPage::new(num_words);
    page.init_display(&mut peripherals.display)?;
    page.draw_to(&mut peripherals.display)?;
    peripherals.display.flush()?;

    let mut entropy = [0u8; 32];
    let entropy = match num_words {
        model::NumWordsMnemonic::Words12 => &mut entropy[..16],
        model::NumWordsMnemonic::Words24 => &mut entropy[..32],
    };
    rand_chacha::rand_core::RngCore::fill_bytes(&mut peripherals.rng, entropy);

    let descriptor = WalletDescriptor::make_bip84(network);

    let unverified_config = UnverifiedConfig {
        entropy: Entropy {
            bytes: alloc::vec::Vec::from(entropy).into(),
        },
        network,
        pair_code: password.map(ToString::to_string),
        descriptor,
    };
    let unverified_config = save_unverified_config(unverified_config, peripherals).await?;
    display_mnemonic(unverified_config, events, peripherals).await
}

pub async fn handle_import_seed(
    mnemonic: &str,
    network: Network,
    password: Option<&str>,
    events: impl Stream<Item = Event> + Unpin,
    peripherals: &mut HandlerPeripherals,
) -> Result<CurrentState, Error> {
    let page = ImportingMnemonicPage::new();
    page.init_display(&mut peripherals.display)?;
    page.draw_to(&mut peripherals.display)?;
    peripherals.display.flush()?;

    let mnemonic = Mnemonic::from_str(mnemonic).map_err(map_err_config)?;
    let (entropy, len) = mnemonic.to_entropy_array();
    let entropy = &entropy[..len];

    let descriptor = WalletDescriptor::make_bip84(network);

    let unverified_config = UnverifiedConfig {
        entropy: Entropy {
            bytes: alloc::vec::Vec::from(entropy).into(),
        },
        network,
        pair_code: password.map(ToString::to_string),
        descriptor,
    };
    let unverified_config = save_unverified_config(unverified_config, peripherals).await?;
    display_mnemonic(unverified_config, events, peripherals).await
}

pub async fn handle_unverified_config(
    config: UnverifiedConfig,
    mut events: impl Stream<Item = Event> + Unpin,
    peripherals: &mut HandlerPeripherals,
) -> Result<CurrentState, Error> {
    let page = LoadingPage::new();
    page.init_display(&mut peripherals.display)?;
    page.draw_to(&mut peripherals.display)?;
    peripherals.display.flush()?;

    {
        let req_events = only_requests(&mut events);
        pin_mut!(req_events);

        loop {
            match req_events.next().await {
                Some(model::Request::GetInfo) => {
                    peripherals
                        .nfc
                        .send(model::Reply::Info(DeviceInfo::new_unverified_config(
                            config.network,
                            config.pair_code.is_some(),
                            env!("CARGO_PKG_VERSION")
                        )))
                        .await
                        .unwrap();
                    peripherals.nfc_finished.recv().await.unwrap();
                    continue;
                }
                Some(model::Request::Resume) => {
                    peripherals
                        .nfc
                        .send(model::Reply::DelayedReply)
                        .await
                        .unwrap();
                    break;
                }
                Some(_) => {
                    peripherals
                        .nfc
                        .send(model::Reply::Unverified)
                        .await
                        .unwrap();
                    peripherals.nfc_finished.recv().await.unwrap();
                    continue;
                }
                _ => unreachable!(),
            }
        }
    }

    display_mnemonic(config, events, peripherals).await
}
