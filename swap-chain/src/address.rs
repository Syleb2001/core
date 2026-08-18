//! Chain-aware segwit v0 addresses, discriminated by their bech32 HRP.
//!
//! The swap protocol only ever deals with two script forms:
//! P2WPKH for user-facing addresses (redeem, refund, punish, withdraw)
//! and P2WSH for the shared 2-of-2 outputs.
//! Both are witness v0 programs, so an address is fully described by
//! `(chain, network, witness program)` and its string form tells us the
//! chain it belongs to (`bc1…` is Bitcoin, `ltc1…` is Litecoin).
//!
//! Parsing existing Bitcoin address strings yields `Chain::Bitcoin`,
//! which keeps this type backward compatible with every address string
//! already persisted or sent over the wire.

use crate::Chain;
use bitcoin::blockdata::script::witness_program::WitnessProgram;
use bitcoin::blockdata::script::witness_version::WitnessVersion;
use bitcoin::{Network, ScriptBuf};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;
use std::str::FromStr;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ChainAddress {
    chain: Chain,
    network: Network,
    program: WitnessProgram,
}

#[derive(Debug, thiserror::Error)]
pub enum ChainAddressError {
    #[error("failed to decode bech32 address: {0}")]
    Bech32(#[from] bech32::segwit::DecodeError),
    #[error("failed to encode bech32 address: {0}")]
    Encode(#[from] bech32::segwit::EncodeError),
    #[error("unknown address prefix {0:?}, expected one of bc, tb, bcrt, ltc, tltc, rltc")]
    UnknownHrp(String),
    #[error("only segwit v0 addresses are supported, got witness version {0}")]
    UnsupportedWitnessVersion(u8),
    #[error("invalid witness program: {0}")]
    InvalidProgram(#[from] bitcoin::blockdata::script::witness_program::Error),
    #[error("script is not a segwit witness program")]
    NotAWitnessProgram,
    #[error("expected a {expected} address but {address} is a {actual} address")]
    WrongChain {
        expected: Chain,
        actual: Chain,
        address: String,
    },
    #[error("expected an address for {expected:?} but {address} is for {actual:?}")]
    WrongNetwork {
        expected: Network,
        actual: Network,
        address: String,
    },
    #[error("expected a P2WPKH (bech32, 20 byte program) address, got {0}")]
    NotP2wpkh(String),
}

impl ChainAddress {
    pub fn from_witness_program(
        chain: Chain,
        network: Network,
        program: WitnessProgram,
    ) -> Result<Self, ChainAddressError> {
        if program.version() != WitnessVersion::V0 {
            return Err(ChainAddressError::UnsupportedWitnessVersion(
                program.version().to_num(),
            ));
        }

        Ok(Self {
            chain,
            network,
            program,
        })
    }

    pub fn from_script(
        chain: Chain,
        network: Network,
        script: &bitcoin::Script,
    ) -> Result<Self, ChainAddressError> {
        if !script.is_witness_program() {
            return Err(ChainAddressError::NotAWitnessProgram);
        }
        let Some(WitnessVersion::V0) = script.witness_version() else {
            let version = script
                .witness_version()
                .map(|v| v.to_num())
                .unwrap_or(u8::MAX);
            return Err(ChainAddressError::UnsupportedWitnessVersion(version));
        };

        // Skip the version opcode and the push length byte.
        let program = WitnessProgram::new(WitnessVersion::V0, &script.as_bytes()[2..])?;
        Self::from_witness_program(chain, network, program)
    }

    /// Parse an address string, inferring chain and network from the HRP.
    ///
    /// All testnet flavors (including signet on Bitcoin) share one HRP
    /// and parse as [`Network::Testnet`].
    pub fn parse(address: &str) -> Result<Self, ChainAddressError> {
        let (hrp, version, program) = bech32::segwit::decode(address)?;

        if version != bech32::Fe32::Q {
            return Err(ChainAddressError::UnsupportedWitnessVersion(
                version.to_u8(),
            ));
        }

        let (chain, network) = chain_and_network_from_hrp(hrp.as_str())
            .ok_or_else(|| ChainAddressError::UnknownHrp(hrp.as_str().to_string()))?;
        let program = WitnessProgram::new(WitnessVersion::V0, &program)?;

        Ok(Self {
            chain,
            network,
            program,
        })
    }

    /// Parse an address string and verify it belongs to the given
    /// chain and network, so a pasted Bitcoin address is rejected
    /// with a clear error on a Litecoin swap (and vice versa).
    pub fn parse_for(
        address: &str,
        chain: Chain,
        network: Network,
    ) -> Result<Self, ChainAddressError> {
        let parsed = Self::parse(address)?;

        if parsed.chain != chain {
            return Err(ChainAddressError::WrongChain {
                expected: chain,
                actual: parsed.chain,
                address: address.to_string(),
            });
        }
        if parsed.network != expected_shadow_network(network) {
            return Err(ChainAddressError::WrongNetwork {
                expected: network,
                actual: parsed.network,
                address: address.to_string(),
            });
        }

        Ok(parsed)
    }

    /// Wrap an existing rust-bitcoin address as a [`Chain::Bitcoin`] address.
    ///
    /// The bech32 string form carries the network, so this simply
    /// re-parses it; base58 (non-segwit) addresses are rejected.
    pub fn from_bitcoin_address(address: &bitcoin::Address) -> Result<Self, ChainAddressError> {
        Self::parse(&address.to_string())
    }

    pub fn chain(&self) -> Chain {
        self.chain
    }

    pub fn network(&self) -> Network {
        self.network
    }

    pub fn script_pubkey(&self) -> ScriptBuf {
        ScriptBuf::new_witness_program(&self.program)
    }

    /// The rust-bitcoin address this maps to on the internal
    /// ("shadow") network — same script, Bitcoin's HRP.
    /// Only for interacting with bdk and other rust-bitcoin APIs;
    /// never show this form to a user for a non-Bitcoin chain.
    pub fn to_shadow_address(&self) -> bitcoin::Address {
        bitcoin::Address::from_script(&self.script_pubkey(), self.network)
            .expect("a valid witness program always maps to an address")
    }

    pub fn is_p2wpkh(&self) -> bool {
        self.program.is_p2wpkh()
    }

    pub fn require_p2wpkh(self) -> Result<Self, ChainAddressError> {
        if !self.is_p2wpkh() {
            return Err(ChainAddressError::NotP2wpkh(self.to_string()));
        }

        Ok(self)
    }
}

impl fmt::Display for ChainAddress {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let hrp = bech32::Hrp::parse(self.chain.bech32_hrp(self.network))
            .expect("statically known HRPs are valid");
        let encoded =
            bech32::segwit::encode(hrp, bech32::Fe32::Q, self.program.program().as_bytes())
                .expect("valid witness program is encodable");

        f.write_str(&encoded)
    }
}

impl FromStr for ChainAddress {
    type Err = ChainAddressError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s)
    }
}

