const CONFIG_ID = "MOCKTIONEER_CONFIG";
const CONFIG_BINDING = `EDGEZERO__STORES__CONFIG__${CONFIG_ID}__NAME`;
const CONFIG_KEY = `EDGEZERO__STORES__CONFIG__${CONFIG_ID}__KEY`;
const SEED_MODE = "MOCKTIONEER_DEMO_SEED";
const SEED_VALUE = "MOCKTIONEER_DEMO_CONFIG_BASE64";

function decodeEnvelope(encoded) {
  const binary = atob(encoded);
  const bytes = Uint8Array.from(binary, (character) => character.charCodeAt(0));
  return new TextDecoder().decode(bytes);
}

export function createDemoWorker(worker) {
  let seedPromise;

  return {
    async fetch(request, env, ctx) {
      if (env[SEED_MODE] === "1") {
        const bindingName = env[CONFIG_BINDING] || "mocktioneer_config";
        const key = env[CONFIG_KEY] || "mocktioneer_config";
        const binding = env[bindingName];
        const encoded = env[SEED_VALUE];
        if (!binding || typeof binding.put !== "function" || !encoded) {
          return new Response("local demo config seeding is not configured", {
            status: 500,
          });
        }

        if (!seedPromise) {
          seedPromise = Promise.resolve()
            .then(() => binding.put(key, decodeEnvelope(encoded)))
            .catch((error) => {
              seedPromise = undefined;
              throw error;
            });
        }

        try {
          await seedPromise;
        } catch {
          return new Response("local demo config seed failed", { status: 500 });
        }
      }

      return new worker(ctx, env).fetch(request);
    },
  };
}
