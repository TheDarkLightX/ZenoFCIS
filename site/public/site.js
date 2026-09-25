// The page: one panel per example, built when its section is opened. Each
// example's description and module are loaded only then. The section that
// is open when the page loads also runs its README's demonstration at once,
// so the first view shows the judge at work.

import { mount } from "./panel.js";

const panels = new Map();

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

for (const section of document.querySelectorAll("details[data-template]")) {
  section.addEventListener("toggle", () => {
    if (section.open) open(section);
  });
  if (section.open) openOnLoad(section);
}
