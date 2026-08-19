use anyhow::{Context, Result};
use futures::{SinkExt, StreamExt, TryStreamExt};
use serde::Deserialize;
use std::convert::TryFrom;
use swap_chain::Chain;
use url::Url;

/// Connect to Kraken websocket API for a constant stream of rate updates,
/// pricing 1 XMR in the given script chain's asset.
///
/// Kraken has no direct XMR/LTC market, so for Litecoin we subscribe to
/// both XMR/XBT and LTC/XBT and emit the cross rate
/// (XMR/XBT ask ÷ LTC/XBT bid — the maker-conservative combination),
/// only while both legs are fresh.
///
/// If the connection fails, it will automatically be re-established.
///
/// price_ticker_ws_url_kraken must point to a websocket server that follows the kraken
/// price ticker protocol
/// See: https://docs.kraken.com/websockets/
pub fn connect(price_ticker_ws_url_kraken: Url, chain: Chain) -> Result<PriceUpdates> {
    crate::ticker::connect(
        "Kraken",
        KrakenParams {
            ws_url: price_ticker_ws_url_kraken,
            chain,
        },
        connection::new,
    )
}

#[derive(Clone)]
pub struct KrakenParams {
    pub ws_url: Url,
    pub chain: Chain,
}

pub type PriceUpdates = crate::ticker::PriceUpdates<wire::PriceUpdate>;
pub type PriceUpdate = crate::ticker::PriceUpdate<wire::PriceUpdate>;
pub type Error = crate::ticker::Error;

