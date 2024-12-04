use std::str::FromStr;

use envconfig::Envconfig;
use zenoh_config::{ModeDependentValue, WhatAmI};

pub mod rpc;

#[derive(Default, Debug, Clone)]
pub struct ServiceIdentifier {
    pub class_id: String,
    pub partition_id: u32,
    pub replica_id: u32,
}

#[derive(Envconfig, Clone, Debug)]
pub struct OprcZenohConfig {
    #[envconfig(from = "OPRC_ZENOH_PORT", default = "7447")]
    pub zenoh_port: u16,

    #[envconfig(from = "OPRC_ZENOH_PEERS")]
    pub peers: Option<String>,
}

impl OprcZenohConfig {
    pub fn create_zenoh(&self) -> zenoh::Config {
        let mut conf = zenoh::Config::default();
        let mut listen_conf = zenoh_config::ListenConfig::default();
        let endpoint = format!("tcp/[::]:{}", self.zenoh_port);
        use zenoh_config::EndPoint;
        listen_conf
            .set_endpoints(zenoh_config::ModeDependentValue::Unique(vec![
                EndPoint::from_str(&endpoint[..]).unwrap(),
            ]))
            .unwrap();

        conf.set_listen(listen_conf).unwrap();
        conf.set_mode(Some(WhatAmI::Router)).unwrap();
        let mut connect_conf = zenoh_config::ConnectConfig::default();
        if let Some(peers) = &self.peers {
            let mut peer_endpoints = Vec::new();
            for peer in peers.split(",") {
                peer_endpoints.push(EndPoint::from_str(peer).unwrap());
            }
            connect_conf
                .set_endpoints(ModeDependentValue::Unique(peer_endpoints))
                .unwrap();
        };
        conf.set_connect(connect_conf).unwrap();
        conf
    }
}

#[cfg(test)]
mod test {
    use envconfig::Envconfig;

    use crate::OprcZenohConfig;

    async fn _test() {
        let conf = OprcZenohConfig::init_from_env().unwrap();
        let session = zenoh::open(conf.create_zenoh()).await.unwrap();
        let queryable = session.declare_queryable("test").await.unwrap();
        let _req = queryable.recv_async().await.unwrap();
    }
}
