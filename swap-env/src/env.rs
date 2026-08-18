use crate::config::Config as AsbConfig;
use serde::Serialize;
use std::cmp::max;
use std::time::Duration;
use swap_chain::Chain;
use time::ext::NumericalStdDuration;

#[derive(Debug, Copy, Clone, PartialEq, Eq, Serialize)]
pub struct Config {
    /// The script chain this environment describes.
    ///
    /// The `bitcoin_*` fields below are named after the original
    /// Bitcoin-only protocol; for another script chain they carry that
    /// chain's values, with `bitcoin_network` holding the internal
    /// (rust-bitcoin) "shadow network" representation.
    pub chain: Chain,
    pub bitcoin_lock_mempool_timeout: Duration,
    pub bitcoin_lock_confirmed_timeout: Duration,
    pub bitcoin_finality_confirmations: u32,
    /// The upper bound for the number of blocks that will be mined before our
    /// Bitcoin transaction is included in a block
    pub bitcoin_blocks_till_confirmed_upper_bound_assumption: u32,
    pub bitcoin_avg_block_time: Duration,
    pub bitcoin_cancel_timelock: u32,
    pub bitcoin_punish_timelock: u32,
    pub bitcoin_remaining_refund_timelock: u32,
    pub bitcoin_network: bitcoin::Network,
    pub monero_avg_block_time: Duration,
    pub monero_finality_confirmations: u64,
    // If Alice does manage to lock her Monero within this timeout, she will initiate an early refund of the Bitcoin.
    pub monero_lock_retry_timeout: Duration,
    // After this many confirmations we assume that the Monero transaction is safe from double spending
    pub monero_double_spend_safe_confirmations: u64,
    #[serde(with = "swap_serde::monero::network")]
    pub monero_network: monero_address::Network,
}

impl Config {
    pub fn chain_params(&self) -> &'static swap_chain::ChainParams {
        self.chain.params()
    }

    pub fn bitcoin_sync_interval(&self) -> Duration {
        sync_interval(self.bitcoin_avg_block_time)
    }

    pub fn monero_sync_interval(&self) -> Duration {
        sync_interval(self.monero_avg_block_time)
    }
}

pub trait GetConfig {
    fn get_config() -> Config;
}

#[derive(Clone, Copy)]
pub struct Mainnet;

#[derive(Clone, Copy)]
pub struct Testnet;

#[derive(Clone, Copy)]
pub struct Regtest;

#[derive(Clone, Copy)]
pub struct LitecoinMainnet;

#[derive(Clone, Copy)]
pub struct LitecoinTestnet;

#[derive(Clone, Copy)]
pub struct LitecoinRegtest;

impl GetConfig for Mainnet {
    fn get_config() -> Config {
        Config {
            chain: Chain::Bitcoin,
            bitcoin_lock_mempool_timeout: 10.std_minutes(),
            bitcoin_lock_confirmed_timeout: 2.std_hours(),
            bitcoin_finality_confirmations: 1,
            // We assume that a transaction that was constructed to be confirmed within one block
            // will be confirmed within at most 6 blocks
            bitcoin_blocks_till_confirmed_upper_bound_assumption: 6,
            bitcoin_avg_block_time: 10.std_minutes(),
            bitcoin_cancel_timelock: 24,
            bitcoin_punish_timelock: 144,
            bitcoin_remaining_refund_timelock: 2,
            bitcoin_network: bitcoin::Network::Bitcoin,
            monero_avg_block_time: 2.std_minutes(),
            // If Alice cannot lock her Monero within this timeout,
            // she will initiate an early refund of Bobs Bitcoin
            monero_lock_retry_timeout: 10.std_minutes(),
            monero_finality_confirmations: 10,
            monero_double_spend_safe_confirmations: 10,
            monero_network: monero_address::Network::Mainnet,
        }
    }
}

