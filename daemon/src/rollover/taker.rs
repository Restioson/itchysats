use crate::command;
use crate::future_ext::FutureExt;
use crate::oracle;
use crate::oracle::NoAnnouncement;
use crate::rollover;
use crate::rollover::protocol::*;
use crate::shared_protocol::format_expect_msg_within;
use crate::Txid;
use anyhow::Context;
use async_trait::async_trait;
use bdk_ext::keypair;
use futures::SinkExt;
use futures::StreamExt;
use maia_core::secp256k1_zkp::XOnlyPublicKey;
use model::libp2p::PeerId;
use model::olivia;
use model::olivia::BitMexPriceEventId;
use model::Dlc;
use model::OrderId;
use model::Role;
use model::Timestamp;
use std::time::Duration;
use tokio_tasks::Tasks;
use xtra::message_channel::MessageChannel;
use xtra::Address;
use xtra_libp2p::Endpoint;
use xtra_libp2p::OpenSubstream;
use xtra_libp2p::Substream;
use xtra_productivity::xtra_productivity;

/// The duration that the taker waits until a decision (accept/reject) is expected from the maker
///
/// If the maker does not respond within `DECISION_TIMEOUT` seconds then the taker will fail the
/// rollover.
const DECISION_TIMEOUT: Duration = Duration::from_secs(30);

/// One actor to rule all the rollovers
pub struct Actor {
    endpoint: Address<Endpoint>,
    oracle_pk: XOnlyPublicKey,
    get_announcement:
        MessageChannel<oracle::GetAnnouncement, Result<olivia::Announcement, NoAnnouncement>>,
    n_payouts: usize,
    tasks: Tasks,
    executor: command::Executor,
}

#[async_trait]
impl xtra::Actor for Actor {
    type Stop = ();

    async fn stopped(self) -> Self::Stop {}
}

#[derive(Copy, Clone)]
pub struct ProposeRollover {
    pub order_id: OrderId,
    pub maker_peer_id: PeerId,
    pub from_commit_txid: Txid,
    pub from_settlement_event_id: BitMexPriceEventId,
}

impl Actor {
    pub fn new(
        endpoint: Address<Endpoint>,
        executor: command::Executor,
        oracle_pk: XOnlyPublicKey,
        get_announcement: MessageChannel<
            oracle::GetAnnouncement,
            Result<olivia::Announcement, NoAnnouncement>,
        >,
        n_payouts: usize,
    ) -> Self {
        Self {
            endpoint,
            tasks: Tasks::default(),
            executor,
            get_announcement,
            oracle_pk,
            n_payouts,
        }
    }
}

impl Actor {
    async fn open_substream(&self, peer_id: PeerId) -> anyhow::Result<Substream> {
        Ok(self
            .endpoint
            .send(OpenSubstream::single_protocol(
                peer_id.inner(),
                rollover::PROTOCOL,
            ))
            .await
            .context("Endpoint is disconnected")
            .context("Failed to open substream")??)
    }
}

