//! Serve tRPC v11 procedures through an Axum router and export their TypeScript types.

mod bindings;
mod error;
mod http;

pub use error::{ErrorCode, RpcError};

use axum::http::request::Parts;
use futures_util::future::BoxFuture;
use serde::{Serialize, de::DeserializeOwned};
use serde_json::Value;
use std::{collections::BTreeMap, future::Future, path::Path, sync::Arc};
use ts_rs::TS;

/// Request headers, URI and Axum middleware extensions, shared by a batch.
pub type Context = Arc<Parts>;

type Handler = dyn Fn(Context, Value) -> BoxFuture<'static, Result<Value, RpcError>> + Send + Sync;

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Query,
    Mutation,
}

struct Procedure {
    kind: Kind,
    handler: Box<Handler>,
    signature: fn(&mut bindings::Models) -> (String, String),
}

impl Procedure {
    fn new<I, O, F, Fut>(kind: Kind, handler: F) -> Self
    where
        I: DeserializeOwned + TS + 'static,
        O: Serialize + TS + 'static,
        F: Fn(Context, I) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<O, RpcError>> + Send + 'static,
    {
        Self {
            kind,
            signature: bindings::signature::<I, O>,
            handler: Box::new(move |ctx, input| {
                let input = match serde_json::from_value(input) {
                    Ok(value) => value,
                    Err(error) => {
                        return Box::pin(async move {
                            Err(RpcError::new(ErrorCode::BadRequest, error.to_string()))
                        });
                    }
                };
                let result = handler(ctx, input);
                Box::pin(async move {
                    serde_json::to_value(result.await?).map_err(|_| {
                        RpcError::new(
                            ErrorCode::InternalServerError,
                            "Failed to serialize response",
                        )
                    })
                })
            }),
        }
    }
}

enum Entry {
    Procedure(Procedure),
    Router(Router),
}

/// Build a procedure tree, export its client contract, then mount it in Axum.
#[derive(Default)]
pub struct Router {
    entries: BTreeMap<String, Entry>,
}

impl Router {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn query<I, O, F, Fut>(self, name: &str, handler: F) -> Self
    where
        I: DeserializeOwned + TS + 'static,
        O: Serialize + TS + 'static,
        F: Fn(Context, I) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<O, RpcError>> + Send + 'static,
    {
        self.insert(name, Entry::Procedure(Procedure::new(Kind::Query, handler)))
    }

    pub fn mutation<I, O, F, Fut>(self, name: &str, handler: F) -> Self
    where
        I: DeserializeOwned + TS + 'static,
        O: Serialize + TS + 'static,
        F: Fn(Context, I) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<O, RpcError>> + Send + 'static,
    {
        self.insert(
            name,
            Entry::Procedure(Procedure::new(Kind::Mutation, handler)),
        )
    }

    pub fn nest(self, name: &str, router: Router) -> Self {
        self.insert(name, Entry::Router(router))
    }

    fn insert(mut self, name: &str, entry: Entry) -> Self {
        assert!(
            !name.is_empty()
                && name.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'_')
                && !matches!(name, "then" | "call" | "apply"),
            "invalid tRPC path segment: {name}"
        );
        assert!(
            self.entries.insert(name.to_owned(), entry).is_none(),
            "duplicate tRPC path segment: {name}"
        );
        self
    }

    fn procedure(&self, path: &str) -> Option<&Procedure> {
        let mut parts = path.split('.').peekable();
        let mut router = self;
        while let Some(part) = parts.next() {
            match router.entries.get(part)? {
                Entry::Router(inner) => router = inner,
                Entry::Procedure(method) => return parts.peek().is_none().then_some(method),
            }
        }
        None
    }

    /// Write a declaration file exporting AppRouter for the stock tRPC client.
    pub fn export_types(&self, path: impl AsRef<Path>) -> std::io::Result<()> {
        bindings::export(self, path.as_ref())
    }

    pub fn into_axum(self) -> axum::Router {
        http::mount(self)
    }
}
