use std::error::Error;

use goose::goose::GooseUser;
use goose::goose::TransactionResult;
use goose::prelude::Scenario;
use goose::prelude::Transaction;
use goose::{scenario, transaction, GooseAttack};

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    GooseAttack::initialize()?
        // In this example, we only create a single scenario, named "WebsiteUser".
        .register_scenario(
            scenario!("invoke echo")
                .register_transaction(transaction!(send_req)),
        )
        .execute()
        .await?;
    Ok(())
}

/// Demonstrates how to log in when a user starts. We flag this transaction as an
/// on_start transaction when registering it above. This means it only runs one time
/// per user, when the user thread first starts.
async fn send_req(user: &mut GooseUser) -> TransactionResult {
    let _resp = user
        .post("/api/class/example/invokes/echo", "test".as_bytes())
        .await?;
    Ok(())
}
