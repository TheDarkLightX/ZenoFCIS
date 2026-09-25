// Capture only after the demonstration finishes. A virtual-time budget can
// expire while asynchronous Wasm compilation is still running on a CI host.
// Prints one JSON line: the document's HTML, and the URL of every request
// the page made, as the DevTools pipe reported them.
import { spawn } from "node:child_process";
import { setTimeout as delay } from "node:timers/promises";

const [chrome, profile, url, limit = "120000"] = process.argv.slice(2);
const timeout = Number(limit);
if (!chrome || !profile || !url || !Number.isSafeInteger(timeout) || timeout < 1 || timeout > 300000) {
  throw new Error("usage: browser.mjs CHROME PROFILE URL [TIMEOUT_MS, 1..300000]");
}
const deadline = Date.now() + timeout;
const args = ["--headless=new", "--disable-gpu", "--no-first-run", "--no-default-browser-check",
  "--disable-extensions", `--user-data-dir=${profile}`, "--remote-debugging-pipe"];
if (process.getuid?.() === 0) args.push("--no-sandbox");
// Chrome reads DevTools messages on fd 3 and writes them on fd 4. This opens
// no listening port and touches only the caller's temporary browser profile.
const browser = spawn(chrome, args, { stdio: ["ignore", "ignore", "pipe", "pipe", "pipe"] });
const pending = new Map();
const loaded = new Set();
const requests = [];
let attached = null;
let sequence = 0;
let stopped = null;
let stderr = "";
let buffered = "";
browser.stderr.setEncoding("utf8");
browser.stderr.on("data", (chunk) => { stderr = (stderr + chunk).slice(-2000); });

function stop(error) {
  stopped ??= error;
  for (const request of pending.values()) request.reject(stopped);
  pending.clear();
}
browser.on("error", stop);
const closed = new Promise((resolve) => browser.on("close", (code, signal) => {
  stop(new Error(`Chrome closed (${code ?? signal}): ${stderr}`));
  resolve();
}));
for (const pipe of [browser.stdio[3], browser.stdio[4]]) pipe.on("error", stop);
browser.stdio[4].setEncoding("utf8");
browser.stdio[4].on("data", (chunk) => {
  buffered += chunk;
  let boundary;
  while ((boundary = buffered.indexOf("\0")) !== -1) {
    const raw = buffered.slice(0, boundary);
    buffered = buffered.slice(boundary + 1);
    try {
      const message = JSON.parse(raw);
      if (message.method === "Page.lifecycleEvent" && message.params.name === "load") {
        loaded.add(`${message.sessionId}:${message.params.loaderId}`);
      }
      if (message.method === "Network.requestWillBeSent" && message.sessionId === attached) {
        requests.push(message.params.request.url);
      }
      const request = pending.get(message.id);
      if (request) {
        pending.delete(message.id);
        if (message.error) request.reject(new Error(JSON.stringify(message.error)));
        else request.resolve(message.result);
      }
    } catch (error) { stop(error); }
  }
});

function command(method, params = {}, sessionId, milliseconds = Math.max(1, deadline - Date.now())) {
  if (stopped) return Promise.reject(stopped);
  const id = ++sequence;
  return new Promise((resolve, reject) => {
    const timer = setTimeout(() => {
      pending.delete(id);
      reject(new Error(`timed out waiting for ${method} at ${url}`));
    }, milliseconds);
    pending.set(id, {
      resolve: (value) => { clearTimeout(timer); resolve(value); },
      reject: (error) => { clearTimeout(timer); reject(error); },
    });
    browser.stdio[3].write(JSON.stringify({ id, method, params, sessionId }) + "\0");
  });
}

function checkDeadline(last) {
  if (stopped) throw stopped;
  if (Date.now() >= deadline) throw new Error(`page not ready after ${timeout} ms at ${url}: ${last}`);
}

try {
  const { targetId } = await command("Target.createTarget", { url: "about:blank" });
  const { sessionId } = await command("Target.attachToTarget", { targetId, flatten: true });
  attached = sessionId;
  await command("Page.enable", {}, sessionId);
  await command("Page.setLifecycleEventsEnabled", { enabled: true }, sessionId);
  await command("Network.enable", {}, sessionId);
  const navigation = await command("Page.navigate", { url }, sessionId);
  if (navigation.errorText) throw new Error(`navigation failed: ${navigation.errorText}`);
  while (!loaded.has(`${sessionId}:${navigation.loaderId}`)) {
    checkDeadline("document load pending");
    await delay(50);
  }
  let last = "demonstration pending";
  for (;;) {
    checkDeadline(last);
    const result = await command("Runtime.evaluate", { returnByValue: true, expression: `(() => {
      const output = document.getElementById("results");
      const label = (output ?? document.querySelector(".status"))?.textContent?.trim() ?? "";
      const ready = output ? label !== "" && label !== "pending"
        : label.includes("decided in this browser when the page loaded");
      return { ready, label: label.slice(0, 300), html: ready ? document.documentElement.outerHTML : null };
    })()` }, sessionId);
    if (result.exceptionDetails) throw new Error(JSON.stringify(result.exceptionDetails));
    const state = result.result.value;
    if (state.ready) {
      process.stdout.write(JSON.stringify({ html: state.html, requests }) + "\n");
      break;
    }
    last = state.label || last;
    await delay(100);
  }
} catch (error) {
  console.error(`browser check: ${error.message}`);
  process.exitCode = 1;
} finally {
  await command("Browser.close", {}, undefined, 2000).catch(() => {});
  const terminate = setTimeout(() => browser.kill("SIGKILL"), 2000);
  await closed;
  clearTimeout(terminate);
}