/// Kraken websocket connection module.
///
/// Responsible for establishing a connection to the Kraken websocket API and
/// transforming the received websocket frames into a stream of rate updates.
/// The connection may fail in which case it is simply terminated and the stream
/// ends.
mod connection {
    use super::*;
    use crate::kraken::wire;
    use futures::stream::BoxStream;
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, Instant};
    use tokio_tungstenite::tungstenite;

    /// A cross-rate leg older than this is considered stale and stops the
    /// cross rate from being emitted until the leg ticks again.
    const CROSS_RATE_LEG_MAX_AGE: Duration = Duration::from_secs(10 * 60);

    pub async fn new(
        params: Arc<KrakenParams>,
    ) -> Result<BoxStream<'static, Result<wire::PriceUpdate, Error>>> {
        let (mut rate_stream, _) = tokio_tungstenite::connect_async(&params.ws_url)
            .await
            .context("Failed to connect to Kraken websocket API")?;

        let subscribe_payload = match params.chain {
            Chain::Bitcoin => SUBSCRIBE_XMR_BTC_TICKER_PAYLOAD,
            Chain::Litecoin => SUBSCRIBE_XMR_LTC_CROSS_TICKER_PAYLOAD,
        };
        rate_stream.send(subscribe_payload.into()).await?;

        let chain = params.chain;
        let cross_rate_state = Arc::new(Mutex::new(CrossRateState::default()));

        let stream = rate_stream
            .err_into()
            .try_filter_map(move |msg| {
                let cross_rate_state = cross_rate_state.clone();
                async move {
                    let Some(ticker) = parse_message(msg)? else {
                        return Ok(None);
                    };

                    match chain {
                        Chain::Bitcoin => Ok(Some(wire::PriceUpdate { ask: ticker.ask })),
                        Chain::Litecoin => {
                            let mut state = cross_rate_state
                                .lock()
                                .expect("cross rate state lock is never poisoned");
                            Ok(state.update(&ticker).map(|ask| wire::PriceUpdate { ask }))
                        }
                    }
                }
            })
            .boxed();

        Ok(stream)
    }

    /// The most recent value of each leg of the XMR/LTC cross rate.
    #[derive(Default)]
    struct CrossRateState {
        xmr_btc_ask: Option<(Instant, bitcoin::Amount)>,
        ltc_btc_bid: Option<(Instant, bitcoin::Amount)>,
    }

    impl CrossRateState {
        /// Feed one ticker into the state, returning the price of 1 XMR in
        /// the Litecoin asset whenever both legs are known and fresh.
        fn update(&mut self, ticker: &wire::Ticker) -> Option<bitcoin::Amount> {
            match ticker.pair.as_deref() {
                Some(XMR_BTC_PAIR) => self.xmr_btc_ask = Some((Instant::now(), ticker.ask)),
                Some(LTC_BTC_PAIR) => self.ltc_btc_bid = Some((Instant::now(), ticker.bid)),
                other => {
                    tracing::warn!(pair = ?other, "Ignoring Kraken ticker for unexpected pair");
                    return None;
                }
            }

            let (ask_at, xmr_btc_ask) = self.xmr_btc_ask?;
            let (bid_at, ltc_btc_bid) = self.ltc_btc_bid?;
            if ask_at.elapsed() > CROSS_RATE_LEG_MAX_AGE
                || bid_at.elapsed() > CROSS_RATE_LEG_MAX_AGE
            {
                return None;
            }

            cross_rate(xmr_btc_ask, ltc_btc_bid)
        }
    }

    /// Price of 1 XMR in litoshis, from the XMR/BTC ask and the LTC/BTC bid.
    ///
    /// Using the bid of the LTC leg (what LTC actually sells for) keeps the
    /// combination conservative for the maker. Returns `None` for a zero bid
    /// or an overflowing result rather than producing a nonsense price.
    fn cross_rate(
        xmr_btc_ask: bitcoin::Amount,
        ltc_btc_bid: bitcoin::Amount,
    ) -> Option<bitcoin::Amount> {
        let litoshi = (xmr_btc_ask.to_sat() as u128)
            .checked_mul(100_000_000)?
            .checked_div(ltc_btc_bid.to_sat() as u128)?;

        u64::try_from(litoshi).ok().map(bitcoin::Amount::from_sat)
    }

    /// Parse a websocket message into a [`wire::Ticker`].
    ///
    /// Messages which are not actually ticker updates are ignored and result in
    /// `None` being returned. In the context of a [`TryStream`], these will
    /// simply be filtered out.
    fn parse_message(msg: tungstenite::Message) -> Result<Option<wire::Ticker>, Error> {
        let msg = match msg {
            tungstenite::Message::Text(msg) => msg,
            tungstenite::Message::Close(close_frame) => {
                if let Some(tungstenite::protocol::CloseFrame { code, reason }) = close_frame {
                    tracing::error!(
                        "Kraken rate stream was closed with code {} and reason: {}",
                        code,
                        reason
                    );
                } else {
                    tracing::error!("Kraken rate stream was closed without code and reason");
                }

                return Err(Error::ConnectionClosed);
            }
            msg => {
                tracing::trace!(
                    "Kraken rate stream returned non text message that will be ignored: {}",
                    msg
                );

                return Ok(None);
            }
        };

        let update = match serde_json::from_str::<wire::Event>(&msg) {
            Ok(wire::Event::SystemStatus) => {
                tracing::debug!("Connected to Kraken websocket API");

                return Ok(None);
            }
            Ok(wire::Event::SubscriptionStatus) => {
                tracing::debug!("Subscribed to updates for ticker");

                return Ok(None);
            }
            Ok(wire::Event::Heartbeat) => {
                return Ok(None);
            }
            // if the message is not an event, it is a ticker update or an unknown event
            Err(_) => match serde_json::from_str::<wire::Ticker>(&msg) {
                Ok(ticker) => ticker,
                Err(error) => {
                    tracing::warn!(%msg, "Failed to deserialize message as ticker update. Error {:#}", error);
                    return Ok(None);
                }
            },
        };

        Ok(Some(update))
    }

    #[derive(Debug, thiserror::Error)]
    pub enum Error {
        #[error("The Kraken server closed the websocket connection")]
        ConnectionClosed,
        #[error("Failed to read message from websocket stream")]
        WebSocket(#[from] tungstenite::Error),
        #[error("Failed to parse rate from websocket message")]
        Parse(#[from] wire::Error),
    }

    const XMR_BTC_PAIR: &str = "XMR/XBT";
    const LTC_BTC_PAIR: &str = "LTC/XBT";

    const SUBSCRIBE_XMR_BTC_TICKER_PAYLOAD: &str = r#"
    { "event": "subscribe",
      "pair": [ "XMR/XBT" ],
      "subscription": {
        "name": "ticker"
      }
    }"#;

    const SUBSCRIBE_XMR_LTC_CROSS_TICKER_PAYLOAD: &str = r#"
    { "event": "subscribe",
      "pair": [ "XMR/XBT", "LTC/XBT" ],
      "subscription": {
        "name": "ticker"
      }
    }"#;

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn cross_rate_prices_one_xmr_in_litoshi() {
            // 1 XMR = 0.0044 BTC, 1 LTC sells for 0.0011 BTC => 1 XMR = 4 LTC
            let xmr_btc_ask = bitcoin::Amount::from_btc(0.0044).unwrap();
            let ltc_btc_bid = bitcoin::Amount::from_btc(0.0011).unwrap();

            assert_eq!(
                cross_rate(xmr_btc_ask, ltc_btc_bid),
                Some(bitcoin::Amount::from_btc(4.0).unwrap())
            );
        }

        #[test]
        fn cross_rate_refuses_zero_bid() {
            let xmr_btc_ask = bitcoin::Amount::from_btc(0.0044).unwrap();

            assert_eq!(cross_rate(xmr_btc_ask, bitcoin::Amount::ZERO), None);
        }

        #[test]
        fn cross_rate_emits_only_once_both_legs_ticked() {
            let mut state = CrossRateState::default();

            let xmr_ticker = wire::Ticker {
                pair: Some(XMR_BTC_PAIR.to_string()),
                ask: bitcoin::Amount::from_btc(0.0044).unwrap(),
                bid: bitcoin::Amount::from_btc(0.0043).unwrap(),
            };
            assert_eq!(state.update(&xmr_ticker), None);

            let ltc_ticker = wire::Ticker {
                pair: Some(LTC_BTC_PAIR.to_string()),
                ask: bitcoin::Amount::from_btc(0.0012).unwrap(),
                bid: bitcoin::Amount::from_btc(0.0011).unwrap(),
            };
            assert_eq!(
                state.update(&ltc_ticker),
                Some(bitcoin::Amount::from_btc(4.0).unwrap())
            );

            let unknown_pair = wire::Ticker {
                pair: Some("ETH/XBT".to_string()),
                ask: bitcoin::Amount::from_btc(0.05).unwrap(),
                bid: bitcoin::Amount::from_btc(0.05).unwrap(),
            };
            assert_eq!(state.update(&unknown_pair), None);
        }
    }
}

