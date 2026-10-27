import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { instantiate } from "../public/demo-module.js";
import { runFactory, inspectInventory } from "../public/factory-runner.js";
const expected = JSON.parse(readFileSync(new URL("harness/factory-native.json", import.meta.url)));
const api = await instantiate(readFileSync(new URL("../public/factory.wasm", import.meta.url)));
for (const [act, oracle] of Object.entries(expected)) {
  const start = performance.now();
  const progress=[];
  const actual = runFactory(api, act,(item)=>progress.push(item));
  if(act.startsWith("inventory/")) {
    assert.equal(progress.length,18,"all 18 bounded row chunks are reported");
    assert.deepEqual(progress.flatMap(chunk=>chunk.cells),oracle.grid.cells,"progress uses the current native rows");
  }
  assert.deepEqual(actual, oracle, `${act}: native parity over full results`);
  assert.ok(performance.now() - start < 3000, `${act}: three second budget`);
  console.log(`factory ${act}: native parity passed in ${(performance.now() - start).toFixed(1)} ms`);
}
const first = expected.synthesis.first_counterexample;
assert.deepEqual(first, expected.synthesis.candidates[0].witness, "first candidate witness is replayed");
const corrupted = structuredClone(expected.synthesis);
corrupted.first_counterexample.input[0]++;
assert.throws(() => assert.deepEqual(corrupted, expected.synthesis), "native_synthesis_counterexample kills unchecked witness");

assert.equal(expected.search.before_nodes,16);
assert.equal(expected.search.after_nodes,7);
assert.equal(expected.search.first_difference.ordinal,1);
const genesis=api.reset();
for(let i=0;i<64;i++) assert.equal(api.stepText("{}").stage,"input");
assert.match(api.step({act:"synthesis"}).error,/session limit/);
assert.deepEqual(api.state(),genesis,"factory refusals retain no state");
assert.deepEqual(api.reset(),genesis);
console.log("factory transport: reset, refused-request accounting, private memory and session cap passed");

for(const vector of JSON.parse(readFileSync(new URL("money-native.json",import.meta.url)))) {
  api.reset();
  assert.deepEqual(api.step(vector.request),vector.result,"money: native and independent big integer reference");
}
for(const bad of [{amount:true},{fee_bps:"10001"},{reserve_in:"0"},{amount:"-1"},{amount:"3000000001"},{reserve_out:"1.5"}]) {
  api.reset();
  assert.equal(api.step({act:"money",...bad}).stage,"input","invalid numeric input is refused");
}
console.log("money: pilot graph identity, big integer edge vectors, exact overflow work and malformed inputs passed");

for(const request of [{act:"inventory",mutant:false},{act:"synthesis",extra:true},{act:"candidate",number:8},{act:"candidate",number:true},[],null]) {
  api.reset();
  assert.equal(api.step(request).stage,"input","strict factory request admission");
}

const details=JSON.parse(readFileSync(new URL("harness/inventory-details-native.json",import.meta.url)));
for(const [key,oracle] of Object.entries(details)) {
  const [mutant,ordinal]=key.split("/");
  assert.deepEqual(inspectInventory(api,mutant,Number(ordinal)),oracle,"selected cell is decided by the native authority");
}
for(const request of [{act:"inventory_rows",start:0,count:49},{act:"inventory_rows",start:864,count:1},{act:"inventory_rows",start:0,count:0},{act:"inventory_detail",ordinal:864}]) {
  api.reset();assert.equal(api.step(request).stage,"input","bounded row and cell admission");
}
