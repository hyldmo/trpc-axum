# trpc-axum

Call Rust query and mutation handlers with the standard tRPC v11 TypeScript client. The same Rust router serves HTTP through Axum and generates an `AppRouter` declaration for the client.

## Try the counter

With Rust, Node.js 24, and npm installed, run these commands from the checkout.

```sh
npm ci
npm run bindings
cargo run --locked --example counter
```

The example listens on `127.0.0.1:3000` and prints its URL. Use the generated declaration in a TypeScript client.

```ts
import { createTRPCClient, httpBatchLink } from '@trpc/client';
import type { AppRouter } from './tests/generated/router.js';

const client = createTRPCClient<AppRouter>({
  links: [httpBatchLink({ url: 'http://127.0.0.1:3000/trpc' })],
});

const count = await client.counter.add.mutate({ by: 2 });
console.log(count.value);
```

## Add a procedure

Add `trpc-axum` from this Git repository to an Axum 0.8 app. Procedure inputs implement `serde::Deserialize` and `ts_rs::TS`. Outputs implement `serde::Serialize` and `ts_rs::TS`. Rust primitives such as `String` already implement these traits. The handler receives the Axum request parts, including headers and middleware extensions.

```rust
use trpc_axum::Router;

let rpc = Router::new().query("greet", |_, name: String| async move {
    Ok(format!("Hello, {name}!"))
});

rpc.export_types("bindings/router.d.ts")?;
let app = axum::Router::new().nest("/trpc", rpc.into_axum());
```

Run `export_types` during development or a build. The generated file uses type-only imports from `@trpc/server`. Install matching v11 versions of `@trpc/client` and `@trpc/server` in the TypeScript project. The [counter example](examples/counter.rs) shows structs, nested routes, shared state, and a separate command for generating bindings.

## HTTP compatibility

- `httpLink` and `httpBatchLink` use tRPC’s default JSON format. Queries use GET and mutations use POST. Batches support up to 32 procedures, preserve request order in the response, and return HTTP 207 for mixed statuses.
- Rust `()` input generates TypeScript `void`, so a query with no input can be called as `.query()`. Unit output serializes as `null`. JSON integers are JavaScript numbers on the client. Use strings for integers that require precision above `Number.MAX_SAFE_INTEGER`.
- Return `RpcError` from a handler to set a tRPC error code. The client can read the code and procedure path from `TRPCClientError.data`.

`npm test` regenerates the types, checks the TypeScript call sites, and runs both official HTTP links against a live Rust server. The client version is pinned in [package.json](package.json).
