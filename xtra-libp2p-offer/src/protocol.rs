use asynchronous_codec::FramedRead;
use asynchronous_codec::FramedWrite;
use asynchronous_codec::JsonCodec;
use asynchronous_codec::JsonCodecError;
use futures::AsyncReadExt;
use futures::AsyncWriteExt;
use futures::SinkExt;
use futures::StreamExt;
use model::MakerOffers;

pub(crate) async fn send<S>(sink: S, offers: Option<MakerOffers>) -> Result<(), JsonCodecError>
where
    S: AsyncWriteExt + Unpin,
{
    let mut framed = FramedWrite::new(sink, JsonCodec::<Option<MakerOffers>, ()>::new());

    framed.send(offers).await?;

    Ok(())
}

pub(crate) async fn recv<S>(stream: S) -> Result<Option<MakerOffers>, ReceiveError>
where
    S: AsyncReadExt + Unpin,
{
    let mut framed = FramedRead::new(stream, JsonCodec::<(), Option<MakerOffers>>::new());

    let offers = framed.next().await.ok_or(ReceiveError::Terminated)??;

    Ok(offers)
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum ReceiveError {
    #[error("The stream has terminated.")]
    Terminated,
    #[error("Failed to decode maker offers.")]
    Decode(#[from] JsonCodecError),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::dummy_maker_offers;
    use sluice::pipe::pipe;

    #[tokio::test]
    async fn can_execute_protocol_with_updated_offers() {
        let (stream, sink) = pipe();

        let maker_offers = dummy_maker_offers();

        let (send_res, recv_res) = tokio::join!(send(sink, maker_offers.clone()), recv(stream));

        assert!(send_res.is_ok());
        assert_eq!(recv_res.unwrap(), maker_offers)
    }

    #[tokio::test]
    async fn can_execute_protocol_with_none_offers() {
        let (stream, sink) = pipe();

        let maker_offers = None;

        let (send_res, recv_res) = tokio::join!(send(sink, maker_offers.clone()), recv(stream));

        assert!(send_res.is_ok());
        assert_eq!(recv_res.unwrap(), maker_offers)
    }
}