impl GetConfig for Testnet {
    fn get_config() -> Config {
        Config {
            chain: Chain::Bitcoin,
            bitcoin_lock_mempool_timeout: 10.std_minutes(),
            bitcoin_lock_confirmed_timeout: 1.std_hours(),
            bitcoin_finality_confirmations: 1,
            bitcoin_blocks_till_confirmed_upper_bound_assumption: 6,
            bitcoin_avg_block_time: 10.std_minutes(),
            bitcoin_cancel_timelock: 12 * 3,
            bitcoin_punish_timelock: 24 * 3,
            bitcoin_remaining_refund_timelock: 2,
            bitcoin_network: bitcoin::Network::Testnet,
            monero_avg_block_time: 2.std_minutes(),
            monero_lock_retry_timeout: 10.std_minutes(),
            monero_finality_confirmations: 10,
            monero_double_spend_safe_confirmations: 10,
            monero_network: monero_address::Network::Stagenet,
        }
    }
}

impl GetConfig for Regtest {
    fn get_config() -> Config {
        Config {
            chain: Chain::Bitcoin,
            bitcoin_lock_mempool_timeout: 30.std_seconds(),
            bitcoin_lock_confirmed_timeout: 5.std_minutes(),
            bitcoin_finality_confirmations: 1,
            bitcoin_blocks_till_confirmed_upper_bound_assumption: 6,
            bitcoin_avg_block_time: 5.std_seconds(),
            bitcoin_cancel_timelock: 100,
            bitcoin_punish_timelock: 50,
            bitcoin_remaining_refund_timelock: 5,
            bitcoin_network: bitcoin::Network::Regtest,
            monero_avg_block_time: 1.std_seconds(),
            monero_lock_retry_timeout: 1.std_minutes(),
            monero_finality_confirmations: 10,
            monero_double_spend_safe_confirmations: 10,
            monero_network: monero_address::Network::Mainnet, // yes this is strange
        }
    }
}

// The Litecoin environments keep the same wall-clock durations as their
// Bitcoin counterparts: Litecoin blocks arrive every 2.5 minutes instead of
// every 10, so every block-denominated value is scaled by 4.
impl GetConfig for LitecoinMainnet {
    fn get_config() -> Config {
        Config {
            chain: Chain::Litecoin,
            bitcoin_lock_mempool_timeout: 10.std_minutes(),
            // ~24 Litecoin blocks, same policy as Bitcoin's 2h (~12 blocks)
            bitcoin_lock_confirmed_timeout: 1.std_hours(),
            // Litecoin has considerably less hashrate than Bitcoin,
            // so we require one extra confirmation
            bitcoin_finality_confirmations: 2,
            bitcoin_blocks_till_confirmed_upper_bound_assumption: 6,
            bitcoin_avg_block_time: 150.std_seconds(),
            bitcoin_cancel_timelock: 24 * 4,
            bitcoin_punish_timelock: 144 * 4,
            bitcoin_remaining_refund_timelock: 2 * 4,
            // The shadow network Litecoin runs on internally
            bitcoin_network: bitcoin::Network::Bitcoin,
            monero_avg_block_time: 2.std_minutes(),
            monero_lock_retry_timeout: 10.std_minutes(),
            monero_finality_confirmations: 10,
            monero_double_spend_safe_confirmations: 10,
            monero_network: monero_address::Network::Mainnet,
        }
    }
}

impl GetConfig for LitecoinTestnet {
    fn get_config() -> Config {
        Config {
            chain: Chain::Litecoin,
            bitcoin_lock_mempool_timeout: 10.std_minutes(),
            bitcoin_lock_confirmed_timeout: 1.std_hours(),
            bitcoin_finality_confirmations: 2,
            bitcoin_blocks_till_confirmed_upper_bound_assumption: 6,
            bitcoin_avg_block_time: 150.std_seconds(),
            bitcoin_cancel_timelock: 12 * 3 * 4,
            bitcoin_punish_timelock: 24 * 3 * 4,
            bitcoin_remaining_refund_timelock: 2 * 4,
            bitcoin_network: bitcoin::Network::Testnet,
            monero_avg_block_time: 2.std_minutes(),
            monero_lock_retry_timeout: 10.std_minutes(),
            monero_finality_confirmations: 10,
            monero_double_spend_safe_confirmations: 10,
            monero_network: monero_address::Network::Stagenet,
        }
    }
}