impl Serialize for ChainAddress {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for ChainAddress {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        s.parse().map_err(serde::de::Error::custom)
    }
}

fn chain_and_network_from_hrp(hrp: &str) -> Option<(Chain, Network)> {
    for chain in [Chain::Bitcoin, Chain::Litecoin] {
        let params = chain.params();
        if hrp == params.bech32_hrp_mainnet {
            return Some((chain, Network::Bitcoin));
        }
        if hrp == params.bech32_hrp_testnet {
            return Some((chain, Network::Testnet));
        }
        if hrp == params.bech32_hrp_regtest {
            return Some((chain, Network::Regtest));
        }
    }

    None
}

/// The canonical network an address parses to for a requested network:
/// all testnet flavors share one HRP, so they all parse as testnet.
fn expected_shadow_network(network: Network) -> Network {
    match network {
        Network::Bitcoin => Network::Bitcoin,
        Network::Regtest => Network::Regtest,
        _ => Network::Testnet,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p2wpkh_program() -> WitnessProgram {
        // The BIP-173 example program (hash160 of the well-known test pubkey).
        let bytes = [
            0x75, 0x1e, 0x76, 0xe8, 0x19, 0x91, 0x96, 0xd4, 0x54, 0x94, 0x1c, 0x45, 0xd1, 0xb3,
            0xa3, 0x23, 0xf1, 0x43, 0x3b, 0xd6,
        ];
        WitnessProgram::new(WitnessVersion::V0, &bytes).unwrap()
    }

    fn p2wsh_program() -> WitnessProgram {
        WitnessProgram::new(WitnessVersion::V0, &[0x42; 32]).unwrap()
    }

    #[test]
    fn display_parse_round_trip_all_hrps() {
        for chain in [Chain::Bitcoin, Chain::Litecoin] {
            for network in [Network::Bitcoin, Network::Testnet, Network::Regtest] {
                for program in [p2wpkh_program(), p2wsh_program()] {
                    let address =
                        ChainAddress::from_witness_program(chain, network, program).unwrap();
                    let parsed = ChainAddress::parse(&address.to_string()).unwrap();
                    assert_eq!(parsed, address);
                    assert_eq!(parsed.chain(), chain);
                    assert_eq!(parsed.network(), network);
                }
            }
        }
    }

    #[test]
    fn bitcoin_form_matches_rust_bitcoin() {
        let address =
            ChainAddress::from_witness_program(Chain::Bitcoin, Network::Bitcoin, p2wpkh_program())
                .unwrap();

        let via_rust_bitcoin = address.to_shadow_address().to_string();
        assert_eq!(address.to_string(), via_rust_bitcoin);
        assert_eq!(
            address.to_string(),
            "bc1qw508d6qejxtdg4y5r3zarvary0c5xw7kv8f3t4"
        );
    }

    #[test]
    fn litecoin_addresses_use_ltc_hrps() {
        let mainnet =
            ChainAddress::from_witness_program(Chain::Litecoin, Network::Bitcoin, p2wpkh_program())
                .unwrap();
        let testnet =
            ChainAddress::from_witness_program(Chain::Litecoin, Network::Testnet, p2wpkh_program())
                .unwrap();
        let regtest =
            ChainAddress::from_witness_program(Chain::Litecoin, Network::Regtest, p2wpkh_program())
                .unwrap();

        assert!(mainnet.to_string().starts_with("ltc1q"));
        assert!(testnet.to_string().starts_with("tltc1q"));
        assert!(regtest.to_string().starts_with("rltc1q"));
    }

    #[test]
    fn same_program_same_script_across_chains() {
        let btc =
            ChainAddress::from_witness_program(Chain::Bitcoin, Network::Bitcoin, p2wpkh_program())
                .unwrap();
        let ltc =
            ChainAddress::from_witness_program(Chain::Litecoin, Network::Bitcoin, p2wpkh_program())
                .unwrap();

        assert_eq!(btc.script_pubkey(), ltc.script_pubkey());
        assert_ne!(btc.to_string(), ltc.to_string());
    }

    #[test]
    fn parse_for_rejects_wrong_chain_and_network() {
        let ltc =
            ChainAddress::from_witness_program(Chain::Litecoin, Network::Bitcoin, p2wpkh_program())
                .unwrap()
                .to_string();

        let wrong_chain = ChainAddress::parse_for(&ltc, Chain::Bitcoin, Network::Bitcoin);
        assert!(matches!(
            wrong_chain,
            Err(ChainAddressError::WrongChain { .. })
        ));

        let wrong_network = ChainAddress::parse_for(&ltc, Chain::Litecoin, Network::Testnet);
        assert!(matches!(
            wrong_network,
            Err(ChainAddressError::WrongNetwork { .. })
        ));

        assert!(ChainAddress::parse_for(&ltc, Chain::Litecoin, Network::Bitcoin).is_ok());
    }

    #[test]
    fn rejects_non_v0_and_unknown_hrps() {
        // A taproot (v1) address must be rejected.
        let taproot = "bc1p5d7rjq7g6rdk2yhzks9smlaqtedr4dekq08ge8ztwac72sfr9rusxg3297";
        assert!(matches!(
            ChainAddress::parse(taproot),
            Err(ChainAddressError::UnsupportedWitnessVersion(1))
        ));

        let doge_style = bech32::segwit::encode(
            bech32::Hrp::parse("doge").unwrap(),
            bech32::Fe32::Q,
            p2wpkh_program().program().as_bytes(),
        )
        .unwrap();
        assert!(matches!(
            ChainAddress::parse(&doge_style),
            Err(ChainAddressError::UnknownHrp(_))
        ));
    }

    #[test]
    fn require_p2wpkh_rejects_p2wsh() {
        let p2wsh =
            ChainAddress::from_witness_program(Chain::Litecoin, Network::Bitcoin, p2wsh_program())
                .unwrap();
        assert!(p2wsh.require_p2wpkh().is_err());

        let p2wpkh =
            ChainAddress::from_witness_program(Chain::Litecoin, Network::Bitcoin, p2wpkh_program())
                .unwrap();
        assert!(p2wpkh.require_p2wpkh().is_ok());
    }

    #[test]
    fn serde_round_trips_as_string() {
        let address =
            ChainAddress::from_witness_program(Chain::Litecoin, Network::Bitcoin, p2wpkh_program())
                .unwrap();

        let json = serde_json::to_string(&address).unwrap();
        assert_eq!(json, format!("\"{}\"", address));

        let parsed: ChainAddress = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, address);
    }

    #[test]
    fn from_bitcoin_address_round_trip() {
        let shadow =
            ChainAddress::from_witness_program(Chain::Bitcoin, Network::Testnet, p2wpkh_program())
                .unwrap()
                .to_shadow_address();

        let wrapped = ChainAddress::from_bitcoin_address(&shadow).unwrap();
        assert_eq!(wrapped.to_string(), shadow.to_string());
        assert_eq!(wrapped.network(), Network::Testnet);
    }
}
