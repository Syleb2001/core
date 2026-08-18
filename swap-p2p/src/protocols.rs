use swap_chain::Chain;

pub mod cooperative_xmr_redeem_after_punish;
pub mod encrypted_signature;
pub mod metered;
pub mod notice;
pub mod quote;
pub mod quotes;
pub mod quotes_cached;
pub mod redial;
pub mod rendezvous;
pub mod swap_setup;
pub mod transfer_proof;
pub mod wormhole;

const IDENTIFY_PROTOCOL: &str = "/comit/xmr/btc/1.0.0";
const IDENTIFY_PROTOCOL_LTC: &str = "/comit/xmr/ltc/1.0.0";

/// The libp2p identify "protocol version" advertised for a script chain.
pub fn identify_protocol_version(chain: Chain) -> &'static str {
    match chain {
        Chain::Bitcoin => IDENTIFY_PROTOCOL,
        Chain::Litecoin => IDENTIFY_PROTOCOL_LTC,
    }
}

#[cfg(test)]
mod chain_protocol_tests {
    use super::*;

    /// The Bitcoin protocol ids are a network-wide convention:
    /// changing any of them breaks compatibility with every deployed peer.
    #[test]
    fn bitcoin_protocol_ids_are_stable() {
        assert_eq!(
            identify_protocol_version(Chain::Bitcoin),
            "/comit/xmr/btc/1.0.0"
        );
        assert_eq!(
            quote::protocol(Chain::Bitcoin),
            "/comit/xmr/btc/bid-quote/2.0.0"
        );
        assert_eq!(
            swap_setup::protocol::name(Chain::Bitcoin),
            "/comit/xmr/btc/swap_setup/1.0.0"
        );
        assert_eq!(
            transfer_proof::protocol(Chain::Bitcoin),
            "/comit/xmr/btc/transfer_proof/1.0.0"
        );
        assert_eq!(
            encrypted_signature::protocol(Chain::Bitcoin),
            "/comit/xmr/btc/encrypted_signature/1.0.0"
        );
        assert_eq!(
            cooperative_xmr_redeem_after_punish::protocol(Chain::Bitcoin),
            "/comit/xmr/btc/cooperative_xmr_redeem_after_punish/1.0.0"
        );
    }

    #[test]
    fn litecoin_protocol_ids_never_collide_with_bitcoin() {
        for (bitcoin, litecoin) in [
            (
                identify_protocol_version(Chain::Bitcoin),
                identify_protocol_version(Chain::Litecoin),
            ),
            (
                quote::protocol(Chain::Bitcoin),
                quote::protocol(Chain::Litecoin),
            ),
            (
                swap_setup::protocol::name(Chain::Bitcoin),
                swap_setup::protocol::name(Chain::Litecoin),
            ),
            (
                transfer_proof::protocol(Chain::Bitcoin),
                transfer_proof::protocol(Chain::Litecoin),
            ),
            (
                encrypted_signature::protocol(Chain::Bitcoin),
                encrypted_signature::protocol(Chain::Litecoin),
            ),
            (
                cooperative_xmr_redeem_after_punish::protocol(Chain::Bitcoin),
                cooperative_xmr_redeem_after_punish::protocol(Chain::Litecoin),
            ),
        ] {
            assert_ne!(bitcoin, litecoin);
            assert!(bitcoin.contains("/btc/"));
            assert!(litecoin.contains("/ltc/"));
        }
    }

    #[test]
    fn rendezvous_namespaces_are_chain_scoped() {
        use rendezvous::XmrBtcNamespace;

        assert_eq!(
            XmrBtcNamespace::for_chain(Chain::Bitcoin, false).to_string(),
            "xmr-btc-swap-mainnet"
        );
        assert_eq!(
            XmrBtcNamespace::for_chain(Chain::Bitcoin, true).to_string(),
            "xmr-btc-swap-testnet"
        );
        assert_eq!(
            XmrBtcNamespace::for_chain(Chain::Litecoin, false).to_string(),
            "xmr-ltc-swap-mainnet"
        );
        assert_eq!(
            XmrBtcNamespace::for_chain(Chain::Litecoin, true).to_string(),
            "xmr-ltc-swap-testnet"
        );
        assert_eq!(
            XmrBtcNamespace::from_is_testnet(false),
            XmrBtcNamespace::Mainnet
        );
    }
}
