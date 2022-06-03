use crate::future_ext::FutureExt;
use crate::Environment;
use crate::Multiaddr;
use anyhow::Context;
use anyhow::Result;
use asynchronous_codec::FramedRead;
use asynchronous_codec::FramedWrite;
use asynchronous_codec::JsonCodec;
use futures::AsyncReadExt;
use futures::AsyncWriteExt;
use futures::SinkExt;
use futures::StreamExt;
use libp2p_core::identity::Keypair;
use serde::Deserialize;
use serde::Serialize;
use std::time::Duration;

// Start libp2p based protocols from 0.3.0 since the last wire version was 0.2.1
const PROTOCOL_VERSION: &str = "0.3.0";

const TIMEOUT: Duration = Duration::from_secs(5);

// TODO: Byte serialization for compliance with identify protocol
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IdentifyMsg {
    protocol_version: String,
    agent_version: String,
    // TODO: Serialization of the key, not sure what's the best way to achieve this
    // public_key: PublicKey,        // TODO serialize as bytes
    listen_addrs: Vec<Multiaddr>, // TODO serialize as bytes
    observed_addr: Multiaddr,     // TODO serialize as bytes
    protocols: Vec<String>,
}

impl IdentifyMsg {
    pub fn new(
        daemon_version: String,
        environment: Environment,
        _identity: Keypair,
        listen_addrs: Vec<Multiaddr>,
        observed_addr: Multiaddr,
        protocols: Vec<String>,
    ) -> Self {
        let agent_version = format!("itchysats/{}/{}", environment, daemon_version);

        Self {
            protocol_version: PROTOCOL_VERSION.to_string(),
            agent_version,
            listen_addrs,
            observed_addr,
            protocols,
        }
    }

    pub fn daemon_version(&self) -> Result<String> {
        let splitted = self.agent_version.split('/').collect::<Vec<_>>();
        splitted
            .get(2)
            .map(|str| str.to_string())
            .context("Unable to extract daemon version")
    }

    pub fn environment(&self) -> Result<Environment> {
        let splitted = self.agent_version.split('/').collect::<Vec<_>>();
        splitted
            .get(1)
            .map(|str| Environment::from_str_or_unknown(*str))
            .context("Unable to extract daemon version")
    }

    pub fn wire_version(&self) -> String {
        self.protocol_version.clone()
    }
}

pub(crate) async fn recv<S>(stream: S) -> Result<IdentifyMsg>
where
    S: AsyncReadExt + Unpin,
{
    let mut framed = FramedRead::new(stream, JsonCodec::<(), IdentifyMsg>::new());

    let peer_identity = framed
        .next()
        .timeout(TIMEOUT)
        .await
        .context("Waiting for peer identity timed out")?
        .context("Receive identify message failed")?
        .context("Failed to decode peer identity")?;

    Ok(peer_identity)
}

pub(crate) async fn send<S>(stream: S, identify_msg: IdentifyMsg) -> Result<()>
where
    S: AsyncWriteExt + Unpin,
{
    let mut framed = FramedWrite::new(stream, JsonCodec::<IdentifyMsg, ()>::new());
    framed
        .send(identify_msg)
        .await
        .context("Failed to send identify message")?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_daemon_version_and_environment() {
        let msg = IdentifyMsg::new(
            "0.4.3".to_string(),
            Environment::Umbrel,
            Keypair::generate_ed25519(),
            vec![],
            Multiaddr::empty(),
            vec![],
        );

        let daemon_version = msg.daemon_version().unwrap();
        let environment = msg.environment().unwrap();

        assert_eq!(daemon_version, "0.4.3".to_string());
        assert_eq!(environment, Environment::Umbrel);
    }
}