#[xtra_productivity]
impl Actor {
    pub async fn handle(&mut self, msg: ProposeRollover) {
        let ProposeRollover {
            order_id,
            maker_peer_id,
            from_commit_txid,
            from_settlement_event_id,
        } = msg;

        let substream = match self.open_substream(maker_peer_id).await {
            Ok(substream) => substream,
            Err(e) => {
                tracing::error!(%order_id, "Failed to start rollover: {e:#}");
                emit_failed(order_id, e, &self.executor).await;
                return;
            }
        };

        self.tasks.add_fallible(
            {
                let executor = self.executor.clone();
                let get_announcement = self.get_announcement.clone();
                let oracle_pk = self.oracle_pk;
                let n_payouts = self.n_payouts;
                async move {
                    let mut framed = asynchronous_codec::Framed::new(
                        substream,
                        asynchronous_codec::JsonCodec::<DialerMessage, ListenerMessage>::new(),
                    );

                    executor
                        .execute(order_id, |cfd| cfd.start_rollover_taker())
                        .await?;

                    framed
                        .send(DialerMessage::Propose(Propose {
                            order_id,
                            timestamp: Timestamp::now(),
                            from_commit_txid,
                        }))
                        .await
                        .context("Failed to send Msg0")?;

                    match framed
                        .next()
                        .timeout(DECISION_TIMEOUT)
                        .await
                        .with_context(|| {
                            format!(
                                "Maker did not accept/reject within {} seconds.",
                                DECISION_TIMEOUT.as_secs()
                            )
                        })?
                        .context("End of stream while receiving rollover decision from maker")?
                        .context("Failed to decode rollover decision from maker")?
                        .into_decision()?
                    {
                        Decision::Confirm(Confirm {
                            order_id,
                            oracle_event_id,
                            tx_fee_rate,
                            funding_rate,
                            complete_fee,
                        }) => {
                            let (rollover_params, dlc, position) = executor
                                .execute(order_id, |cfd| {
                                    cfd.handle_rollover_accepted_taker(
                                        tx_fee_rate,
                                        funding_rate,
                                        from_settlement_event_id,
                                    )
                                })
                                .await?;

                            let announcement = get_announcement
                                .send(oracle::GetAnnouncement(oracle_event_id))
                                .await
                                .context("Oracle actor disconnected")?
                                .context("Failed to get announcement")?;

                            tracing::info!(%order_id, "Rollover proposal got accepted");

                            let funding_fee = *rollover_params.funding_fee();
                            let complete_fee_before_rollover =
                                rollover_params.complete_fee_before_rollover();
                            let our_role = Role::Taker;
                            let our_position = position;

                            let (rev_sk, rev_pk) = keypair::new(&mut rand::thread_rng());
                            let (publish_sk, publish_pk) = keypair::new(&mut rand::thread_rng());

                            framed
                                .send(DialerMessage::RolloverMsg(Box::new(RolloverMsg::Msg0(
                                    RolloverMsg0 {
                                        revocation_pk: rev_pk,
                                        publish_pk,
                                    },
                                ))))
                                .await
                                .context("Failed to send Msg0")?;

                            let msg0 = framed
                                .next()
                                .timeout(ROLLOVER_MSG_TIMEOUT)
                                .await
                                .with_context(|| {
                                    format_expect_msg_within("Msg0", ROLLOVER_MSG_TIMEOUT)
                                })?
                                .context("Empty stream instead of Msg0")?
                                .context("Unable to decode listener Msg0")?
                                .into_rollover_msg()?
                                .try_into_msg0()?;

                            let punish_params = build_punish_params(
                                our_role,
                                dlc.identity,
                                dlc.identity_counterparty,
                                msg0,
                                rev_pk,
                                publish_pk,
                            );

                            let own_cfd_txs = build_own_cfd_transactions(
                                &dlc,
                                rollover_params,
                                &announcement,
                                oracle_pk,
                                our_position,
                                n_payouts,
                                complete_fee.into(),
                                punish_params,
                            )
                            .await?;

                            framed
                                .send(DialerMessage::RolloverMsg(Box::new(RolloverMsg::Msg1(
                                    RolloverMsg1::from(own_cfd_txs.clone()),
                                ))))
                                .await
                                .context("Failed to send Msg1")?;

                            let msg1 = framed
                                .next()
                                .timeout(ROLLOVER_MSG_TIMEOUT)
                                .await
                                .with_context(|| {
                                    format_expect_msg_within("Msg1", ROLLOVER_MSG_TIMEOUT)
                                })?
                                .context("Empty stream instead of Msg1")?
                                .context("Unable to decode listener Msg1")?
                                .into_rollover_msg()?
                                .try_into_msg1()?;

                            let commit_desc = build_commit_descriptor(punish_params);
                            let (cets, refund_tx) = build_and_verify_cets_and_refund(
                                &dlc,
                                &announcement,
                                oracle_pk,
                                publish_pk,
                                our_role,
                                &own_cfd_txs,
                                &commit_desc,
                                &msg1,
                            )
                            .await?;

                            // reveal revocation secrets to the counterparty
                            framed
                                .send(DialerMessage::RolloverMsg(Box::new(RolloverMsg::Msg2(
                                    RolloverMsg2 {
                                        revocation_sk: dlc.revocation,
                                    },
                                ))))
                                .await
                                .context("Failed to send Msg2")?;

                            let msg2 = framed
                                .next()
                                .timeout(ROLLOVER_MSG_TIMEOUT)
                                .await
                                .with_context(|| {
                                    format_expect_msg_within("Msg2", ROLLOVER_MSG_TIMEOUT)
                                })?
                                .context("Empty stream instead of Msg2")?
                                .context("Unable to decode listener Msg2")?
                                .into_rollover_msg()?
                                .try_into_msg2()?;

                            let revoked_commit = finalize_revoked_commits(
                                &dlc,
                                dlc.commit.1,
                                msg2,
                                complete_fee_before_rollover,
                            )?;

                            let dlc = Dlc {
                                identity: dlc.identity,
                                identity_counterparty: dlc.identity_counterparty,
                                revocation: rev_sk,
                                revocation_pk_counterparty: punish_params
                                    .counterparty_params()
                                    .revocation_pk,
                                publish: publish_sk,
                                publish_pk_counterparty: punish_params
                                    .counterparty_params()
                                    .publish_pk,
                                maker_address: dlc.maker_address,
                                taker_address: dlc.taker_address,
                                lock: dlc.lock.clone(),
                                commit: (own_cfd_txs.commit.0.clone(), msg1.commit, commit_desc),
                                cets,
                                refund: (refund_tx, msg1.refund),
                                maker_lock_amount: dlc.maker_lock_amount,
                                taker_lock_amount: dlc.taker_lock_amount,
                                revoked_commit,
                                settlement_event_id: announcement.id,
                                refund_timelock: rollover_params.refund_timelock,
                            };

                            emit_completed(
                                order_id,
                                dlc,
                                funding_fee,
                                complete_fee.into(),
                                &executor,
                            )
                            .await;
                        }
                        Decision::Reject(_) => {
                            emit_rejected(order_id, &executor).await;
                        }
                    }
                    Ok(())
                }
            },
            {
                let executor = self.executor.clone();
                move |e| async move {
                    emit_failed(order_id, e, &executor).await;
                }
            },
        );
    }
}
