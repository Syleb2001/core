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
