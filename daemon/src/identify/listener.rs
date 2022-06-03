use crate::identify::protocol;
use crate::Environment;
use async_trait::async_trait;
use libp2p_core::identity::Keypair;
use libp2p_core::Multiaddr;
use tokio_tasks::Tasks;
use xtra::Address;
use xtra::Context;
use xtra_libp2p::NewInboundSubstream;
use xtra_productivity::xtra_productivity;
use xtras::spawner;
use xtras::spawner::SpawnFallible;
use xtras::SendAsyncSafe;

pub struct Actor {
    tasks: Tasks,
    spawner: Option<Address<spawner::Actor>>,

    daemon_version: String,
    environment: Environment,
    identity: Keypair,
    listen_addrs: Vec<Multiaddr>,
    observed_addr: Multiaddr,
    protocols: Vec<String>,
}

impl Actor {
    pub fn new(
        daemon_version: String,
        environment: Environment,
        identity: Keypair,
        listen_addrs: Vec<Multiaddr>,
        observed_addr: Multiaddr,
        protocols: Vec<String>,
    ) -> Self {
        Self {
            tasks: Default::default(),
            spawner: None,
            daemon_version,
            environment,
            identity,
            listen_addrs,
            observed_addr,
            protocols,
        }
    }
}

#[xtra_productivity(message_impl = false)]
impl Actor {
    async fn handle(&mut self, message: NewInboundSubstream) {
        let NewInboundSubstream { stream, peer } = message;

        let identiyfy_msg = protocol::IdentifyMsg::new(
            self.daemon_version.clone(),
            self.environment.into(),
            self.identity.clone(),
            self.listen_addrs.clone(),
            self.observed_addr.clone(),
            self.protocols.clone(),
        );

        let send_identify_msg_fut = protocol::send(stream, identiyfy_msg);

        let err_handler = move |e| async move {
            tracing::debug!(peer_id=%peer, "Identity protocol failed upon response: {e:#}")
        };

        if let Err(e) = self
            .spawner
            .as_ref()
            .expect("some after started")
            .send_async_safe(SpawnFallible::new(send_identify_msg_fut, err_handler))
            .await
        {
            tracing::error!("Failed to spawn identity task: {e:#}");
        };
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
