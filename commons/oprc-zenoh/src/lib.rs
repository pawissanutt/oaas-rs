use std::str::FromStr;

use envconfig::Envconfig;
use zenoh_config::{ModeDependentValue, WhatAmI};

pub mod util;

#[derive(Default, Debug, Clone)]
pub struct ServiceIdentifier {
    pub class_id: String,
    pub partition_id: u16,
    pub replica_id: u64,
}

#[derive(Envconfig, Clone, Debug)]
pub struct OprcZenohConfig {
    #[envconfig(from = "OPRC_ZENOH_PORT", default = "0")]
    pub zenoh_port: u16,

    #[envconfig(from = "OPRC_ZENOH_PROTOCOL")]
    pub protocol: Option<String>,

    #[envconfig(from = "OPRC_ZENOH_PEERS")]
    pub peers: Option<String>,

    #[envconfig(from = "OPRC_ZENOH_MODE", default = "peer")]
    pub mode: WhatAmI,

    #[envconfig(from = "OPRC_ZENOH_GOSSIP_ENABLED")]
    pub gossip_enabled: Option<bool>,
    #[envconfig(from = "OPRC_ZENOH_GOSSIP_MULTIHOP")]
    pub gossip_multihop: Option<bool>,

    #[envconfig(from = "OPRC_ZENOH_LINKSTATE", default = "false")]
    pub linkstate: bool,

    #[envconfig(from = "OPRC_ZENOH_MAX_SESSIONS", default = "4096")]
    pub max_sessions: usize,

    #[envconfig(from = "OPRC_ZENOH_MAX_LINKS", default = "16")]
    pub max_links: usize,

    #[envconfig(from = "OPRC_ZENOH_BUFFER_SIZE")]
    pub buffer_size: Option<u64>,

    #[envconfig(from = "OPRC_ZENOH_SCOUTING_MULTICAST_ENABLED")]
    pub scouting_multicast_enabled: Option<bool>,
}

impl Default for OprcZenohConfig {
    fn default() -> Self {
        Self {
            zenoh_port: 0,
            protocol: None,
            peers: None,
            mode: WhatAmI::Peer,
            gossip_enabled: Some(true),
            linkstate: false,
            max_sessions: 4096,
            max_links: 16,
            buffer_size: None,
            scouting_multicast_enabled: None,
            gossip_multihop: None,
        }
    }
}

impl OprcZenohConfig {
    pub fn create_zenoh(&self) -> zenoh::Config {
        let mut conf = zenoh::Config::default();
        let mut listen_conf = zenoh_config::ListenConfig::default();
        let protocol = self.protocol.to_owned().unwrap_or("tcp".into());
        let endpoint = format!("{}/[::]:{}", protocol, self.zenoh_port);
        use zenoh_config::EndPoint;
        listen_conf
            .set_endpoints(zenoh_config::ModeDependentValue::Unique(vec![
                EndPoint::from_str(&endpoint[..]).unwrap(),
            ]))
            .unwrap();

        conf.set_listen(listen_conf).unwrap();
        conf.set_mode(Some(self.mode)).unwrap();
        if Some(true) == self.gossip_enabled {
            conf.scouting.gossip.set_enabled(Some(true)).unwrap();
        }

        if Some(true) == self.gossip_multihop {
            conf.scouting.gossip.set_multihop(Some(true)).unwrap();
        }

        conf.scouting
            .multicast
            .set_enabled(self.scouting_multicast_enabled)
            .unwrap();

        if self.linkstate {
            conf.routing
                .peer
                .set_mode(Some("linkstate".into()))
                .unwrap();
        }

        conf.transport
            .unicast
            .set_max_sessions(self.max_sessions)
            .unwrap();

        conf.transport
            .unicast
            .set_max_links(self.max_links)
            .unwrap();
        if let Some(buffer_size) = self.buffer_size {
            conf.transport
                .link
                .rx
                .set_buffer_size(buffer_size as usize)
                .unwrap();
        }
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