impl GetConfig for LitecoinRegtest {
    fn get_config() -> Config {
        // Regtest blocks are mined on demand, so the Bitcoin regtest values
        // work unchanged; only the chain identity differs.
        Config {
            chain: Chain::Litecoin,
            ..Regtest::get_config()
        }
    }
}

fn sync_interval(avg_block_time: Duration) -> Duration {
    max(avg_block_time / 10, Duration::from_secs(1))
}

/// Build the environment config for the script chain configured in the
/// ASB config file, applying its finality-confirmation overrides.
pub fn new(is_testnet: bool, asb_config: &AsbConfig) -> anyhow::Result<Config> {
    let (chain, script_chain) = asb_config.script_chain()?;

    let env_config = match (chain, is_testnet) {
        (Chain::Bitcoin, false) => Mainnet::get_config(),
        (Chain::Bitcoin, true) => Testnet::get_config(),
        (Chain::Litecoin, false) => LitecoinMainnet::get_config(),
        (Chain::Litecoin, true) => LitecoinTestnet::get_config(),
    };

    let env_config =
        if let Some(bitcoin_finality_confirmations) = script_chain.finality_confirmations {
            Config {
                bitcoin_finality_confirmations,
                ..env_config
            }
        } else {
            env_config
        };

    let env_config =
        if let Some(monero_finality_confirmations) = asb_config.monero.finality_confirmations {
            Config {
                monero_finality_confirmations,
                ..env_config
            }
        } else {
            env_config
        };

    Ok(env_config)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn check_interval_is_one_second_if_avg_blocktime_is_one_second() {
        let interval = sync_interval(Duration::from_secs(1));

        assert_eq!(interval, Duration::from_secs(1))
    }

    #[test]
    fn check_interval_is_tenth_of_avg_blocktime() {
        let interval = sync_interval(Duration::from_secs(100));

        assert_eq!(interval, Duration::from_secs(10))
    }

    fn wall_clock(blocks: u32, block_time: Duration) -> Duration {
        block_time * blocks
    }

    #[test]
    fn litecoin_environments_keep_bitcoin_wall_clock_timelocks() {
        for (bitcoin, litecoin) in [
            (Mainnet::get_config(), LitecoinMainnet::get_config()),
            (Testnet::get_config(), LitecoinTestnet::get_config()),
        ] {
            for (bitcoin_blocks, litecoin_blocks) in [
                (
                    bitcoin.bitcoin_cancel_timelock,
                    litecoin.bitcoin_cancel_timelock,
                ),
                (
                    bitcoin.bitcoin_punish_timelock,
                    litecoin.bitcoin_punish_timelock,
                ),
                (
                    bitcoin.bitcoin_remaining_refund_timelock,
                    litecoin.bitcoin_remaining_refund_timelock,
                ),
            ] {
                assert_eq!(
                    wall_clock(bitcoin_blocks, bitcoin.bitcoin_avg_block_time),
                    wall_clock(litecoin_blocks, litecoin.bitcoin_avg_block_time),
                );
            }
        }
    }

    #[test]
    fn litecoin_environment_values() {
        let config = LitecoinMainnet::get_config();

        assert_eq!(config.chain, Chain::Litecoin);
        assert_eq!(config.bitcoin_avg_block_time, Duration::from_secs(150));
        assert_eq!(config.bitcoin_cancel_timelock, 96);
        assert_eq!(config.bitcoin_punish_timelock, 576);
        assert_eq!(config.bitcoin_remaining_refund_timelock, 8);
        assert_eq!(config.bitcoin_finality_confirmations, 2);
        // The shadow network Litecoin mainnet runs on internally
        assert_eq!(config.bitcoin_network, bitcoin::Network::Bitcoin);
        assert_eq!(config.monero_network, monero_address::Network::Mainnet);

        assert_eq!(
            LitecoinTestnet::get_config().bitcoin_network,
            bitcoin::Network::Testnet
        );
        assert_eq!(
            LitecoinRegtest::get_config().bitcoin_network,
            bitcoin::Network::Regtest
        );
        assert_eq!(LitecoinRegtest::get_config().chain, Chain::Litecoin);
    }
}
