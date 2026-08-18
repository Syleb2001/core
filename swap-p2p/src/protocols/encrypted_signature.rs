use crate::out_event;
use crate::protocols::metered::{Metered, RequestResponseMetrics};
use libp2p::request_response::{self};
use libp2p::{PeerId, StreamProtocol};
use serde::{Deserialize, Serialize};
use swap_chain::Chain;
use uuid::Uuid;

const PROTOCOL: &str = "/comit/xmr/btc/encrypted_signature/1.0.0";
const PROTOCOL_LTC: &str = "/comit/xmr/ltc/encrypted_signature/1.0.0";
type OutEvent = request_response::Event<Request, ()>;
type Message = request_response::Message<Request, ()>;

pub type Behaviour = Metered<request_response::cbor::Behaviour<Request, ()>>;

pub fn protocol(chain: Chain) -> &'static str {
    match chain {
        Chain::Bitcoin => PROTOCOL,
        Chain::Litecoin => PROTOCOL_LTC,
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Request {
    pub swap_id: Uuid,
    pub tx_redeem_encsig: swap_core::bitcoin::EncryptedSignature,
}

pub fn alice(chain: Chain, metrics: Option<RequestResponseMetrics>) -> Behaviour {
    Metered::new(
        request_response::cbor::Behaviour::new(
            vec![(
                StreamProtocol::new(protocol(chain)),
                request_response::ProtocolSupport::Inbound,
            )],
            request_response::Config::default()
                .with_request_timeout(crate::defaults::DEFAULT_REQUEST_TIMEOUT),
        ),
        protocol(chain),
        metrics,
    )
}

pub fn bob(chain: Chain) -> Behaviour {
    Metered::new(
        request_response::cbor::Behaviour::new(
            vec![(
                StreamProtocol::new(protocol(chain)),
                request_response::ProtocolSupport::Outbound,
            )],
            request_response::Config::default()
                .with_request_timeout(crate::defaults::DEFAULT_REQUEST_TIMEOUT),
        ),
        protocol(chain),
        None,
    )
}

impl From<(PeerId, Message)> for out_event::alice::OutEvent {
    fn from((peer, message): (PeerId, Message)) -> Self {
        match message {
            Message::Request {
                request, channel, ..
            } => Self::EncryptedSignatureReceived {
                msg: request,
                channel,
                peer,
            },
            Message::Response { .. } => Self::unexpected_response(peer),
        }
    }
}
crate::impl_from_rr_event!(OutEvent, out_event::alice::OutEvent, PROTOCOL);

impl From<(PeerId, Message)> for out_event::bob::OutEvent {
    fn from((peer, message): (PeerId, Message)) -> Self {
        match message {
            Message::Request { .. } => Self::unexpected_request(peer),
            Message::Response { request_id, .. } => {
                Self::EncryptedSignatureAcknowledged { id: request_id }
            }
        }
    }
}

crate::impl_from_rr_event!(OutEvent, out_event::bob::OutEvent, PROTOCOL);
