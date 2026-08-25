//! Identity and intrinsic parameters of the "script chain" of a swap —
//! the chain Bob locks with the Lock/Cancel/Redeem/Punish transaction family
//! (Bitcoin today, Litecoin as a second pair).
//!
//! This crate only describes what a chain *is*: tickers, address encoding,
//! derivation coin type and public endpoints.
//! Consensus-timing values (timelocks, finality confirmations, block times)
//! are environment configuration and stay in `swap_env::env::Config`.
//!
//! Consensus data structures (transactions, scripts, PSBTs, amounts) are
//! byte-identical between Bitcoin and Litecoin, so the rust-bitcoin types are
//! used for both chains everywhere. A non-Bitcoin chain runs on the
//! corresponding `bitcoin::Network` internally ("shadow network") and only
//! differs at the boundaries described by [`ChainParams`].

pub mod address;

pub use address::{ChainAddress, ChainAddressError};

use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

/// The script chain a swap (or a whole ASB process) operates on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Chain {
    Bitcoin,
    Litecoin,
}

/// Parameters intrinsic to a script chain, independent of which
/// network (mainnet/testnet/regtest) is in use.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChainParams {
    pub chain: Chain,
    pub ticker: &'static str,
    /// SLIP-44 coin type for the mainnet wallet derivation path
    /// (all chains share coin type 1 on testnets).
    pub slip44_coin_type: u32,
    pub bech32_hrp_mainnet: &'static str,
    pub bech32_hrp_testnet: &'static str,
    pub bech32_hrp_regtest: &'static str,
    /// Base URL of a mempool.space-compatible fee estimation API,
    /// if one exists for this chain.
    pub fee_api_base_url: Option<&'static str>,
    /// Base URL of the block explorer used for user-facing transaction links.
    pub explorer_base_url: &'static str,
    /// Genesis block hashes in display order, as the nodes print them.
    pub genesis_mainnet: &'static str,
    pub genesis_testnet: &'static str,
    pub genesis_regtest: &'static str,
}

pub const BITCOIN: ChainParams = ChainParams {
    chain: Chain::Bitcoin,
    ticker: "BTC",
    slip44_coin_type: 0,
    bech32_hrp_mainnet: "bc",
    bech32_hrp_testnet: "tb",
    bech32_hrp_regtest: "bcrt",
    fee_api_base_url: Some("https://mempool.space"),
    explorer_base_url: "https://mempool.space",
    genesis_mainnet: "000000000019d6689c085ae165831e934ff763ae46a2a6c172b3f1b60a8ce26f",
    genesis_testnet: "000000000933ea01ad0ee984209779baaec3ced90fa3f408719526f8d77f4943",
    genesis_regtest: "0f9188f13cb7b2c71f2a335e3a4fc328bf5beb436012afca590b1a11466e2206",
};

pub const LITECOIN: ChainParams = ChainParams {
    chain: Chain::Litecoin,
    ticker: "LTC",
    slip44_coin_type: 2,
    bech32_hrp_mainnet: "ltc",
    bech32_hrp_testnet: "tltc",
    bech32_hrp_regtest: "rltc",
    // litecoinspace.org runs the mempool.space codebase with the same API.
    fee_api_base_url: Some("https://litecoinspace.org"),
    explorer_base_url: "https://litecoinspace.org",
    // From litecoin's chainparams.cpp assertions (testnet is testnet4)
    genesis_mainnet: "12a765e31ffd4059bada1e25190f6e98c99d9714d334efa41a195a7e7e04bfe2",
    genesis_testnet: "4966625a4b2851d9fdee139e56211a0d88575f59ed816ff5e6a63deb4e3e29a0",
    genesis_regtest: "530827f38f93b43ed12af0b3ad25a288dc02ed74d6d7857862df51fc56c416f9",
};

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("unknown chain {0:?}, expected \"bitcoin\" or \"litecoin\"")]
pub struct UnknownChain(String);

