mod conf;
mod conn;
mod error;
mod id;
mod route;
mod rpc;
use std::{sync::Arc, time::Duration};

use axum::{
    body::Bytes,
    extract::{Path, State},
    routing::post,
    Router,
};
pub use conf::Config;
use conn::{ConnManager, PoolConfig};
use error::GatewayError;
use id::parse_id;
use oprc_pb::{InvocationRequest, ObjectInvocationRequest};
use route::{Routable, RoutingManager};
use rpc::RpcManager;
use tracing::{info, warn};

struct RouteState {
    conn_manager: ConnManager<Routable, RpcManager>,
}

impl RouteState {
    pub async fn new(config: Config) -> Result<Arc<Self>, GatewayError> {
        let routing_manager = Arc::new(RoutingManager::new());
        let conf = PoolConfig {
            max_open: config.max_pool_size as u64,
            max_idle_lifetime: Some(Duration::from_secs(30)),
            ..Default::default()
        };
        let conn_manager =
            ConnManager::new_with_builder(routing_manager.clone(), conf);

        routing_manager.start_sync(&config.pm_uri).await?;
        Ok(Arc::new(Self { conn_manager }))
    }
}

pub async fn start_server(
    config: Config,
) -> Result<(), Box<dyn std::error::Error>> {
    let route_state = RouteState::new(config.clone()).await?;
    if config.print_pool_state_interval > 0 {
        let sleep_time = config.print_pool_state_interval;
        let rs = route_state.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_secs(sleep_time as u64))
                    .await;
                let states = rs.conn_manager.get_states().await;
                info!("pool state {:?}", states);
            }
        });
    }
    let app = Router::new()
        .route(
            "/api/class/:cls/objects/:oid/invokes/:func",
            post(invoke_obj),
        )
        .route("/api/class/:cls/invokes/:func", post(invoke_fn))
        .with_state(route_state);
    let listener =
        tokio::net::TcpListener::bind(format!("0.0.0.0:{}", config.http_port))
            .await?;
    info!("start server on port {}", config.http_port);
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
            let resp = resp.into_inner();
            if let Some(playload) = resp.payload {
                Ok(playload)
            } else {
                Ok(Bytes::new())
            }
        }
        Err(e) => Err(GatewayError::from(e)),
    }
}

async fn invoke_fn(
    Path(path): Path<FunctionPathParams>,
    State(state): State<Arc<RouteState>>,
    body: Bytes,
) -> Result<Bytes, GatewayError> {
    let routable = Routable {
        cls: path.cls.clone(),
        func: path.func.clone(),
    };
    let result_conn = state.conn_manager.get(routable).await;
    match result_conn {
        Ok(mut conn) => {
            let req = InvocationRequest {
                cls_id: path.cls.clone(),
                fn_id: path.func.clone(),
                payload: body,
                ..Default::default()
            };
            let result = conn.invoke_fn(req).await;
            match result {
                Ok(resp) => {
                    let resp = resp.into_inner();
                    if let Some(playload) = resp.payload {
                        Ok(playload)
                    } else {
                        Ok(Bytes::new())
                    }
                }
                Err(e) => {
                    warn!("gateway error: {:?}", e);
                    Err(GatewayError::from(e))
                }
            }
        }
        Err(e) => {
            // warn!("gateway pool error: {:?}", e);
            Err(e.into())
        }
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
