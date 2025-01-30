use envconfig::Envconfig;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let conf = oprc_zenoh::OprcZenohConfig::init_from_env().unwrap();
    let conf = conf.create_zenoh();
    // println!("{:?}", conf);
    println!("Scouting...");

    let session = zenoh::open(conf).await.unwrap();
    // let receiver = scout(WhatAmI::Peer | WhatAmI::Router, conf).await.unwrap();

    // let _ = tokio::time::timeout(std::time::Duration::from_secs(3), async {
    //     while let Ok(hello) = receiver.recv_async().await {
    //         println!("{hello}");
    //     }
    // })
    // .await;
    tokio::time::sleep(std::time::Duration::from_secs(3)).await;

    print!("Scouting done\n");

    // stop scouting
    // receiver.stop();
    session.close().await.unwrap();
}
