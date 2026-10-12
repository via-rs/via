use redis::{PushInfo, PushKind, Value::BulkString, aio::ConnectionManager};
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
) -> impl Future<Output = ()>
where
    T: DeserializeOwned + Serialize,
    U: DeserializeOwned + Serialize,
{
    let mut send_buf = Vec::with_capacity(trx.concurrency);
    let mut recv_buf = Vec::with_capacity(trx.concurrency);

    async move {
        loop {
            let send_offset = send_buf.len();
            let recv_offset = recv_buf.len();

            tokio::select! {
                // subscriber -> dispatch -> redis
                len @ 1.. = trx.outbound.recv_many(&mut send_buf, trx.concurrency) => {
                    let Some(events) = send_buf.get(send_offset..send_offset + len) else {
                        send_buf = Vec::with_capacity(trx.concurrency);
                        // Placeholder for tracing...
                        continue;
                    };

                    log!(info(pubsub), "publish pipeline (len = {})", len);

                    // Pack the batch of messages into the pipeline.
                    if let Err(ref error) = events
                        .iter()
                        .fold(&mut redis::pipe(), |pipe, event| {
                            match signer.serialize::<T, U>(event) {
                                Ok((channel, payload)) => {
                                    // Append the command to the pipeline.
                                    pipe.cmd("PUBLISH").arg(channel).arg(payload)
                                }
                                Err(error) => {
                                    // Log the error if #[cfg(debug_assertions)] and return pipe.
                                    log!(error(pubsub = 1, ret = pipe), "{}", &error)
                                }
                            }
                        })
                        .exec_async(&mut redis)
                        .await
                    {
                        log!(error(pubsub = 1), "{}", error);
                    }

                    send_buf.clear();
                }
                // subscriber <- dispatch <- redis
                len @ 1.. = trx.inbound.recv_many(&mut recv_buf, trx.concurrency) => {
                    let Some(events) = recv_buf.get(recv_offset..recv_offset + len) else {
                        recv_buf = Vec::with_capacity(trx.concurrency);
                        continue;
                    };

                    log!(info(pubsub), "receive push (len = {})", len);

                    for push in events {
                        if let PushKind::Message = push.kind
                            && let [BulkString(scope), BulkString(payload), ..] = &*push.data
                        {
                            if str::from_utf8(scope).is_ok_and(|value| value == signer.scope()) {
                                match signer.deserialize::<T, U>(payload) {
                                    // Event deserialized successfully.
                                    Ok(event) => {
                                        // Continue processing each push.
                                        // A subscriber can join in the next nanosecond.
                                        let _ = trx.fanout.send(event);
                                    }
                                    // Deserialization failed.
                                    Err(error) => {
                                        log!(error(pubsub = 1), "{}", &error);
                                    }
                                }
                            }
                        }
                    }

                    recv_buf.clear();
                }
            }
        }
    }
}