impl Chain {
    pub const fn params(self) -> &'static ChainParams {
        match self {
            Chain::Bitcoin => &BITCOIN,
            Chain::Litecoin => &LITECOIN,
        }
    }

    /// The bech32 HRP of this chain on the given (shadow) bitcoin network.
    ///
    /// Signet and all testnet variants share the testnet HRP,
    /// as they do on Bitcoin itself.
    pub const fn bech32_hrp(self, network: bitcoin::Network) -> &'static str {
        let params = self.params();
        match network {
            bitcoin::Network::Bitcoin => params.bech32_hrp_mainnet,
            bitcoin::Network::Regtest => params.bech32_hrp_regtest,
            _ => params.bech32_hrp_testnet,
        }
    }

    /// The chain's genesis block hash on the given (shadow) network.
    ///
    /// A fresh BDK wallet anchors its local chain on this hash; derived
    /// from the shadow network alone it would anchor on Bitcoin's
    /// genesis and never find an agreement block with an electrum
    /// server of another chain.
    pub fn genesis_hash(self, network: bitcoin::Network) -> bitcoin::BlockHash {
        let params = self.params();
        let hex = match network {
            bitcoin::Network::Bitcoin => params.genesis_mainnet,
            bitcoin::Network::Regtest => params.genesis_regtest,
            _ => params.genesis_testnet,
        };
        hex.parse().expect("genesis hash constants are valid")
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Chain::Bitcoin => "bitcoin",
            Chain::Litecoin => "litecoin",
        }
    }
}

impl fmt::Display for Chain {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for Chain {
    type Err = UnknownChain;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "bitcoin" => Ok(Chain::Bitcoin),
            "litecoin" => Ok(Chain::Litecoin),
            other => Err(UnknownChain(other.to_string())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn params_lookup_matches_chain() {
        assert_eq!(Chain::Bitcoin.params().chain, Chain::Bitcoin);
        assert_eq!(Chain::Litecoin.params().chain, Chain::Litecoin);
        assert_eq!(Chain::Bitcoin.params().ticker, "BTC");
        assert_eq!(Chain::Litecoin.params().ticker, "LTC");
        assert_eq!(Chain::Litecoin.params().slip44_coin_type, 2);
    }

    #[test]
    fn bech32_hrp_per_network() {
        assert_eq!(Chain::Bitcoin.bech32_hrp(bitcoin::Network::Bitcoin), "bc");
        assert_eq!(Chain::Bitcoin.bech32_hrp(bitcoin::Network::Testnet), "tb");
        assert_eq!(Chain::Bitcoin.bech32_hrp(bitcoin::Network::Signet), "tb");
        assert_eq!(Chain::Bitcoin.bech32_hrp(bitcoin::Network::Regtest), "bcrt");
        assert_eq!(Chain::Litecoin.bech32_hrp(bitcoin::Network::Bitcoin), "ltc");
        assert_eq!(
            Chain::Litecoin.bech32_hrp(bitcoin::Network::Testnet),
            "tltc"
        );
        assert_eq!(
            Chain::Litecoin.bech32_hrp(bitcoin::Network::Regtest),
            "rltc"
        );
    }

    #[test]
    fn serde_round_trip_is_lowercase() {
        let json = serde_json::to_string(&Chain::Litecoin).unwrap();
        assert_eq!(json, "\"litecoin\"");
        let chain: Chain = serde_json::from_str(&json).unwrap();
        assert_eq!(chain, Chain::Litecoin);
    }

    #[test]
    fn display_and_from_str_round_trip() {
        for chain in [Chain::Bitcoin, Chain::Litecoin] {
            assert_eq!(chain.to_string().parse::<Chain>().unwrap(), chain);
        }
        assert!("dogecoin".parse::<Chain>().is_err());
    }
}

#[cfg(test)]
mod genesis_tests {
    use super::*;

    #[test]
    fn bitcoin_genesis_hashes_match_rust_bitcoin() {
        for network in [
            bitcoin::Network::Bitcoin,
            bitcoin::Network::Testnet,
            bitcoin::Network::Regtest,
        ] {
            assert_eq!(
                Chain::Bitcoin.genesis_hash(network),
                bitcoin::constants::genesis_block(network).block_hash(),
            );
        }
    }

    #[test]
    fn litecoin_genesis_hashes_are_pinned() {
        assert_eq!(
            Chain::Litecoin
                .genesis_hash(bitcoin::Network::Regtest)
                .to_string(),
            "530827f38f93b43ed12af0b3ad25a288dc02ed74d6d7857862df51fc56c416f9"
        );
        assert_eq!(
            Chain::Litecoin
                .genesis_hash(bitcoin::Network::Bitcoin)
                .to_string(),
            "12a765e31ffd4059bada1e25190f6e98c99d9714d334efa41a195a7e7e04bfe2"
        );
    }
}
