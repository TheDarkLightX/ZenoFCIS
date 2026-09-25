// The page: six examples behind a row of tabs, one shown at a time in the
// same place, so choosing one never scrolls the page. An example's panel is
// built, and its module fetched, the first time it is chosen. The example
// shown when the page loads also runs its README's demonstration at once,
// so the first view shows the judge at work.

import { mount } from "./panel.js";

const tabs = [...document.querySelectorAll('[role="tab"]')];
const panels = new Map();

function sectionOf(tab) {
  return document.getElementById(tab.getAttribute("aria-controls"));
}

async function open(section) {
  const name = section.dataset.template;
  if (panels.has(name)) return panels.get(name);
  const container = section.querySelector(".demo");
  container.replaceChildren(document.createTextNode("Loading the example."));
  const panel = (async () => {
    try {
      const { template } = await import(new URL(`./templates/${name}.js`, import.meta.url));
      return await mount(container, template);
    } catch (error) {
      container.replaceChildren(document.createTextNode(
        `The example could not be loaded (${error.message}). Build the site with python3 site/build.py and serve site/public over HTTP.`,
      ));
      return null;
    }
  })();
  panels.set(name, panel);
  return panel;
}

async function openOnLoad(section) {
  const panel = await open(section);
  if (panel !== null) await panel.runDemonstration({ instant: true, onLoad: true });
}

function show(tab) {
  for (const other of tabs) {
    const selected = other === tab;
    other.setAttribute("aria-selected", String(selected));
    other.tabIndex = selected ? 0 : -1;
    sectionOf(other).hidden = !selected;
  }
  open(sectionOf(tab));
}

// Arrow keys move the focus along the tabs; Enter or Space, as on any
// button, chooses the focused one.
const STEPS = { ArrowRight: 1, ArrowDown: 1, ArrowLeft: -1, ArrowUp: -1 };
for (const [index, tab] of tabs.entries()) {
  tab.addEventListener("click", () => show(tab));
  tab.addEventListener("keydown", (event) => {
    let target;
    if (event.key in STEPS) target = tabs[(index + STEPS[event.key] + tabs.length) % tabs.length];
    else if (event.key === "Home") target = tabs[0];
    else if (event.key === "End") target = tabs[tabs.length - 1];
    else return;
    event.preventDefault();
    target.focus();
  });
}

const shown = tabs.find((tab) => !sectionOf(tab).hidden);
if (shown) openOnLoad(sectionOf(shown));
