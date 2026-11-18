use crate::{Context, ErrorCode, Kind, Router, RpcError};
use axum::{
    Json,
    body::Bytes,
    extract::{Path, Query, State, rejection::BytesRejection},
    http::{Method, StatusCode, header, request::Parts},
    response::{IntoResponse, Response},
    routing::get,
};
use futures_util::future::join_all;
use serde::Deserialize;
use serde_json::{Value, json};
use std::sync::Arc;

const MAX_BATCH_SIZE: usize = 32;

pub(crate) fn mount(router: Router) -> axum::Router {
    axum::Router::new()
        .route("/{procedures}", get(handle).post(handle))
        .with_state(Arc::new(router))
}

#[derive(Deserialize)]
struct Params {
    batch: Option<String>,
    input: Option<String>,
}

fn parse_input(parts: &Parts, params: &Params, body: &[u8]) -> Result<Option<Value>, RpcError> {
    let bytes = if parts.method == Method::GET {
        params.input.as_ref().map(|s| s.as_bytes())
    } else {
        let content_type = parts
            .headers
            .get(header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.split(';').next());
        if !content_type.is_some_and(|v| v.trim().eq_ignore_ascii_case("application/json")) {
            return Err(RpcError::new(
                ErrorCode::UnsupportedMediaType,
                "Expected application/json",
            ));
        }
        (!body.is_empty()).then_some(body)
    };
    bytes
        .map(serde_json::from_slice)
        .transpose()
        .map_err(|_| RpcError::new(ErrorCode::ParseError, "Invalid JSON input"))
}

async fn handle(
    State(router): State<Arc<Router>>,
    Path(path): Path<String>,
    parts: Parts,
    body: Result<Bytes, BytesRejection>,
) -> Result<Response, RpcError> {
    if parts.method != Method::GET && parts.method != Method::POST {
        return Err(RpcError::new(
            ErrorCode::MethodNotSupported,
            "Expected GET or POST",
        ));
    }
    let Query(params) = Query::<Params>::try_from_uri(&parts.uri)
        .map_err(|_| RpcError::new(ErrorCode::BadRequest, "Invalid query parameters"))?;
    let batch = params.batch.as_deref() == Some("1");
    let body = body.map_err(|err| {
        if err.status() == StatusCode::PAYLOAD_TOO_LARGE {
            RpcError::new(ErrorCode::PayloadTooLarge, "Request body is too large")
        } else {
            RpcError::new(ErrorCode::BadRequest, "Could not read request body")
        }
    })?;
    let input = parse_input(&parts, &params, &body)?;
    let ctx = Arc::new(parts);
    if !batch {
        let (status, value) = call(&router, &path, ctx, input.unwrap_or(Value::Null)).await;
        return Ok((status, Json(value)).into_response());
    }

    let paths: Vec<_> = path.split(',').take(MAX_BATCH_SIZE + 1).collect();
    if paths.len() > MAX_BATCH_SIZE {
        return Err(RpcError::new(
            ErrorCode::PayloadTooLarge,
            "Too many procedures in batch",
        ));
    }
    let mut inputs = match input {
        None => Default::default(),
        Some(Value::Object(map)) => map,
        _ => {
            return Err(RpcError::new(
                ErrorCode::BadRequest,
                "Expected indexed inputs for batch",
            ));
        }
    };
    let mut kinds = paths
        .iter()
        .filter_map(|p| router.procedure(p).map(|p| p.kind));
    if let Some(first) = kinds.next()
        && kinds.any(|kind| kind != first)
    {
        return Err(RpcError::new(
            ErrorCode::BadRequest,
            "Cannot mix queries and mutations in one batch",
        ));
    }
    let replies = join_all(paths.iter().enumerate().map(|(i, path)| {
        let input = inputs.remove(&i.to_string()).unwrap_or(Value::Null);
        call(&router, path, Arc::clone(&ctx), input)
    }))
    .await;
    let status = replies[0].0;
    let status = if replies.iter().all(|(s, _)| *s == status) {
        status
    } else {
        StatusCode::MULTI_STATUS
    };
    let body = replies.into_iter().map(|(_, v)| v).collect::<Vec<_>>();
    Ok((status, Json(body)).into_response())
}

async fn call(router: &Router, path: &str, ctx: Context, input: Value) -> (StatusCode, Value) {
    let result = match router.procedure(path) {
        None => Err(RpcError::new(ErrorCode::NotFound, "Procedure not found")),
        Some(procedure) => {
            let method = match procedure.kind {
                Kind::Query => Method::GET,
                Kind::Mutation => Method::POST,
            };
            if ctx.method == method {
                (procedure.handler)(ctx, input).await
            } else {
                Err(RpcError::new(
                    ErrorCode::MethodNotSupported,
                    format!("Expected {method} for {path}"),
                ))
            }
        }
    };
    match result {
        Ok(value) => (StatusCode::OK, json!({ "result": { "data": value } })),
        Err(error) => error.envelope(Some(path)),
    }
}
