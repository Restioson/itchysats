pub mod maker;
mod protocol;
pub mod taker;

pub const PROTOCOL_NAME: &str = "/itchysats/offer/1.0.0";

#[cfg(test)]
mod tests {
    use super::*;
    use crate::taker::LatestMakerOffers;
    use async_trait::async_trait;
    use futures::Future;
    use futures::FutureExt;
    use model::olivia::BitMexPriceEventId;
    use model::FundingRate;
    use model::Leverage;
    use model::MakerOffers;
    use model::OpeningFee;
    use model::Order;
    use model::Origin;
    use model::Position;
    use model::Price;
    use model::TxFeeRate;
    use model::Usd;
    use rust_decimal::Decimal;
    use rust_decimal_macros::dec;
    use std::time::Duration;
    use time::macros::datetime;
    use xtra::spawn::TokioGlobalSpawnExt;
    use xtra::Actor as _;
    use xtra::Address;
    use xtra::Context;
    use xtra_libp2p::endpoint::Subscribers;
    use xtra_libp2p::libp2p::identity::Keypair;
    use xtra_libp2p::libp2p::multiaddr::Protocol;
    use xtra_libp2p::libp2p::transport::MemoryTransport;
    use xtra_libp2p::libp2p::Multiaddr;
    use xtra_libp2p::libp2p::PeerId;
    use xtra_libp2p::Connect;
    use xtra_libp2p::Endpoint;
    use xtra_libp2p::ListenOn;
    use xtra_productivity::xtra_productivity;

    #[tokio::test]
    async fn given_new_offers_then_received_offers_match_originals() {
        tracing_subscriber::fmt()
            .with_env_filter("xtra_libp2p_offer=trace")
            .with_test_writer()
            .init();

        let (maker_peer_id, maker_offer_addr, maker_endpoint_addr) =
            create_endpoint_with_offer_maker();
        let (offer_receiver_addr, taker_endpoint_addr) = create_endpoint_with_offer_taker();

        maker_endpoint_addr
            .send(ListenOn(Multiaddr::empty().with(Protocol::Memory(1000))))
            .await
            .unwrap();
        taker_endpoint_addr
            .send(Connect(
                Multiaddr::empty()
                    .with(Protocol::Memory(1000))
                    .with(Protocol::P2p(maker_peer_id.into())),
            ))
            .await
            .unwrap()
            .unwrap();

        let new_offers = dummy_maker_offers();

        // maker keeps sending the offers until the taker establishes
        // a connection
        #[allow(clippy::disallowed_methods)]
        tokio::spawn({
            let new_offers = new_offers.clone();
            async move {
                loop {
                    maker_offer_addr
                        .send(crate::maker::NewOffers::new(new_offers.clone()))
                        .await
                        .unwrap();

                    tokio::time::sleep(Duration::from_millis(200)).await;
                }
            }
        });

        // taker retries until the connection is established and we
        // get the maker's latest offers
        let received_offers = retry_until_some(|| {
            let offer_receiver_addr = offer_receiver_addr.clone();
            async move {
                offer_receiver_addr
                    .send(GetLatestOffers)
                    .map(|res| res.unwrap())
                    .await
            }
        })
        .await;

        assert_eq!(new_offers, Some(received_offers))
    }

    fn create_endpoint_with_offer_maker(
    ) -> (PeerId, Address<crate::maker::Actor>, Address<Endpoint>) {
        let (endpoint_addr, endpoint_context) = Context::new(None);

        let id = Keypair::generate_ed25519();
        let offer_maker_addr = crate::maker::Actor::new(endpoint_addr.clone())
            .create(None)
            .spawn_global();

        let endpoint = Endpoint::new(
            Box::new(MemoryTransport::default),
            id.clone(),
            Duration::from_secs(10),
            [],
            Subscribers::new(
                vec![offer_maker_addr.clone().into()],
                vec![offer_maker_addr.clone().into()],
                vec![],
                vec![],
            ),
        );

        #[allow(clippy::disallowed_methods)]
        tokio::spawn(endpoint_context.run(endpoint));

        (id.public().to_peer_id(), offer_maker_addr, endpoint_addr)
    }

    fn create_endpoint_with_offer_taker() -> (Address<OffersReceiver>, Address<Endpoint>) {
        let offers_receiver_addr = OffersReceiver::new().create(None).spawn_global();

        let offer_taker_addr = crate::taker::Actor::new(offers_receiver_addr.clone().into())
            .create(None)
            .spawn_global();

        let endpoint_addr = Endpoint::new(
            Box::new(MemoryTransport::default),
            Keypair::generate_ed25519(),
            Duration::from_secs(10),
            [(PROTOCOL_NAME, offer_taker_addr.into())],
            Subscribers::default(),
        )
        .create(None)
        .spawn_global();

        (offers_receiver_addr, endpoint_addr)
    }

    struct OffersReceiver {
        latest_offers: Option<MakerOffers>,
    }

    impl OffersReceiver {
        fn new() -> Self {
            Self {
                latest_offers: None,
            }
        }
    }

    #[async_trait]
    impl xtra::Actor for OffersReceiver {
        type Stop = ();

        async fn stopped(self) -> Self::Stop {}
    }

    #[xtra_productivity(message_impl = false)]
    impl OffersReceiver {
        async fn handle(&mut self, msg: LatestMakerOffers) {
            self.latest_offers = msg.0;
        }
    }

    struct GetLatestOffers;

    #[xtra_productivity]
    impl OffersReceiver {
        async fn handle(&mut self, _: GetLatestOffers) -> Option<MakerOffers> {
            self.latest_offers.clone()
        }
    }

    async fn retry_until_some<F, FUT, T>(mut fut: F) -> T
    where
        F: FnMut() -> FUT,
        FUT: Future<Output = Option<T>>,
    {
        loop {
            match fut().await {
                Some(t) => return t,
                None => tokio::time::sleep(Duration::from_millis(200)).await,
            }
        }
    }

    pub fn dummy_maker_offers() -> Option<MakerOffers> {
        Some(MakerOffers {
            long: Some(dummy_order(Position::Long)),
            short: Some(dummy_order(Position::Short)),
            tx_fee_rate: TxFeeRate::default(),
            funding_rate_long: FundingRate::new(Decimal::ONE).unwrap(),
            funding_rate_short: FundingRate::new(Decimal::NEGATIVE_ONE).unwrap(),
        })
    }

    fn dummy_order(position: Position) -> Order {
        Order::new(
            position,
            Price::new(dec!(1000)).unwrap(),
            Usd::new(dec!(100)),
            Usd::new(dec!(1000)),
            Origin::Ours,
            BitMexPriceEventId::with_20_digits(datetime!(2021-10-04 22:00:00).assume_utc()),
            time::Duration::hours(24),
            TxFeeRate::default(),
            FundingRate::default(),
            OpeningFee::default(),
            vec![Leverage::TWO],
        )
    }
}
