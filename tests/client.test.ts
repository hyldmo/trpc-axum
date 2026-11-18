import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { once } from 'node:events';
import { createInterface } from 'node:readline';
import { test, type TestContext } from 'node:test';
import { fileURLToPath } from 'node:url';
import { createTRPCClient, httpBatchLink, httpLink, TRPCClientError } from '@trpc/client';
import type { AppRouter } from './generated/router.js';

async function startServer(t: TestContext): Promise<string> {
    const binary = process.platform === 'win32' ? 'counter.exe' : 'counter';
    const path = fileURLToPath(new URL('../target/debug/examples/' + binary, import.meta.url));
    const child = spawn(path, [], { env: { ...process.env, PORT: '0' }, stdio: ['ignore', 'pipe', 'inherit'] });
    const lines = createInterface({ input: child.stdout });
    const exited = once(child, 'exit');
    t.after(async () => {
        lines.close();
        if (child.exitCode === null && child.signalCode === null) child.kill();
        await exited;
    });
    const address = once(lines, 'line', { signal: AbortSignal.timeout(10_000) });
    const [url] = await Promise.race([
        address,
        exited.then(([code]) => { throw new Error('Counter exited during startup: ' + code); }),
    ]);
    return String(url);
}

test('the stock HTTP client calls nested procedures and receives typed errors', async t => {
    const url = await startServer(t);
    const api = createTRPCClient<AppRouter>({ links: [httpLink({ url })] });
    assert.deepEqual(await api.counter.value.query(), { value: 0 });
    assert.deepEqual(await api.counter.add.mutate({ by: 4 }), { value: 4 });

    await assert.rejects(api.counter.add.mutate({ by: 2_147_483_647 }), error => {
        assert.ok(error instanceof TRPCClientError);
        assert.equal(error.data?.code, 'BAD_REQUEST');
        assert.equal(error.data?.path, 'counter.add');
        return true;
    });

    const wrongMethod = await fetch(url + '/counter.add?input=' + encodeURIComponent('{"by":10}'));
    assert.equal(wrongMethod.status, 405);
    assert.deepEqual(await api.counter.value.query(), { value: 4 });
});

test('the stock batch client gets ordered results and independent errors', async t => {
    const url = await startServer(t);
    const api = createTRPCClient<AppRouter>({ links: [httpBatchLink({ url })] });
    const [ok, bad] = await Promise.allSettled([
        api.counter.add.mutate({ by: 7 }),
        api.counter.add.mutate({ by: 2.5 }),
    ]);

    assert.deepEqual(ok, { status: 'fulfilled', value: { value: 7 } });
    assert.equal(bad.status, 'rejected');
    assert.ok(bad.reason instanceof TRPCClientError);
    assert.equal(bad.reason.data?.code, 'BAD_REQUEST');
    const response = bad.reason.meta?.response;
    assert.ok(response instanceof Response);
    assert.equal(response.status, 207);
    assert.deepEqual(await Promise.all([api.counter.value.query(), api.counter.value.query()]), [
        { value: 7 }, { value: 7 },
    ]);
});
