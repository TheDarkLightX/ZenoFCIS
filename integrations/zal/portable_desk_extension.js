/* The extension's only background code. Pressing its toolbar button opens the desk in a tab.
   It asks for no permission and requests nothing outside the extension's own files. */
const host = globalThis.browser || globalThis.chrome;
host.action.onClicked.addListener(() => host.tabs.create({ url: host.runtime.getURL("index.html") }));
