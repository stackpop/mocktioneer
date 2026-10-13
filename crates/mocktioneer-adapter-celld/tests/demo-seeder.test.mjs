import assert from "node:assert/strict";
import test from "node:test";

import { createDemoWorker } from "../demo-seeder.mjs";

const envelope = JSON.stringify({
  version: 1,
  generated_at: "2026-10-13T00:00:00Z",
  sha256: "e35a5d93c58c8bdabd1a5cada532e85b192f38f5ab3b86a14ba0278d7e2e1d45",
  data: { bid_cpm: 0.37 },
});
const encodedEnvelope = Buffer.from(envelope).toString("base64");
const context = { marker: "WorkerEntrypoint context" };

function environment({ seed = "1", encoded = encodedEnvelope, put } = {}) {
  return {
    MOCKTIONEER_DEMO_SEED: seed,
    MOCKTIONEER_DEMO_CONFIG_BASE64: encoded,
    mocktioneer_config: { put },
  };
}

function workerEntrypoint(fetch) {
  return class WorkerEntrypointFixture {
    constructor(ctx, env) {
      this.ctx = ctx;
      this.env = env;
    }

    async fetch(request) {
      return fetch.call(this, request);
    }
  };
}

test("disabled seeding forwards through the workers-rs entrypoint class", async () => {
  let puts = 0;
  const worker = createDemoWorker(
    workerEntrypoint(async function fetch(request) {
      assert.equal(this.ctx, context);
      assert.equal(this.env.MOCKTIONEER_DEMO_SEED, "0");
      assert.equal(request.url, "http://localhost/");
      return new Response("real worker");
    }),
  );
  const env = environment({
    seed: "0",
    put: async () => {
      puts += 1;
    },
  });

  const response = await worker.fetch(new Request("http://localhost/"), env, context);

  assert.equal(response.status, 200);
  assert.equal(await response.text(), "real worker");
  assert.equal(puts, 0);
});

test("explicit seeding writes the envelope before constructing and dispatching the entrypoint", async () => {
  let stored;
  let dispatched = false;
  const worker = createDemoWorker(
    workerEntrypoint(async function fetch() {
      assert.equal(this.ctx, context);
      assert.equal(stored?.key, "mocktioneer_config");
      assert.equal(stored?.value, envelope);
      dispatched = true;
      return new Response("seeded");
    }),
  );
  const env = environment({
    put: async (key, value) => {
      stored = { key, value };
    },
  });

  const response = await worker.fetch(
    new Request("http://localhost/openrtb2/auction"),
    env,
    context,
  );

  assert.equal(response.status, 200);
  assert.equal(dispatched, true);
});

test("config binding and key overrides match the EdgeZero store selectors", async () => {
  let stored;
  const worker = createDemoWorker(
    workerEntrypoint(async function fetch() {
      assert.equal(stored?.key, "selected-key");
      assert.equal(stored?.value, envelope);
      return new Response("seeded");
    }),
  );
  const env = environment();
  env.EDGEZERO__STORES__CONFIG__MOCKTIONEER_CONFIG__NAME = "selected_binding";
  env.EDGEZERO__STORES__CONFIG__MOCKTIONEER_CONFIG__KEY = "selected-key";
  env.selected_binding = {
    put: async (key, value) => {
      stored = { key, value };
    },
  };

  const response = await worker.fetch(new Request("http://localhost/"), env, context);
  assert.equal(response.status, 200);
});

test("concurrent first requests share one awaited seed operation in the isolate", async () => {
  let puts = 0;
  let release;
  const pendingPut = new Promise((resolve) => {
    release = resolve;
  });
  let dispatched = 0;
  const worker = createDemoWorker(
    workerEntrypoint(async function fetch() {
      assert.equal(this.ctx, context);
      dispatched += 1;
      return new Response("ok");
    }),
  );
  const env = environment({
    put: async (key, value) => {
      puts += 1;
      assert.equal(key, "mocktioneer_config");
      assert.equal(value, envelope);
      await pendingPut;
    },
  });

  const requests = [
    worker.fetch(new Request("http://localhost/first"), env, context),
    worker.fetch(new Request("http://localhost/second"), env, context),
  ];
  await Promise.resolve();
  await Promise.resolve();
  assert.equal(puts, 1);
  assert.equal(dispatched, 0);

  release();
  const responses = await Promise.all(requests);
  assert.deepEqual(responses.map((response) => response.status), [200, 200]);
  assert.equal(dispatched, 2);
});

test("failed seed fails closed and a later request can retry", async () => {
  let puts = 0;
  let dispatched = 0;
  const worker = createDemoWorker(
    workerEntrypoint(async function fetch() {
      dispatched += 1;
      return new Response("must not dispatch on failed seed");
    }),
  );
  const env = environment({
    put: async () => {
      puts += 1;
      if (puts === 1) throw new Error("sensitive backend detail");
    },
  });

  const failed = await worker.fetch(new Request("http://localhost/"), env, context);
  assert.equal(failed.status, 500);
  assert.equal(await failed.text(), "local demo config seed failed");
  assert.equal(dispatched, 0);

  const retried = await worker.fetch(new Request("http://localhost/"), env, context);
  assert.equal(retried.status, 200);
  assert.equal(puts, 2);
  assert.equal(dispatched, 1);
});

test("explicit mode without fixture or KV binding fails before dispatch", async () => {
  let dispatched = 0;
  const worker = createDemoWorker(
    workerEntrypoint(async function fetch() {
      dispatched += 1;
      return new Response("unexpected");
    }),
  );

  const response = await worker.fetch(
    new Request("http://localhost/"),
    { MOCKTIONEER_DEMO_SEED: "1" },
    context,
  );

  assert.equal(response.status, 500);
  assert.equal(dispatched, 0);
});
