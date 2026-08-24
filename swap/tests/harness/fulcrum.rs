use crate::harness::litecoind;
use testcontainers::{Image, ImageArgs, core::WaitFor};

pub const TCP_PORT: u16 = 50001;

/// Fulcrum electrum server, pointed at the litecoind container.
///
/// The esplora electrs fork used for the Bitcoin tests does not speak
/// Litecoin; Fulcrum supports it natively via `--coin LTC` and serves
/// the same electrum protocol the wallet already uses.
#[derive(Debug)]
pub struct Fulcrum {
    args: FulcrumArgs,
}

impl Fulcrum {
    pub fn new(litecoind_rpc_addr: String) -> Self {
        Fulcrum {
            args: FulcrumArgs { litecoind_rpc_addr },
        }
    }

    pub fn self_and_args(self) -> (Self, FulcrumArgs) {
        let args = self.args.clone();
        (self, args)
    }
}

impl Image for Fulcrum {
    type Args = FulcrumArgs;

    fn name(&self) -> String {
        "cculianu/fulcrum".into()
    }

    fn tag(&self) -> String {
        "latest".into()
    }

    fn ready_conditions(&self) -> Vec<WaitFor> {
        // Printed once the electrum TCP listener accepts connections.
        // Fulcrum logs through Qt, which writes to stderr.
        vec![WaitFor::message_on_stderr("Service started")]
    }

    // Make sure the electrum port is mapped even if the image's
    // Dockerfile does not EXPOSE it
    fn expose_ports(&self) -> Vec<u16> {
        vec![TCP_PORT]
    }
}

#[derive(Debug, Clone)]
pub struct FulcrumArgs {
    pub litecoind_rpc_addr: String,
}

impl IntoIterator for FulcrumArgs {
    type Item = String;
    type IntoIter = ::std::vec::IntoIter<String>;

    fn into_iter(self) -> <Self as IntoIterator>::IntoIter {
        let args = vec![
            "Fulcrum".to_string(),
            // No coin flag: `coin` only exists as a config-file key, and
            // Fulcrum auto-detects Litecoin by querying the daemon
            format!("--bitcoind={}", self.litecoind_rpc_addr),
            format!("--rpcuser={}", litecoind::RPC_USER),
            format!("--rpcpassword={}", litecoind::RPC_PASSWORD),
            format!("--tcp=0.0.0.0:{}", TCP_PORT),
            // No --datadir here: the image's entrypoint appends
            // `-D /data` (plus the SSL cert pair) on its own when the
            // command starts with `Fulcrum`
        ];

        args.into_iter()
    }
}

impl ImageArgs for FulcrumArgs {
    fn into_iterator(self) -> Box<dyn Iterator<Item = String>> {
        Box::new(self.into_iter())
    }
}
