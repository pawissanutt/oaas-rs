mod conf;
mod conn;
mod error;
mod func_registry;
mod id;
mod rpc;
use std::sync::Arc;

use axum::{
    body::Bytes,
    extract::{Path, State},
    routing::post,
    Router,
};
use conn::ConnManager;
use envconfig::Envconfig;
use error::GatewayError;
use id::parse_id;
use oprc_proto::ObjectInvocationRequest;
use rpc::RpcManager;

#[derive(Hash, Clone, PartialEq, Eq, Debug)]
struct Routable {
    cls: String,
    func: String,
}

#[derive(Debug)]
struct RouteState {
    conn_manager: ConnManager<Routable, RpcManager>,
}

impl RouteState {
    pub fn new() -> Self {
        let conn = ConnManager::new(|routable: Routable| RpcManager::new(&routable.func));
        Self { conn_manager: conn }
    }
}

pub async fn start_server() -> Result<(), Box<dyn std::error::Error>> {
    let config = conf::Config::init_from_env()?;
    let conn_manager = Arc::new(RouteState::new());
    let app = Router::new()
        .route(
            "/api/class/:cls/objects/:oid/invokes/:func",
            post(invoke_obj),
        )
        .with_state(conn_manager);
    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{}", config.http_port)).await?;
    axum::serve(listener, app).await.unwrap();
    Ok(())
}

async fn invoke_obj(
    Path(path): Path<ObjectPathParams>,
    State(state): State<Arc<RouteState>>,
    body: Bytes,
) -> Result<Bytes, GatewayError> {
    let routable = Routable {
        cls: path.cls.clone(),
        func: path.func.clone(),
    };
    let mut conn = state.conn_manager.get(routable).await?;
    let (pid, oid) = parse_id(&path.oid)?;
    let req = ObjectInvocationRequest {
        partition_id: pid as i32,
        object_id: Some(oid),
        cls_id: path.cls.clone(),
        fn_id: path.func.clone(),
        payload: body,
        ..Default::default()
    };
    let result = conn.invoke_obj(req).await;
    match result {
        Ok(resp) => {
            if let Some(playload) = resp.into_inner().payload {
                Ok(playload)
            } else {
                Ok(Bytes::new())
            }
        }
        Err(e) => Err(GatewayError::from(e)),
    }
}

#[derive(serde::Deserialize)]
struct ObjectPathParams {
    cls: String,
    oid: String,
    func: String,
}

#[derive(serde::Deserialize)]
struct FunctionPathParams {
    cls: String,
    func: String,
}
