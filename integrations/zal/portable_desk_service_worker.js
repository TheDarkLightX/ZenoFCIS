/* Keeps the portable desk's own files on the device, so that it starts with no connection. It answers GET
   requests for the files listed here from its own copy and reaches the network only to fetch those files.
   A newer build waits until every open desk has closed, so one page never mixes the files of two builds. */
const STAMP = "__ZAL_PORTABLE_DESK_STAMP__";
const FILES = /*__ZAL_PORTABLE_DESK_FILES__*/[];
const KEPT = "zal-portable-desk-" + STAMP;

self.addEventListener("install", (event) => event.waitUntil(caches.open(KEPT).then((kept) => kept.addAll(FILES))));
self.addEventListener("activate", (event) => event.waitUntil(caches.keys().then((names) =>
  Promise.all(names.filter((name) => name.startsWith("zal-portable-desk-") && name !== KEPT).map((name) => caches.delete(name))))));
self.addEventListener("fetch", (event) => {
  if (event.request.method !== "GET") return;
  event.respondWith(caches.open(KEPT).then((kept) => kept.match(event.request, { ignoreSearch: true })).then((own) => own || fetch(event.request)));
});
