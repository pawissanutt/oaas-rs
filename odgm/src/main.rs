use std::error::Error;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    println!("Hello, world!");
    // let flare =  flare_dht::start_server(FlareOptions{
    //      addr: todo!(),
    //      port: todo!(),
    //      leader: todo!(),
    //      peer_addr: todo!(),
    //      node_id: todo!()
    // }).await.unwrap();

    Ok(())
}
