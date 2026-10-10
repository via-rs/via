use redis::aio::ConnectionManager;
use redis::{PushInfo, PushKind, Value};
use serde::{Serialize, de::DeserializeOwned};
use tokio::sync::{broadcast, mpsc};

use crate::sign::{OurEvent, RawPeerEvent, Signer};

pub(super) struct Dispatcher<T, U> {
    pub(super) fanout: broadcast::Sender<RawPeerEvent<T>>,
    pub(super) inbound: mpsc::Receiver<PushInfo>,
    pub(super) outbound: mpsc::Receiver<OurEvent<T, U>>,
    pub(super) concurrency: usize,
}

pub(super) fn dispatch<T, U>(
    mut redis: ConnectionManager,
    signer: Signer,
    mut trx: Dispatcher<T, U>,
) -> impl Future<Output = ()> + Send
where
    T: DeserializeOwned + Serialize + Send,
    U: DeserializeOwned + Serialize + Send,
{
    let mut send_buf = Vec::with_capacity(trx.concurrency);
    let mut recv_buf = Vec::with_capacity(trx.concurrency);

    async move {
        loop {
            let send_offset = send_buf.len();
            let recv_offset = recv_buf.len();

            tokio::select! {
                // subscriber <- dispatch <- redis
                len @ 1.. = trx.inbound.recv_many(&mut recv_buf, trx.concurrency) => {
                    let Some(buf) = recv_buf.get(recv_offset..recv_offset + len) else {
                        recv_buf = Vec::with_capacity(trx.concurrency);
                        continue;
                    };

                    log!(info, "batch size = {}", len);

                    for payload in buf.iter().filter_map(|push| {
                        let scope = signer.scope();
                        extract_payload(push, scope)
                    }) {
                        match signer.deserialize::<_, U>(payload) {
                            // Event deserialized successfully.
                            Ok(raw_peer_event) => {
                                // Zero subscribers does not terminate the loop.
                                let _ = trx.fanout.send(raw_peer_event);
                            }

                            // Deserialization failed.
                            Err(ref error) => {
                                #[cfg(not(debug_assertions))]
                                let _ = error; // Placeholder for tracing...
                                log!(error, "{}", error);
                            }
                        }
                    }

                    recv_buf.clear();
                }

                // subscriber -> dispatch -> redis
                len @ 1.. = trx.outbound.recv_many(&mut send_buf, trx.concurrency) => {
                    let mut pipeline = redis::pipe();
                    let Some(buf) = send_buf.get(send_offset..send_offset + len) else {
                        // Placeholder for tracing...
                        send_buf = Vec::with_capacity(trx.concurrency);
                        continue;
                    };

                    log!(info, "pipeline size = {}", len);

                    // Pack the batch of messages into the pipeline.
                    buf.iter().fold(&mut pipeline, |pipe, event| {
                        match signer.serialize(event) {
                            Ok((channel, payload)) => pipe
                                .cmd("PUBLISH")
                                .arg(channel)
                                .arg(payload),

                            Err(error) => {
                                #[cfg(not(debug_assertions))]
                                let _ = error; // Placeholder for tracing...
                                log!(error, "{}", &error);
                                pipe
                            }
                        }
                    });

                    // Publish to peers. If an error occurs, log it in debug builds.
                    if let Err(error) = pipeline.exec_async(&mut redis).await {
                        #[cfg(not(debug_assertions))]
                        let _ = error; // Placeholder for tracing...
                        log!(error, "{}", &error);
                    }

                    send_buf.clear();
                }
            }
        }
    }
}

#[inline]
fn extract_payload<'a>(push: &'a PushInfo, scope: &str) -> Option<&'a [u8]> {
    if let PushKind::Message = &push.kind
        && let Some([Value::BulkString(name), Value::BulkString(vec)]) =
            push.data.split_first_chunk().map(|(head, _)| head)
        && str::from_utf8(name).is_ok_and(|utf8| scope == utf8)
    {
        Some(vec.as_ref())
    } else {
        None
    }
}
