use tokio_util::sync::CancellationToken;
use zenoh::{query::Query, sample::Sample, Session};

#[async_trait::async_trait]
pub trait Handler<T>: Send + Sync + Clone {
    async fn handle(&self, input: T);
}

pub async fn declare_queryable_loop<H>(
    z_session: &Session,
    token: CancellationToken,
    key: String,
    handler: H,
    concurrency: usize,
    channel_size: usize,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>>
where
    H: Handler<Query> + 'static,
{
    let (tx, rx) = flume::bounded(channel_size);
    z_session
        .declare_queryable(key.clone())
        .complete(true)
        .callback(move |query| tx.send(query).unwrap())
        .background()
        .await?;
    tracing::info!("queryable '{}': declare with {} threads", key, concurrency);
    for i in 0..concurrency {
        let local_token = token.clone();
        let local_rx = rx.clone();
        let ke = key.clone();
        let local_handler = handler.clone();
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    query_res = local_rx.recv_async() => match query_res {
                        Ok(query) =>  local_handler.handle(query).await,
                        Err(err) => {
                            tracing::error!("queryable '{}' {}: error: {}", ke, i, err,);
                            break;
                        }
                    },
                    _ = local_token.cancelled() => {
                        break;
                    }
                }
            }
            tracing::info!("queryable '{}' {}: cancelled", ke, i);
        });
    }
    Ok(())
}

pub async fn declare_subscriber_loop<H>(
    z_session: &Session,
    token: CancellationToken,
    key: String,
    handler: H,
    concurrency: usize,
    channel_size: usize,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>>
where
    H: Handler<Sample> + 'static,
{
    let (tx, rx) = flume::bounded(channel_size);
    z_session
        .declare_subscriber(key.clone())
        .callback(move |query| tx.send(query).unwrap())
        .background()
        .await?;
    tracing::info!(
        "subscriber '{}': declare with {} threads",
        key,
        concurrency
    );
    for i in 0..concurrency {
        let local_token = token.clone();
        let local_rx = rx.clone();
        let ke = key.clone();
        let local_handler = handler.clone();
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    query_res = local_rx.recv_async() => match query_res {
                        Ok(query) =>  local_handler.handle(query).await,
                        Err(err) => {
                            tracing::error!("subscriber '{}' {}: error: {}", ke, i, err,);
                            break;
                        }
                    },
                    _ = local_token.cancelled() => {
                        break;
                    }
                }
            }
            tracing::info!("subscriber '{}' {}: cancelled", ke, i);
        });
    }
    Ok(())
}
