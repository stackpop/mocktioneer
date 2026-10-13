import worker from "./build/worker/shim.mjs";
import { createDemoWorker } from "./demo-seeder.mjs";

export default createDemoWorker(worker);
