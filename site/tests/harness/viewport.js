// The deploy check's viewport harness. It loads the served page in a
// viewport-sized frame of the same origin, waits until the page's open
// section says its demonstration was decided on load, and prints the
// frame's scroll position, its size, the document's height, and the
// banner's rectangle, for site/tests/deploy_check.py to read from the
// dumped DOM. A page that scrolled itself while deciding shows a scroll
// position above zero and a banner outside the viewport.

const output = document.getElementById("results");
const frame = document.getElementById("page");
const LOAD_LABEL = "decided in this browser when the page loaded";
const POLL_MS = 100;
const POLL_LIMIT = 1000;

const delay = (milliseconds) => new Promise((resolve) => setTimeout(resolve, milliseconds));

function print(value) {
  output.textContent = JSON.stringify(value);
}

try {
  frame.src = new URL(new URLSearchParams(location.search).get("page") ?? "/", location.href);
  let status = null;
  for (let attempt = 0; attempt < POLL_LIMIT && status === null; attempt += 1) {
    await delay(POLL_MS);
    const found = frame.contentDocument?.querySelector(".status");
    if (found && found.textContent.includes(LOAD_LABEL)) status = found.textContent;
  }
  const inner = frame.contentWindow;
  const banner = inner.document.querySelector("header h1");
  const rectangle = banner ? banner.getBoundingClientRect() : null;
  print({
    status,
    scrollY: inner.scrollY,
    viewportHeight: inner.innerHeight,
    documentHeight: inner.document.documentElement.scrollHeight,
    banner: rectangle ? { top: rectangle.top, bottom: rectangle.bottom, text: banner.textContent } : null,
  });
} catch (error) {
  print({ error: String(error) });
}
