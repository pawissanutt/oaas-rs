use crate::ConnectionArgs;

pub async fn handle_liveliness(conn: &ConnectionArgs) {
    let session = conn.open_zenoh().await;
    match session.liveliness().get("oprc/**").await {
        Ok(handler) => {
            while let Ok(l) = handler.recv_async().await {
                println!("Liveliness: {l:?}");
            }
        }
        Err(err) => {
            eprint!("Failed to get liveliness: {err:?}");
        }
    };
}
