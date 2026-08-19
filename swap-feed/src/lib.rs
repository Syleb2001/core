pub mod bitfinex;
pub mod exolix;
pub mod kraken;
pub mod kucoin;
pub mod rate;
pub mod traits;

// Re-exports for convenience
pub use kraken::{Error as KrakenError, PriceUpdates, connect};
pub use rate::{ExchangeRate, FixedRate, Rate};
pub use traits::LatestRate;

mod ticker;

// Core functions
pub fn connect_kraken(
    url: url::Url,
    chain: swap_chain::Chain,
) -> anyhow::Result<kraken::PriceUpdates> {
    kraken::connect(url, chain)
}

pub fn connect_bitfinex(
    url: url::Url,
    chain: swap_chain::Chain,
) -> anyhow::Result<bitfinex::PriceUpdates> {
    bitfinex::connect(url, chain)
}

pub fn connect_kucoin(
    url: url::Url,
    client: reqwest::Client,
    chain: swap_chain::Chain,
) -> anyhow::Result<kucoin::PriceUpdates> {
    kucoin::connect(url, client, chain)
}

pub fn connect_exolix(
    url: url::Url,
    api_key: String,
    poll_interval: std::time::Duration,
    client: reqwest::Client,
    chain: swap_chain::Chain,
) -> anyhow::Result<exolix::PriceUpdates> {
    exolix::connect(url, api_key, poll_interval, client, chain)
}
