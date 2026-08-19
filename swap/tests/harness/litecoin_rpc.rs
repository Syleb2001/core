//! Minimal JSON-RPC client for the litecoind container.
//!
//! The `bitcoin-harness` client used for the Bitcoin tests parses every
//! address into `bitcoin::Address`, which rejects Litecoin encodings
//! (bech32 HRP `rltc` on regtest). This client keeps addresses as
//! opaque strings instead — litecoind is the only party that needs to
//! understand them.

use anyhow::{Context, Result, bail};
use serde_json::{Value, json};
use std::time::Duration;
use url::Url;

pub const WALLET_NAME: &str = "testwallet";

#[derive(Debug, Clone)]
pub struct Client {
    url: Url,
    inner: reqwest::Client,
}

impl Client {
    pub fn new(url: Url) -> Self {
        Self {
            url,
            inner: reqwest::Client::new(),
        }
    }

    async fn call(&self, wallet: Option<&str>, method: &str, params: Value) -> Result<Value> {
        let mut url = self.url.clone();
        if let Some(wallet) = wallet {
            url.set_path(&format!("/wallet/{wallet}"));
        }

        let username = url.username().to_string();
        let password = url.password().map(str::to_string);

        let response = self
            .inner
            .post(url)
            .basic_auth(username, password)
            .json(&json!({
                "jsonrpc": "1.0",
                "id": "harness",
                "method": method,
                "params": params,
            }))
            .send()
            .await
            .with_context(|| format!("Failed to reach litecoind for {method}"))?;

        let body: Value = response
            .json()
            .await
            .with_context(|| format!("litecoind returned a non-JSON response to {method}"))?;

        if let Some(error) = body.get("error").filter(|error| !error.is_null()) {
            bail!("litecoind rejected {method}: {error}");
        }

        Ok(body["result"].clone())
    }

    pub async fn create_wallet(&self, name: &str) -> Result<()> {
        self.call(None, "createwallet", json!([name])).await?;
        Ok(())
    }

    /// A fresh legacy (base58) address of the node's test wallet.
    ///
    /// Legacy is deliberate: Litecoin's regtest base58 version bytes
    /// match Bitcoin's testnet ones, so these addresses remain readable
    /// everywhere, and litecoind accepts them for mining rewards.
    pub async fn new_legacy_address(&self) -> Result<String> {
        let address = self
            .call(Some(WALLET_NAME), "getnewaddress", json!(["", "legacy"]))
            .await?;
        address
            .as_str()
            .map(str::to_string)
            .context("getnewaddress did not return a string")
    }

    pub async fn generate_to_address(&self, blocks: u32, address: &str) -> Result<()> {
        self.call(None, "generatetoaddress", json!([blocks, address]))
            .await?;
        Ok(())
    }

    pub async fn send_to_address(&self, address: &str, amount: bitcoin::Amount) -> Result<()> {
        // Amounts as strings: floats would go through f64 rounding
        self.call(
            Some(WALLET_NAME),
            "sendtoaddress",
            json!([address, format!("{:.8}", amount.to_btc())]),
        )
        .await?;
        Ok(())
    }
}

/// Set up the node's own wallet and make its coinbase rewards spendable,
/// mirroring `init_bitcoind` on the Bitcoin side.
pub async fn init_litecoind(node_url: Url, spendable_quantity: u32) -> Result<Client> {
    let client = Client::new(node_url);

    client.create_wallet(WALLET_NAME).await?;

    let reward_address = client.new_legacy_address().await?;
    client
        .generate_to_address(101 + spendable_quantity, &reward_address)
        .await?;

    tokio::spawn(mine(client.clone(), reward_address));

    Ok(client)
}

/// Background miner: one block a second, like the Bitcoin harness.
async fn mine(client: Client, reward_address: String) -> Result<()> {
    loop {
        tokio::time::sleep(Duration::from_secs(1)).await;
        client.generate_to_address(1, &reward_address).await?;
    }
}
