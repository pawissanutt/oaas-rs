use oprc_offload::proxy::ProxyError;
use rlt::Status;

pub fn setup_runtime(threads: Option<usize>) -> tokio::runtime::Runtime {
    let mut builder = tokio::runtime::Builder::new_multi_thread();
    builder.enable_all();
    if let Some(t) = threads {
        builder.worker_threads(t);
    } else {
        builder.worker_threads(1);
    }
    builder.build().unwrap()
}

pub fn to_status(err: &ProxyError) -> rlt::Status {
    match err {
        ProxyError::NoQueryable(_) => Status::server_error(1),
        ProxyError::RetrieveReplyErr(_) => Status::server_error(2),
        ProxyError::ReplyError(_) => Status::server_error(3),
        ProxyError::DecodeError(_) => Status::client_error(1),
        ProxyError::RequireMetadata => Status::client_error(2),
        ProxyError::KeyErr() => Status::server_error(4),
    }
}
