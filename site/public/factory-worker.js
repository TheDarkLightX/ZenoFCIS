import { instantiate } from "./demo-module.js";
import { runFactory, inspectInventory } from "./factory-runner.js";
let module;
self.onmessage = async ({ data: { id, identity, act, options, kind, mutant, ordinal } }) => {
  const start = performance.now();
  try {
    module ??= fetch(new URL("factory.wasm", import.meta.url)).then(async (response) => {
      if (!response.ok) throw new Error(`Module: HTTP ${response.status}`);
      return instantiate(await response.arrayBuffer());
    });
    const api = await module;
    if (kind === "inspect") {
      self.postMessage({ id, identity, ordinal, detail: inspectInventory(api, mutant, ordinal) });
      return;
    }
    const result = runFactory(api, act, (progress) => {
      self.postMessage(progress.kind === "inventory" ? { id, identity, progress } : { id, identity, candidate: progress });
    }, options);
    self.postMessage({ id, identity, result, elapsed: performance.now() - start });
  } catch (error) {
    self.postMessage({ id, identity, ordinal, error: error.message });
  }
};
