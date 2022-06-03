use crate::identify::protocol;
use crate::identify::PROTOCOL;
use crate::Environment;
use std::collections::HashMap;
use tokio_tasks::Tasks;
use xtra::async_trait;
use xtra::Address;
use xtra::Context;
use xtra_libp2p::endpoint;
use xtra_libp2p::libp2p::PeerId;
use xtra_libp2p::Endpoint;
use xtra_libp2p::OpenSubstream;
use xtra_productivity::xtra_productivity;
use xtras::spawner;
use xtras::spawner::SpawnFallible;
use xtras::SendAsyncSafe;

// TODO: Move NUM_CONNECTIONS_GAUGE to a shared crate and then use it here and

pub struct Actor {
    endpoint: Address<Endpoint>,
    tasks: Tasks,
    spawner: Option<Address<spawner::Actor>>,
    identities: HashMap<PeerId, IdentityInfo>,
}

impl Actor {
    pub fn new(endpoint: Address<Endpoint>) -> Self {
        Self {
            endpoint,
            tasks: Tasks::default(),
            spawner: None,
            identities: HashMap::default(),
        }
    }
}

#[async_trait]
impl xtra::Actor for Actor {
    type Stop = ();

    async fn started(&mut self, _ctx: &mut Context<Self>) {
        self.spawner = Some(spawner::Actor::new().create(None).spawn(&mut self.tasks));
    }

    async fn stopped(self) -> Self::Stop {}
}

pub(crate) struct GetIdentifyInfo(pub PeerId);

pub(crate) struct PeerIdentityReceived {
    peer_id: PeerId,
    peer_identity: protocol::IdentifyMsg,
}

#[derive(Debug, Clone)]
pub struct IdentityInfo {
    pub wire_version: String,
    pub daemon_version: String,
    pub environment: Environment,
}

#[derive(Debug, thiserror::Error)]
#[error("Conversion to identity info failed: {error}")]
pub struct ConversionError {
    #[source]
    error: anyhow::Error,
}

impl TryFrom<protocol::IdentifyMsg> for IdentityInfo {
    type Error = ConversionError;

    fn try_from(identity_msg: protocol::IdentifyMsg) -> Result<Self, Self::Error> {
        let identity_info = IdentityInfo {
            wire_version: identity_msg.wire_version(),
            daemon_version: identity_msg
                .daemon_version()
                .map_err(|error| ConversionError { error })?,
            environment: identity_msg.environment().into(),
        };

        Ok(identity_info)
    }
}

#[xtra_productivity]
impl Actor {
    async fn handle(&mut self, msg: GetIdentifyInfo) -> Option<IdentityInfo> {
        let peer_id = msg.0;
        self.identities.get(&peer_id).cloned()
    }

    async fn handle(&mut self, msg: PeerIdentityReceived) {
        let peer_id = msg.peer_id;
        let identity_info = match IdentityInfo::try_from(msg.peer_identity.clone()) {
            Ok(identity_info) => identity_info,
            Err(e) => {
                tracing::error!("Peer identity discarded {:?}: {e:#}", msg.peer_identity);
                return;
            }
        };

        tracing::info!(%peer_id, daemon_version=%identity_info.daemon_version, environment=%identity_info.environment, wire_version=%identity_info.wire_version, "New identify message received");
        self.identities.insert(peer_id, identity_info);
    }
}

#[xtra_productivity(message_impl = false)]
impl Actor {
    async fn handle_connections_established(
        &mut self,
        msg: endpoint::ConnectionEstablished,
        ctx: &mut Context<Self>,
    ) {
        let peer_id = msg.peer;
        let endpoint = self.endpoint.clone();
        let this = ctx.address().expect("we are alive");

        let request_peer_identity_fut = async move {
            let stream = endpoint
                .send(OpenSubstream::single_protocol(peer_id, PROTOCOL))
                .await??;

            let peer_identity = protocol::recv(stream).await?;

            this.send(PeerIdentityReceived {
                peer_id,
                peer_identity,
            })
            .await?;

            anyhow::Ok(())
        };

        let err_handler = move |e| async move {
            tracing::debug!(%peer_id, "Identity protocol failed upon request: {e:#}")
        };

        if let Err(e) = self
            .spawner
            .as_ref()
            .expect("some after started")
            .send_async_safe(SpawnFallible::new(request_peer_identity_fut, err_handler))
            .await
        {
            tracing::error!("Failed to spawn identity task: {e:#}");
        };
    }

    async fn handle_connections_dropped(&mut self, msg: endpoint::ConnectionDropped) {
        let peer_id = msg.peer;
        tracing::trace!(%peer_id, "Remove peer-info because connection dropped");
        self.identities.remove(&peer_id);
    }
}
