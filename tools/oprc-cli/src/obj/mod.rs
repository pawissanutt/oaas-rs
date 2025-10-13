use std::process;

use crate::config::ContextManager;
use crate::types::{
    ConnectionArgs, InvokeOperation, ObjectOperation, ResultOperation,
};

mod grpc;
mod util;
mod zenoh;

/// Resolve class ID from explicit value or context
async fn resolve_class_id(cls_id: &Option<String>) -> anyhow::Result<String> {
    if let Some(cls) = cls_id {
        return Ok(cls.clone());
    }

    // Try to load from context
    match ContextManager::new().await {
        Ok(manager) => {
            if let Some(context) = manager.get_current_context() {
                if let Some(default_class) = &context.default_class {
                    return Ok(default_class.clone());
                }
            }
        }
        Err(_) => {
            // Context loading failed, continue to error
        }
    }

    Err(anyhow::anyhow!(
        "Class ID not provided and no default class found in context. \
         Use --cls-id or set a default class with 'oprc-cli context set --cls <class_name>'"
    ))
}

pub async fn handle_obj_ops(
    opt: &ObjectOperation,
    conn: &ConnectionArgs,
    use_grpc: bool,
) {
    if use_grpc {
        grpc::handle_obj_ops(opt, conn).await;
    } else {
        zenoh::handle_obj_ops(opt, conn).await;
    }
}

pub async fn handle_invoke_ops(
    opt: &InvokeOperation,
    connect: &ConnectionArgs,
    use_grpc: bool,
) {
    if opt.async_mode {
        // Handle async invocations
        let result = if use_grpc {
            // For now, async is only supported over Zenoh
            eprintln!("Async invocations are not supported over gRPC");
            process::exit(1);
        } else {
            zenoh::invoke_fn_async(opt, connect).await
        };

        match result {
            Ok(invocation_id) => {
                println!("Async invocation submitted successfully");
                println!("Invocation ID: {}", invocation_id);
                println!(
                    "Use 'oprc-cli result <invocation_id>' to retrieve results"
                );
            }
            Err(err) => {
                eprintln!("Failed to submit async invocation: {:?}", err);
                process::exit(1);
            }
        }
    } else {
        // Handle sync invocations (existing logic)
        let res = if use_grpc {
            grpc::invoke_fn(opt, connect).await
        } else {
            zenoh::invoke_fn_sync(opt, connect).await
        };

        match res {
            Ok(resp) => {
                if opt.print_all {
                    println!(
                        "status: {:?}",
                        oprc_grpc::ResponseStatus::try_from(resp.status)
                            .expect("Invalid status")
                    );
                    println!("headers: {:?}", resp.headers);
                    println!("======= payload =======");
                }
                if let Some(b) = &resp.payload {
                    let str_resp = String::from_utf8_lossy(b);
                    print!("{}\n", str_resp);
                } else {
                    println!("{:?}", resp);
                }
            }
            Err(err) => {
                eprintln!("Failed to invoke function: {:?}", err);
                process::exit(1);
            }
        }
    }
}

pub async fn handle_result_ops(
    opt: &ResultOperation,
    connect: &ConnectionArgs,
    use_grpc: bool,
) {
    if use_grpc {
        eprintln!("Result retrieval over gRPC is not yet implemented");
        process::exit(1);
    }

    let result = zenoh::get_async_result(opt, connect).await;
    match result {
        Ok(Some(resp)) => {
            if opt.print_all {
                println!(
                    "status: {:?}",
                    oprc_grpc::ResponseStatus::try_from(resp.status)
                        .expect("Invalid status")
                );
                println!("headers: {:?}", resp.headers);
                println!("invocation_id: {}", resp.invocation_id);
                println!("======= payload =======");
            }
            if let Some(b) = &resp.payload {
                let str_resp = String::from_utf8_lossy(b);
                print!("{}\n", str_resp);
            } else {
                println!("{:?}", resp);
            }
        }
        Ok(None) => {
            println!(
                "Result not yet available for invocation: {}",
                opt.invocation_id
            );
        }
        Err(err) => {
            eprintln!("Failed to get async result: {:?}", err);
            process::exit(1);
        }
    }
}
