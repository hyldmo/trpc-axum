use serde::{Deserialize, Serialize};
use std::{
    env,
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicI32, Ordering},
    },
};
use trpc_axum::{ErrorCode, Router, RpcError};
use ts_rs::TS;

#[derive(Deserialize, TS)]
struct Add {
    by: i32,
}

#[derive(Serialize, TS)]
struct Count {
    value: i32,
}

fn counter() -> Router {
    let value = Arc::new(AtomicI32::new(0));
    let reader = Arc::clone(&value);
    let counter = Router::new()
        .query("value", move |_, (): ()| {
            let value = reader.load(Ordering::Relaxed);
            async move { Ok(Count { value }) }
        })
        .mutation("add", move |_, change: Add| {
            let counter = Arc::clone(&value);
            async move {
                let previous = counter
                    .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |v| {
                        v.checked_add(change.by)
                    })
                    .map_err(|_| RpcError::new(ErrorCode::BadRequest, "Counter overflow"))?;
                Ok(Count {
                    value: previous + change.by,
                })
            }
        });
    Router::new().nest("counter", counter)
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let router = counter();
    let args: Vec<_> = env::args().skip(1).collect();
    if let [flag, dir] = args.as_slice()
        && flag == "--export"
    {
        router.export_types(Path::new(dir).join("router.d.ts"))?;
        return Ok(());
    }
    if !args.is_empty() {
        return Err("usage: counter [--export <directory>]".into());
    }

    let port: u16 = env::var("PORT")
        .unwrap_or_else(|_| "3000".to_owned())
        .parse()?;
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", port)).await?;
    let app = axum::Router::new().nest("/trpc", router.into_axum());
    println!("http://{}/trpc", listener.local_addr()?);
    axum::serve(listener, app).await?;
    Ok(())
}
