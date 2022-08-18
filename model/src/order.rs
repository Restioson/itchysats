use crate::{AsBlocks, calculate_margin, FundingFee, SETTLEMENT_INTERVAL};
use crate::long_and_short_leverage;
use crate::Amount;
use crate::CfdEvent;
use crate::ContractSymbol;
use crate::EventKind;
use crate::FeeAccount;
use crate::FundingRate;
use crate::Identity;
use crate::Leverage;
use crate::Offer;
use crate::OfferId;
use crate::OpeningFee;
use crate::OrderId;
use crate::PeerId;
use crate::Position;
use crate::Price;
use crate::Role;
use crate::SetupParams;
use crate::TxFeeRate;
use crate::Usd;
use time::Duration;

/// Models an order. An order is a CFD-to-be which does not necessarily have a DLC yet. It does
/// not ever change.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct Order {
    id: OrderId,
    offer_id: OfferId,
    position: Position,
    initial_price: Price,
    initial_funding_rate: FundingRate,
    long_leverage: Leverage,
    short_leverage: Leverage,
    settlement_interval: Duration,
    quantity: Usd,
    counterparty_network_identity: Identity,
    counterparty_peer_id: Option<PeerId>,
    role: Role,
    opening_fee: OpeningFee,
    initial_tx_fee_rate: TxFeeRate,
    contract_symbol: ContractSymbol,
}

impl Order {
    pub fn new(
        id: OrderId,
        offer_id: OfferId,
        position: Position,
        initial_price: Price,
        taker_leverage: Leverage,
        settlement_interval: Duration, /* TODO: Make a newtype that enforces hours only so
                                        * we don't have to deal with precisions in the
                                        * database. */
        role: Role,
        quantity: Usd,
        counterparty_network_identity: Identity,
        counterparty_peer_id: Option<PeerId>,
        opening_fee: OpeningFee,
        initial_funding_rate: FundingRate,
        initial_tx_fee_rate: TxFeeRate,
        contract_symbol: ContractSymbol,
    ) -> Order {
        let (long_leverage, short_leverage) =
            long_and_short_leverage(taker_leverage, role, position);

        Order {
            id,
            offer_id,
            position,
            initial_price,
            long_leverage,
            short_leverage,
            settlement_interval,
            quantity,
            counterparty_network_identity,
            counterparty_peer_id,
            role,
            initial_funding_rate,
            opening_fee,
            initial_tx_fee_rate,
            contract_symbol,
        }
    }

    pub fn from_taken_offer(
        order_id: OrderId,
        offer: &Offer,
        quantity: Usd,
        counterparty_network_identity: Identity,
        counterparty_peer_id: Option<PeerId>,
        role: Role,
        taker_leverage: Leverage,
    ) -> Self {
        let position = match role {
            Role::Maker => offer.position_maker,
            Role::Taker => offer.position_maker.counter_position(),
        };

        Order::new(
            order_id,
            offer.id,
            position,
            offer.price,
            taker_leverage,
            offer.settlement_interval,
            role,
            quantity,
            counterparty_network_identity,
            counterparty_peer_id,
            offer.opening_fee,
            offer.funding_rate,
            offer.tx_fee_rate,
            offer.contract_symbol,
        )
    }

    fn margin(&self) -> Amount {
        match self.position {
            Position::Long => {
                calculate_margin(self.initial_price, self.quantity, self.long_leverage)
            }
            Position::Short => {
                calculate_margin(self.initial_price, self.quantity, self.short_leverage)
            }
        }
    }

    fn counterparty_margin(&self) -> Amount {
        match self.position {
            Position::Long => {
                calculate_margin(self.initial_price, self.quantity, self.short_leverage)
            }
            Position::Short => {
                calculate_margin(self.initial_price, self.quantity, self.long_leverage)
            }
        }
    }

    pub fn start_contract_setup(
        &self,
    ) -> (CfdEvent, SetupParams, Position) {
        let margin = self.margin();
        let counterparty_margin = self.counterparty_margin();

        // TODO(restioson): is this correct?
        // TODO(restioson): extract?
        let initial_funding_fee = FundingFee::calculate(
            self.initial_price(),
            self.quantity(),
            self.long_leverage(),
            self.short_leverage(),
            self.initial_funding_rate(),
            SETTLEMENT_INTERVAL.whole_hours(),
        )
        .expect("values from db to be sane");

        let fee_account = FeeAccount::new(self.position(), self.role())
            .add_opening_fee(self.opening_fee())
            .add_funding_fee(initial_funding_fee);

        (
            CfdEvent::new(self.id, EventKind::ContractSetupStarted),
            SetupParams::new(
                margin,
                counterparty_margin,
                self.counterparty_network_identity,
                self.initial_price,
                self.quantity,
                self.long_leverage,
                self.short_leverage,
                self.refund_timelock_in_blocks(),
                self.initial_tx_fee_rate,
                fee_account,
            ),
            self.position,
        )
    }

    /// A factor to be added to the CFD order settlement_interval for calculating the
    /// refund timelock.
    ///
    /// The refund timelock is important in case the oracle disappears or never publishes a
    /// signature. Ideally, both users collaboratively settle in the refund scenario. This
    /// factor is important if the users do not settle collaboratively.
    /// `1.5` times the settlement_interval as defined in CFD order should be safe in the
    /// extreme case where a user publishes the commit transaction right after the contract was
    /// initialized. In this case, the oracle still has `1.0 *
    /// cfdorder.settlement_interval` time to attest and no one can publish the refund
    /// transaction.
    /// The downside is that if the oracle disappears: the users would only notice at the end
    /// of the cfd settlement_interval. In this case the users has to wait for another
    /// `1.5` times of the settlement_interval to get his funds back.
    const REFUND_THRESHOLD: f32 = 1.5;

    pub fn refund_timelock_in_blocks(&self) -> u32 {
        (self.settlement_interval * Self::REFUND_THRESHOLD)
            .as_blocks()
            .ceil() as u32
    }

    pub fn id(&self) -> OrderId {
        self.id
    }

    pub fn offer_id(&self) -> OfferId {
        self.offer_id
    }

    pub fn position(&self) -> Position {
        self.position
    }

    pub fn initial_price(&self) -> Price {
        self.initial_price
    }

    pub fn taker_leverage(&self) -> Leverage {
        match (self.role, self.position) {
            (Role::Taker, Position::Long) | (Role::Maker, Position::Short) => self.long_leverage,
            (Role::Taker, Position::Short) | (Role::Maker, Position::Long) => self.short_leverage,
        }
    }

    pub fn long_leverage(&self) -> Leverage {
        self.long_leverage
    }

    pub fn short_leverage(&self) -> Leverage {
        self.short_leverage
    }

    pub fn settlement_time_interval_hours(&self) -> Duration {
        self.settlement_interval
    }

    pub fn quantity(&self) -> Usd {
        self.quantity
    }

    pub fn counterparty_network_identity(&self) -> Identity {
        self.counterparty_network_identity
    }

    pub fn counterparty_peer_id(&self) -> Option<PeerId> {
        self.counterparty_peer_id
    }

    pub fn role(&self) -> Role {
        self.role
    }

    pub fn initial_funding_rate(&self) -> FundingRate {
        self.initial_funding_rate
    }

    pub fn initial_tx_fee_rate(&self) -> TxFeeRate {
        self.initial_tx_fee_rate
    }

    pub fn contract_symbol(&self) -> ContractSymbol {
        self.contract_symbol
    }

    pub fn opening_fee(&self) -> OpeningFee {
        self.opening_fee
    }
}
