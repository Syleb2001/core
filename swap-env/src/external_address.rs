use std::fmt;
use std::str::FromStr;

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use swap_chain::{Chain, ChainAddress};

/// An address destined for the configured script chain, as the user
/// wrote it.
///
/// The original string is kept so the config file round-trips
/// unchanged. Bech32 (segwit v0) addresses are understood for every
/// supported chain, discriminated by their HRP; anything else falls
/// back to the historical Bitcoin parser so legacy base58 and taproot
/// addresses keep working for a Bitcoin instance.
///
/// The address is only tied to a chain once [`Self::to_shadow`] is
/// called with the configured chain and network.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExternalAddress {
    raw: String,
    parsed: Parsed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Parsed {
    Chain(ChainAddress),
    Bitcoin(bitcoin::Address<bitcoin::address::NetworkUnchecked>),
}

impl ExternalAddress {
    /// Validate the address against the configured chain and network
    /// and return the shadow address the wallet and the state machines
    /// operate on.
    pub fn to_shadow(&self, chain: Chain, network: bitcoin::Network) -> Result<bitcoin::Address> {
        match &self.parsed {
            Parsed::Chain(_) => {
                let address = ChainAddress::parse_for(&self.raw, chain, network)?;
                Ok(address.to_shadow_address())
            }
            Parsed::Bitcoin(address) => {
                if chain != Chain::Bitcoin {
                    bail!(
                        "Address {} is a Bitcoin address but this instance is configured for {chain}",
                        self.raw
                    );
                }
                address.clone().require_network(network).with_context(|| {
                    format!("Address {} is not valid on the {network} network", self.raw)
                })
            }
        }
    }
}

impl FromStr for ExternalAddress {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self> {
        let parsed = match ChainAddress::parse(s) {
            Ok(address) => Parsed::Chain(address),
            Err(segwit_error) => match bitcoin::Address::from_str(s) {
                Ok(address) => Parsed::Bitcoin(address),
                Err(bitcoin_error) => bail!(
                    "Failed to parse address {s}: not a segwit address of a supported chain ({segwit_error}) nor any other Bitcoin address form ({bitcoin_error})"
                ),
            },
        };

        Ok(Self {
            raw: s.to_owned(),
            parsed,
        })
    }
}

impl fmt::Display for ExternalAddress {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.raw)
    }
}

impl Serialize for ExternalAddress {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.raw.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for ExternalAddress {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        String::deserialize(deserializer)?
            .parse()
            .map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BTC_BECH32: &str = "bc1qw508d6qejxtdg4y5r3zarvary0c5xw7kv8f3t4";
    const BTC_BASE58: &str = "1KFHE7w8BhaENAswwryaoccDb6qcT6DbYY";
    const LTC_BECH32: &str = "ltc1qw508d6qejxtdg4y5r3zarvary0c5xw7kgmn4n9";

    #[test]
    fn bitcoin_addresses_resolve_for_a_bitcoin_instance() {
        for raw in [BTC_BECH32, BTC_BASE58] {
            let address: ExternalAddress = raw.parse().unwrap();
            let shadow = address
                .to_shadow(Chain::Bitcoin, bitcoin::Network::Bitcoin)
                .unwrap();
            assert_eq!(shadow.to_string(), raw);
            assert!(
                address
                    .to_shadow(Chain::Litecoin, bitcoin::Network::Bitcoin)
                    .is_err()
            );
        }
    }

    #[test]
    fn litecoin_address_resolves_to_its_shadow_form() {
        let address: ExternalAddress = LTC_BECH32.parse().unwrap();
        let shadow = address
            .to_shadow(Chain::Litecoin, bitcoin::Network::Bitcoin)
            .unwrap();
        assert_eq!(shadow.to_string(), BTC_BECH32);
        assert!(
            address
                .to_shadow(Chain::Bitcoin, bitcoin::Network::Bitcoin)
                .is_err()
        );
    }

    #[test]
    fn wrong_network_is_rejected() {
        let address: ExternalAddress = LTC_BECH32.parse().unwrap();
        assert!(
            address
                .to_shadow(Chain::Litecoin, bitcoin::Network::Testnet)
                .is_err()
        );
    }

    #[test]
    fn display_and_serde_keep_the_original_form() {
        for raw in [BTC_BECH32, BTC_BASE58, LTC_BECH32] {
            let address: ExternalAddress = raw.parse().unwrap();
            assert_eq!(address.to_string(), raw);

            let json = serde_json::to_string(&address).unwrap();
            assert_eq!(json, format!("\"{raw}\""));
            let back: ExternalAddress = serde_json::from_str(&json).unwrap();
            assert_eq!(back, address);
        }
    }

    #[test]
    fn garbage_is_rejected() {
        assert!("not-an-address".parse::<ExternalAddress>().is_err());
    }
}
