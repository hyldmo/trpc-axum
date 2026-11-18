import type { TRPCClient } from '@trpc/client';
import type { AppRouter } from './generated/router.js';

declare const client: TRPCClient<AppRouter>;
const value: Promise<{ value: number }> = client.counter.value.query();
const updated: Promise<{ value: number }> = client.counter.add.mutate({ by: 1 });

// @ts-expect-error The Rust input requires a number.
client.counter.add.mutate({ by: '1' });
// @ts-expect-error Mutations do not have a query method.
client.counter.add.query({ by: 1 });
// @ts-expect-error Only registered procedures appear on the client.
client.counter.reset.mutate();
// @ts-expect-error The response contains a numeric value.
const wrong: Promise<{ value: string }> = value;