/// Kraken websocket API wire module.
///
/// Responsible for parsing websocket text messages to events and rate updates.
pub mod wire {
    use super::*;
    use bitcoin::amount::ParseAmountError;
    use serde_json::Value;

    #[derive(Debug, Deserialize, PartialEq, Eq)]
    #[serde(tag = "event")]
    pub enum Event {
        #[serde(rename = "systemStatus")]
        SystemStatus,
        #[serde(rename = "heartbeat")]
        Heartbeat,
        #[serde(rename = "subscriptionStatus")]
        SubscriptionStatus,
    }

    #[derive(Clone, Debug, thiserror::Error)]
    pub enum Error {
        #[error("Data field is missing")]
        DataFieldMissing,
        #[error("Rate Element is of unexpected type")]
        UnexpectedAskRateElementType,
        #[error("Rate Element is missing")]
        MissingAskRateElementType,
        #[error("Failed to parse Bitcoin amount")]
        BitcoinParseAmount(#[from] ParseAmountError),
    }

    /// Represents an update within the price ticker:
    /// 1 XMR priced in the script chain's asset.
    #[derive(Clone, Debug)]
    pub struct PriceUpdate {
        pub ask: bitcoin::Amount,
    }

    /// One ticker message from the websocket, for a single trading pair.
    ///
    /// Kraken sends ticker updates as
    /// `[channelId, {..data..}, "ticker", "XMR/XBT"]`,
    /// so the trailing string names the pair.
    #[derive(Clone, Debug, Deserialize)]
    #[serde(try_from = "TickerUpdate")]
    pub struct Ticker {
        pub pair: Option<String>,
        pub ask: bitcoin::Amount,
        pub bid: bitcoin::Amount,
    }

