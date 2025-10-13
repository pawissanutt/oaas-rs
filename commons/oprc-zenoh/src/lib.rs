use std::str::FromStr;

pub use envconfig::Envconfig;
use zenoh_config::{ModeDependentValue, WhatAmI};

pub mod pool;
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

    #[envconfig(from = "OPRC_ZENOH_DEFAULT_TIMEOUT")]
    pub default_query_timout: Option<u64>,

    #[envconfig(from = "OPRC_ZENOH_SCOUTING_MULTICAST_ENABLED")]
    pub scouting_multicast_enabled: Option<bool>,

    #[envconfig(from = "OPRC_ZENOH_ADMINSPACE_ENABLED")]
    pub adminspace_enabled: Option<bool>,

    #[envconfig(from = "OPRC_ZENOH_CONFIG")]
    pub json: Option<String>,
}

impl Default for OprcZenohConfig {
    fn default() -> Self {
        Self {
            zenoh_port: 0,
            protocol: None,
            peers: None,
            mode: WhatAmI::Peer,
            gossip_enabled: None,
            linkstate: false,
            max_sessions: 4096,
            max_links: 16,
            buffer_size: None,
            scouting_multicast_enabled: None,
            gossip_multihop: None,
            default_query_timout: None,
            adminspace_enabled: None,
            json: None,
        }
    }
}

impl OprcZenohConfig {
    pub fn create_zenoh(&self) -> zenoh::Config {
        let mut conf = if let Some(json) = &self.json {
            zenoh::Config::from_json5(json).expect("Invalid zenoh config json5")
        } else {
            zenoh::Config::default()
        };
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
        if self.gossip_enabled.is_some() {
            conf.scouting
                .gossip
                .set_enabled(self.gossip_enabled)
                .unwrap();
        }

        if self.gossip_multihop.is_some() {
            conf.scouting
                .gossip
                .set_multihop(self.gossip_multihop)
                .unwrap();
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

        if let Some(t) = self.default_query_timout {
            conf.set_queries_default_timeout(Some(t)).unwrap();
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

        if let Some(enabled) = self.adminspace_enabled {
            conf.adminspace.set_enabled(enabled).unwrap();
        }
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
