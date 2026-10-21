// Execute the published worker in a real Node worker thread, with local bytes
// supplied instead of HTTP. Browser transport and layout have separate gates.
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { Worker } from "node:worker_threads";
import { test } from "node:test";
const expected = JSON.parse(readFileSync(new URL("harness/factory-native.json", import.meta.url)));
const publicRoot = new URL("../public/", import.meta.url);
const source = readFileSync(new URL("factory-worker.js",publicRoot),"utf8")
  .replaceAll('"./demo-module.js"',JSON.stringify(new URL("demo-module.js",publicRoot).href))
  .replaceAll('"./factory-runner.js"',JSON.stringify(new URL("factory-runner.js",publicRoot).href))
  .replace('new URL("factory.wasm", import.meta.url)',`new URL(${JSON.stringify(new URL("factory.wasm",publicRoot).href)})`);
const bootstrap = `
const {parentPort,workerData} = require("node:worker_threads");
const {readFile} = require("node:fs/promises");
globalThis.fetch = async url => ({ok:true,arrayBuffer:async()=>{const b=await readFile(url);return b.buffer.slice(b.byteOffset,b.byteOffset+b.byteLength)}});
globalThis.self = {postMessage(message) {
 if (message.result) {
  const r=message.result;
  if(workerData.control==="all-pass" && r.act==="inventory") {r.passed=r.domain;r.mutant_passed=r.domain;r.differences=0;r.first_difference=null;r.status="equivalent over the full domain";}
  if(workerData.control==="ignore-difference") r.first_difference=null;
  if(workerData.control==="unchecked-counterexample") r.first_counterexample.input[0]++;
 }
 parentPort.postMessage(message);
}};
import("data:text/javascript;base64,"+Buffer.from(workerData.source).toString("base64")).then(()=>{
 parentPort.on("message", data=>self.onmessage({data}));parentPort.postMessage({ready:true});
});`;
async function run(act, control) {
  const worker = new Worker(bootstrap, { eval:true, workerData:{source,control} });
  try {
    return await new Promise((resolve,reject) => {
      const timer=setTimeout(()=>reject(new Error("worker deadline")),10000);
      worker.on("error",reject);
      worker.on("message", message => {
        if(message.ready) worker.postMessage({id:1,act});
        if(message.error) {clearTimeout(timer);reject(new Error(message.error));}
        if(message.result) {clearTimeout(timer);resolve(message.result);}
      });
    });
  } finally { await worker.terminate(); }
}
test("published_worker_matches_native_results", async()=> {
  for(const [act,oracle] of Object.entries(expected)) assert.deepEqual(await run(act),oracle,act);
});
test("native_domain_results_kill_all_passing_worker",async()=> {
  const key="inventory/m0018";
  const awaitResult=await run(key,"all-pass");
  assert.throws(()=>assert.deepEqual(awaitResult,expected[key]),/AssertionError/);
});
test("native_mutant_first_difference_kills_ignored_witness",async()=> {
  const key="inventory/m0002";
  const awaitResult=await run(key,"ignore-difference");
  assert.throws(()=>assert.deepEqual(awaitResult,expected[key]),/AssertionError/);
});
test("native_synthesis_counterexample_kills_unchecked_witness",async()=> {
  const actual=await run("synthesis","unchecked-counterexample");
  assert.throws(()=>assert.deepEqual(actual,expected.synthesis),/AssertionError/);
});
