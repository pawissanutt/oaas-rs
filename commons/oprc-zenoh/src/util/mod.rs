use flume::Receiver;
use zenoh::{
    Session,
    pubsub::Subscriber,
    query::{Query, Queryable},
    sample::Sample,
};

#[async_trait::async_trait]
pub trait Handler<T>: Send + Clone {
    async fn handle(&self, input: T);
}

pub async fn declare_managed_queryable<H>(
    z_session: &Session,
    key: String,
    handler: H,
    concurrency: usize,
    channel_size: usize,
) -> Result<Queryable<Receiver<Query>>, Box<dyn std::error::Error + Send + Sync>>
where
    H: Handler<Query> + 'static,
{
    let chan = if channel_size == 0 {
        flume::unbounded()
    } else {
        flume::bounded(channel_size)
    };
    let queryable = z_session
        .declare_queryable(key.clone())
        .complete(true)
        .with(chan)
        .await?;

    tracing::info!("queryable '{}': declare with {} threads", key, concurrency);
    for i in 0..concurrency {
        let local_rx = queryable.handler().clone();
        let ke = key.clone();
        let local_handler = handler.clone();
        tokio::spawn(async move {
            loop {
                match local_rx.recv_async().await {
                    Ok(query) => local_handler.handle(query).await,
                    Err(err) => {
                        tracing::debug!(
                            "queryable '{}' {}: error: {}",
                            ke,
                            i,
                            err,
                        );
                        break;
                    }
                }
            }
        });
    }
    Ok(queryable)
}

pub async fn declare_managed_subscriber<H>(
    z_session: &Session,
    key: String,
    handler: H,
    concurrency: usize,
    channel_size: usize,
) -> Result<
    Subscriber<Receiver<Sample>>,
    Box<dyn std::error::Error + Send + Sync>,
>
where
    H: Handler<Sample> + 'static,
{
    let chan = if channel_size == 0 {
        flume::unbounded()
    } else {
        flume::bounded(channel_size)
    };
    let subscriber =
        z_session.declare_subscriber(key.clone()).with(chan).await?;
    tracing::info!(
        "subscriber '{}': declare with {} threads",
        key,
        concurrency
    );

    for i in 0..concurrency {
        let local_rx = subscriber.handler().clone();
        let ke = key.clone();
        let local_handler = handler.clone();
        tokio::spawn(async move {
            loop {
                match local_rx.recv_async().await {
                    Ok(query) => local_handler.handle(query).await,
                    Err(err) => {
                        tracing::debug!(
                            "subscriber '{}' {}: error: {}",
                            ke,
                            i,
                            err,
                        );
                        break;
                    }
                }
            }
        });
    }
    Ok(subscriber)
}