    #[derive(Debug, Deserialize)]
    #[serde(transparent)]
    pub struct TickerUpdate(Vec<TickerField>);

    #[allow(unused)]
    #[derive(Debug, Deserialize)]
    #[serde(untagged)]
    pub enum TickerField {
        Data(TickerData),
        Metadata(Value),
    }

    #[derive(Debug, Deserialize)]
    pub struct TickerData {
        #[serde(rename = "a")]
        ask: Vec<RateElement>,
        #[serde(rename = "b")]
        bid: Vec<RateElement>,
    }

    #[allow(unused)]
    #[derive(Debug, Deserialize)]
    #[serde(untagged)]
    pub enum RateElement {
        Text(String),
        Number(u64),
    }

    fn parse_rate(elements: &[RateElement]) -> Result<bitcoin::Amount, Error> {
        let rate = elements.first().ok_or(Error::MissingAskRateElementType)?;
        match rate {
            RateElement::Text(rate) => Ok(bitcoin::Amount::from_str_in(
                rate,
                ::bitcoin::Denomination::Bitcoin,
            )?),
            _ => Err(Error::UnexpectedAskRateElementType),
        }
    }

    impl TryFrom<TickerUpdate> for Ticker {
        type Error = Error;

        fn try_from(value: TickerUpdate) -> Result<Self, Error> {
            let data = value
                .0
                .iter()
                .find_map(|field| match field {
                    TickerField::Data(data) => Some(data),
                    TickerField::Metadata(_) => None,
                })
                .ok_or(Error::DataFieldMissing)?;
            let ask = parse_rate(&data.ask)?;
            let bid = parse_rate(&data.bid)?;

            // The pair is the last string element of the message,
            // after the "ticker" channel name.
            let pair = value
                .0
                .iter()
                .filter_map(|field| match field {
                    TickerField::Metadata(Value::String(text)) => Some(text),
                    _ => None,
                })
                .next_back()
                .filter(|text| *text != "ticker")
                .cloned();

            Ok(Ticker { pair, ask, bid })
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn can_deserialize_system_status_event() {
            let event = r#"{"connectionID":14859574189081089471,"event":"systemStatus","status":"online","version":"1.8.1"}"#;

            let event = serde_json::from_str::<Event>(event).unwrap();

            assert_eq!(event, Event::SystemStatus)
        }

        #[test]
        fn can_deserialize_subscription_status_event() {
            let event = r#"{"channelID":980,"channelName":"ticker","event":"subscriptionStatus","pair":"XMR/XBT","status":"subscribed","subscription":{"name":"ticker"}}"#;

            let event = serde_json::from_str::<Event>(event).unwrap();

            assert_eq!(event, Event::SubscriptionStatus)
        }

        #[test]
        fn deserialize_ticker_update() {
            let message = r#"[980,{"a":["0.00440700",7,"7.35318535"],"b":["0.00440200",7,"7.57416678"],"c":["0.00440700","0.22579000"],"v":["273.75489000","4049.91233351"],"p":["0.00446205","0.00441699"],"t":[123,1310],"l":["0.00439400","0.00429900"],"h":["0.00450000","0.00450000"],"o":["0.00449100","0.00433700"]},"ticker","XMR/XBT"]"#;

            let ticker = serde_json::from_str::<Ticker>(message).unwrap();

            assert_eq!(ticker.pair.as_deref(), Some("XMR/XBT"));
            assert_eq!(ticker.ask, bitcoin::Amount::from_btc(0.004407).unwrap());
            assert_eq!(ticker.bid, bitcoin::Amount::from_btc(0.004402).unwrap());
        }
    }
}
