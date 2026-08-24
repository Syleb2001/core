use testcontainers::{Image, ImageArgs, core::WaitFor};

pub const RPC_USER: &str = "admin";
pub const RPC_PASSWORD: &str = "123";
pub const RPC_PORT: u16 = 18443;
pub const PORT: u16 = 18886;

/// Litecoin Core in regtest mode.
///
/// Litecoin Core is a fork of Bitcoin Core, so the daemon accepts the
/// same flags and speaks the same RPC protocol; only the address
/// encodings differ (bech32 HRP `rltc`, legacy version bytes shared
/// with Bitcoin's testnet).
#[derive(Debug, Default)]
pub struct Litecoind;

impl Image for Litecoind {
    type Args = LitecoindArgs;

    fn name(&self) -> String {
        "uphold/litecoin-core".into()
    }

    fn tag(&self) -> String {
        "0.21".into()
    }

    fn ready_conditions(&self) -> Vec<WaitFor> {
        vec![WaitFor::message_on_stdout("init message: Done loading")]
    }

    // Unlike the bitcoind image, this one does not EXPOSE the regtest
    // RPC port in its Dockerfile, so testcontainers would not map it
    fn expose_ports(&self) -> Vec<u16> {
        vec![RPC_PORT]
    }
}

#[derive(Debug, Clone, Default)]
pub struct LitecoindArgs;

impl IntoIterator for LitecoindArgs {
    type Item = String;
    type IntoIter = ::std::vec::IntoIter<String>;

    fn into_iter(self) -> <Self as IntoIterator>::IntoIter {
        let args = vec![
            "-server".to_string(),
            "-regtest".to_string(),
            "-listen=1".to_string(),
            "-prune=0".to_string(),
            "-rpcallowip=0.0.0.0/0".to_string(),
            "-rpcbind=0.0.0.0".to_string(),
            format!("-rpcuser={}", RPC_USER),
            format!("-rpcpassword={}", RPC_PASSWORD),
            "-printtoconsole".to_string(),
            "-fallbackfee=0.0002".to_string(),
            format!("-rpcport={}", RPC_PORT),
            format!("-port={}", PORT),
            "-rest".to_string(),
        ];

        args.into_iter()
    }
}

impl ImageArgs for LitecoindArgs {
    fn into_iterator(self) -> Box<dyn Iterator<Item = String>> {
        Box::new(self.into_iter())
    }
}
