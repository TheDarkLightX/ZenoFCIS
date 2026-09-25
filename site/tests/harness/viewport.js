// The deploy check's viewport harness. It loads the served page in a frame
// of the same origin, sized by `?width=` and `?height=` (1280 by 800 unless
// given), waits until the page's example says its demonstration was decided
// on load, and prints the frame's scroll position, its size, the document's
// size, the title heading's rectangle, and any element that reaches past
// the viewport's right edge, for site/tests/deploy_check.py to read from
// the dumped DOM. A page that scrolled itself while deciding shows a scroll
// position above zero and a title outside the viewport; a page too wide for
// a phone shows a document wider than the viewport.

const output = document.getElementById("results");
const frame = document.getElementById("page");
const LOAD_LABEL = "decided in this browser when the page loaded";
const POLL_MS = 100;
const POLL_LIMIT = 1000;
const SETTLE_MS = 500;

const delay = (milliseconds) => new Promise((resolve) => setTimeout(resolve, milliseconds));

function print(value) {
  output.textContent = JSON.stringify(value);
}

try {
  const parameters = new URLSearchParams(location.search);
  frame.width = String(Number(parameters.get("width") ?? 1280));
  frame.height = String(Number(parameters.get("height") ?? 800));
  frame.src = new URL(parameters.get("page") ?? "/", location.href);
  let status = null;
  for (let attempt = 0; attempt < POLL_LIMIT && status === null; attempt += 1) {
    await delay(POLL_MS);
    const found = frame.contentDocument?.querySelector(".status");
    if (found && found.textContent.includes(LOAD_LABEL)) status = found.textContent;
  }
  // A scroll the page starts while deciding settles before it is measured.
  await delay(SETTLE_MS);
  const inner = frame.contentWindow;
  const root = inner.document.documentElement;
  const title = inner.document.querySelector("h1");
  const rectangle = title ? title.getBoundingClientRect() : null;
  const wide = [...inner.document.querySelectorAll("body *")]
    .map((element) => [element, element.getBoundingClientRect()])
    .filter(([, box]) => box.width > 0 && box.right > root.clientWidth + 1)
    .slice(0, 8)
    .map(([element]) => `${element.tagName.toLowerCase()}${element.className ? `.${String(element.className).split(" ")[0]}` : ""}`);
  print({
    status,
    scrollX: inner.scrollX,
    scrollY: inner.scrollY,
    viewportWidth: inner.innerWidth,
    viewportHeight: inner.innerHeight,
    clientWidth: root.clientWidth,
    documentWidth: root.scrollWidth,
    documentHeight: root.scrollHeight,
    title: rectangle ? { top: rectangle.top, bottom: rectangle.bottom, text: title.textContent } : null,
    wide,
  });
} catch (error) {
  print({ error: String(error) });
}
