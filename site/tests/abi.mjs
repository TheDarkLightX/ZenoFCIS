#!/usr/bin/env node
// The built modules must enforce the bounded protocol, including when the
// page's loader is bypassed. These tests use only version 2 scalar calls.
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { instantiate } from "../public/demo-module.js";

const expected = ["demo_abi_version", "demo_begin", "demo_write", "demo_step",
  "demo_read", "demo_state", "demo_reset"].sort();
const encoder = new TextEncoder();
const decoder = new TextDecoder("utf-8", { fatal: true });

for (const name of process.argv.slice(2)) {
  const bytes = readFileSync(new URL(`../public/${name}.wasm`, import.meta.url));
  const module = await WebAssembly.compile(bytes);
  assert.deepEqual(WebAssembly.Module.imports(module), [], `${name}: no imported memory or functions`);
  assert.deepEqual(WebAssembly.Module.exports(module).map(({ name, kind }) => [name, kind]).sort(),
    expected.map((name) => [name, "function"]), `${name}: only the safe scalar interface is exported`);
  const { exports: api } = await WebAssembly.instantiate(module, {});
  assert.equal(api.demo_abi_version(), 2);

  function take(length) {
    length >>>= 0;
    assert.ok(length > 0 && length <= 131072, `${name}: bounded reply`);
    const bytes = new Uint8Array(length);
    for (let offset = 0; offset < length; offset += 4) {
      const word = api.demo_read(offset) >>> 0;
      for (let i = 0; i < 4 && offset + i < length; i++) bytes[offset + i] = (word >>> (8 * i)) & 255;
    }
    assert.equal(api.demo_read(length), 0);
    assert.equal(api.demo_read(0xffffffff), 0);
    return JSON.parse(decoder.decode(bytes));
  }

  function put(input) {
    assert.equal(api.demo_begin(input.length), 1);
    for (let offset = 0; offset < input.length; offset += 4) {
      let word = 0;
      for (let i = 0; i < 4 && offset + i < input.length; i++) word |= input[offset + i] << (8 * i);
      assert.equal(api.demo_write(word >>> 0), 1);
    }
  }

  const genesis = take(api.demo_reset());
  assert.equal(genesis.steps, 0);
  for (const length of [0, 4097, 0xffffffff]) {
    assert.equal(api.demo_begin(length), 0);
    assert.equal(api.demo_write(0), 0);
    assert.equal(take(api.demo_step()).stage, "input");
  }
  assert.equal(api.demo_begin(5), 1);
  assert.equal(api.demo_write(0), 1);
  assert.equal(take(api.demo_step()).stage, "input");
  put(new Uint8Array([255]));
  assert.equal(take(api.demo_step()).error, "input is not UTF-8");
  put(encoder.encode("{}"));
  assert.equal(api.demo_write(0), 0); // An extra word invalidates the request.
  assert.equal(take(api.demo_step()).stage, "input");
  put(encoder.encode("{}"));
  assert.deepEqual(take(api.demo_state()), genesis); // Reading state discards pending input.
  assert.equal(take(api.demo_step()).stage, "input");
  assert.deepEqual(take(api.demo_state()), genesis);
  put(encoder.encode(" ".repeat(4094) + "{}"));
  assert.equal(take(api.demo_step()).stage, "input"); // Full-capacity input reaches the parser.
  assert.equal(take(api.demo_step()).stage, "input"); // It cannot be consumed twice.
  assert.deepEqual(take(api.demo_state()), genesis);

  const { template } = await import(`../public/templates/${name}.js`);
  const demo = await instantiate(bytes);
  assert.deepEqual(demo.reset(), genesis);
  assert.equal(demo.stepText("a".repeat(4097)).stage, "input");
  assert.equal(demo.stepText("é".repeat(2049)).stage, "input");
  assert.deepEqual(demo.state(), genesis);
  // Valid schema-admitted requests also exercise retained history at the cap.
  const request = template.demonstration[0].request;
  for (let index = 0; index < 64; index++) {
    const report = demo.step(request);
    assert.equal(report.error, undefined, `${name}: request ${index + 1}: ${report.error}`);
  }
  const full = demo.state();
  assert.equal(full.steps, 64);
  assert.match(demo.step(request).error, /session limit/);
  assert.deepEqual(demo.state(), full);
  assert.deepEqual(demo.reset(), genesis);
  assert.equal(demo.step(request).decision, template.demonstration[0].expect);

  // Parser refusals consume capacity too, so they cannot grow error history.
  demo.reset();
  for (let index = 0; index < 64; index++) assert.equal(demo.stepText("{}").stage, "input");
  assert.match(demo.step(request).error, /session limit/);
  assert.deepEqual(demo.state(), genesis);
  console.log(`${name}: private memory, bounded input/output, request lifecycle, session cap and reset: passed`);
}
