use crate::maia::OliviaData;
use async_trait::async_trait;
use daemon::command;
use daemon::oracle;
use model::olivia;
use model::olivia::BitMexPriceEventId;
use model::OrderId;
use std::sync::Arc;
use time::OffsetDateTime;
use tokio::sync::Mutex;
use xtra_productivity::xtra_productivity;

/// Test Stub simulating the Oracle actor.
/// Serves as an entrypoint for injected mock handlers.
pub struct OracleActor {
    mock: Arc<Mutex<MockOracle>>,
}

impl OracleActor {
    pub fn new(executor: command::Executor) -> (Self, Arc<Mutex<MockOracle>>) {
        let mock = Arc::new(Mutex::new(MockOracle::new(executor)));
        let actor = Self { mock: mock.clone() };

        (actor, mock)
    }
}

#[async_trait]
impl xtra::Actor for OracleActor {
    type Stop = ();

    async fn stopped(self) -> Self::Stop {}
}

#[xtra_productivity(message_impl = false)]
impl OracleActor {
    async fn handle(
        &mut self,
        msg: oracle::GetAnnouncement,
    ) -> Result<olivia::Announcement, oracle::NoAnnouncement> {
        self.mock
            .lock()
            .await
            .announcement
            .clone()
            .ok_or(oracle::NoAnnouncement(msg.0))
    }

    async fn handle(&mut self, _msg: oracle::MonitorAttestation) {}

    async fn handle(&mut self, _msg: oracle::SyncAnnouncements) {}

    async fn handle(&mut self, _msg: oracle::SyncAttestations) {}
}
pub struct MockOracle {
    executor: command::Executor,
    announcement: Option<olivia::Announcement>,
}

impl MockOracle {
    fn new(executor: command::Executor) -> Self {
        Self {
            executor,
            announcement: None,
        }
    }

    pub async fn simulate_attestation(&mut self, id: OrderId, attestation: oracle::Attestation) {
        self.executor
            .execute(id, |cfd| cfd.decrypt_cet(&attestation.into_inner()))
            .await
            .unwrap();
    }

    pub fn set_announcement(&mut self, announcement: olivia::Announcement) {
        self.announcement = Some(announcement);
    }
}

/// We do *not* depend on the current time in our tests, the valid combination of
/// announcement/attestation is hard-coded in OliviaData struct (along with event id's).
/// Therefore, an attestation based on current utc time will always be wrong.
pub fn dummy_wrong_attestation() -> oracle::Attestation {
    let olivia::Attestation {
        id: _,
        price,
        scalars,
    } = OliviaData::example_0().attestation().into_inner();

    oracle::Attestation::new(olivia::Attestation {
        id: BitMexPriceEventId::with_20_digits(OffsetDateTime::now_utc()),
        price,
        scalars,
    })
}
