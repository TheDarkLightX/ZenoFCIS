"""Build the portable desk: the review desk with ZAL running in the page, for phones, tablets and browsers.

    python3 integrations/zal/portable_desk_build.py --runtime RUNTIME --out OUT [--version 0.1.0]

RUNTIME is the unpacked WebAssembly Python runtime that portable_desk_runtime.json pins. This tool downloads
nothing. It compares every runtime file with the pinned hash and refuses a runtime that differs.

OUT receives:

    web/                          static files; served over HTTPS they install to a home screen and start offline
    extension/                    the same page as a Manifest V3 extension for Chrome, Brave and Firefox
    portable-desk-extension.zip   the extension folder, packed
    portable-desk-build.json      what was built: the checker identity, the runtime and a hash of every file

The page is review_desk.html with portable_desk.js placed before its script. ZAL's sources travel unmodified
in zal_sources.zip. Which sources travel is found by watching ZAL answer the desk in a fresh interpreter, not
by a list kept here. The packed sources are then run alone, away from this repository, and must give the
checker identity and the result of every carried example that the repository gives.

The build grants no authority and records no agreement. Publishing its output is the owner's decision.
"""
from __future__ import annotations

import argparse
import hashlib
import io
import json
import math
from pathlib import Path
import re
import shutil
import struct
import subprocess
import sys
import tempfile
import zipfile
import zlib

import json_numbers
import review_desk
from behavior import Refusal

HERE = Path(__file__).resolve().parent
PIN = HERE / "portable_desk_runtime.json"
RUNTIME_FOLDER, SOURCES_NAME = "pyodide", "zal_sources.zip"
NAME, SHORT_NAME = "ZAL portable desk", "ZAL"
SAYS = ("Write, check and read ZAL statements on this device. ZAL's own checker runs inside the page. "
        "Nothing is sent anywhere, and the desk records no agreement.")
INK, PAPER = "#131a24", "#eceff3"
VERSION = re.compile(r"(?:0|[1-9]\d{0,4})(?:\.(?:0|[1-9]\d{0,4})){0,3}\Z")       # what an extension manifest accepts
WEB_POLICY = ("default-src 'none'; script-src 'self' 'wasm-unsafe-eval'; style-src 'self' 'unsafe-inline'; "
              "img-src 'self' data:; connect-src 'self'; worker-src 'self'; manifest-src 'self'; "
              "base-uri 'none'; form-action 'none'")
EXTENSION_POLICY = "script-src 'self' 'wasm-unsafe-eval'; object-src 'self'"
ADD_ON_ID = "portable-desk@zal.invalid"     # a placeholder: the owner chooses the identity before any store sees it
FIXED_TIME = (1980, 1, 1, 0, 0, 0)          # so the same input packs to the same bytes

# Run in a fresh interpreter: watch which of ZAL's files are opened while the desk answers, and what it answers.
PROBE = r'''
import hashlib, json, os, sys
here, opened = os.path.realpath(sys.argv[1]), set()
def watch(event, args):
    if event == "open" and args and isinstance(args[0], (str, bytes)):
        path = os.path.realpath(os.fsdecode(args[0]))
        if os.path.dirname(path) == here:
            opened.add(os.path.basename(path))
sys.addaudithook(watch)
sys.dont_write_bytecode = True
sys.path.insert(0, here)
import portable_desk
desk = portable_desk.Desk()
ask = lambda **request: json.loads(desk.exchange(json.dumps(request)))
about, told = ask(route="about")["reply"], []
for example in json.load(sys.stdin):
    reply = ask(route="open", name=example["file"], source=example["source"])["reply"]
    if "bundle" not in reply:
        told.append({"file": example["file"], "refused": reply["error"]["reason"]})
        continue
    bundle = reply["bundle"]
    edits = ask(route="edits", revision=bundle["revision"])
    rule = next((entry["id"] for entry in bundle["declared"] if entry["concept"] == "rule"), None)
    step = ask(route="fires", revision=bundle["revision"], rule=rule)["reply"] if rule else None
    explained = ask(route="explain", revision=bundle["revision"], input=step) if step else None
    compared = ask(route="candidate", revision=bundle["revision"], source=example["source"])
    rest = json.dumps([edits, step, explained, compared], sort_keys=True, separators=(",", ":"))
    told.append({"file": example["file"], "name": bundle["name"], "profile": bundle["profile"], "status": bundle["check"]["status"],
                 "digest": reply["digest"], "rest": hashlib.sha256(rest.encode()).hexdigest()})
modules = {os.path.basename(module.__file__) for module in list(sys.modules.values())
           if getattr(module, "__file__", None) and os.path.dirname(os.path.realpath(module.__file__)) == here}
print(json.dumps({"checker": about["checker"], "files": sorted(opened | modules), "examples": told}))
'''


def sha256(data):
    return hashlib.sha256(data).hexdigest()


def read_pin(path=PIN):
    pin = json.loads(path.read_text(encoding="utf-8"), **json_numbers.hooks(json_numbers.raising(Refusal, "runtime pin: ")))
    if not (isinstance(pin, dict) and isinstance(pin.get("files"), dict) and pin["files"] and
            all(re.fullmatch(r"[A-Za-z0-9._-]+", name) and re.fullmatch(r"[0-9a-f]{64}", str(digest)) for name, digest in pin["files"].items())):
        raise Refusal("The runtime pin needs a files table of plain names and SHA-256 hashes")
    return pin


def verified_runtime(folder, pin):
    """The pinned runtime files as bytes. A missing file or one that differs from its pinned hash is refused."""
    files = {}
    for name, expected in sorted(pin["files"].items()):
        path = Path(folder) / name
        if not path.is_file():
            raise Refusal(f"The runtime folder has no {name}; unpack {pin.get('package')} {pin.get('version')} there")
        data = path.read_bytes()
        if sha256(data) != expected:
            raise Refusal(f"Runtime file {name} is not the pinned one (SHA-256 {sha256(data)[:16]}..., pinned {expected[:16]}...)")
        files[name] = data
    return files


def probe(folder, examples):
    """What a fresh interpreter says when only `folder` holds ZAL: the checker, the files it opened and each example's result."""
    done = subprocess.run([sys.executable, "-I", "-B", "-c", PROBE, str(folder)], input=json.dumps(examples), text=True,
                          stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=600)
    if done.returncode != 0:
        raise Refusal("ZAL did not answer the desk from " + str(folder) + ": " + done.stderr.strip()[-600:])
    return json.loads(done.stdout, **json_numbers.hooks(json_numbers.raising(Refusal, "probe: ")))


def carried_examples(folder=HERE / "examples"):
    """The example statements the desk reads, as the page will carry them."""
    out = []
    for path in sorted(folder.glob("*.zal")):
        text = review_desk.source_text(review_desk.read_bounded(path))
        try:
            review_desk.read_statement(text)
        except Refusal:                                                # a typed or effects profile, which the desk does not read
            continue
        out.append({"file": path.name, "source": text})
    return out


def packed(files):
    """A zip of the given {name: bytes}: names sorted and times fixed, so the same files pack to the same bytes."""
    buffer = io.BytesIO()
    with zipfile.ZipFile(buffer, "w", zipfile.ZIP_DEFLATED, compresslevel=9) as archive:
        for name in sorted(files):
            entry = zipfile.ZipInfo(name, FIXED_TIME)
            entry.compress_type, entry.external_attr = zipfile.ZIP_DEFLATED, 0o644 << 16
            archive.writestr(entry, files[name])
    return buffer.getvalue()


def zal_sources(examples):
    """ZAL's sources as they travel, and proof that they are enough: (zip bytes, {name: hash}, checker, example results)."""
    seen = probe(HERE, examples)
    files = {name: (HERE / name).read_bytes() for name in seen["files"]}
    archive = packed(files)
    with tempfile.TemporaryDirectory(prefix="zal-portable-desk-") as folder:
        with zipfile.ZipFile(io.BytesIO(archive)) as unpacked:
            unpacked.extractall(folder)
        alone = probe(folder, examples)
    if (alone["checker"], alone["examples"]) != (seen["checker"], seen["examples"]) or not set(alone["files"]) <= set(files):
        raise Refusal("The packed sources do not answer as the repository does; nothing was built")
    refused = [told["file"] for told in seen["examples"] if "refused" in told]
    if refused:
        raise Refusal("ZAL refuses a carried example: " + ", ".join(refused))
    return archive, {name: sha256(data) for name, data in sorted(files.items())}, seen["checker"], seen["examples"]


# ---- the mark: drawn here, so the repository keeps no binary file -----------------------------------------------------

def mark(size, bleed=False):
    """A white Z on the desk's ink as PNG bytes. With bleed the ground fills the square, for a launcher that masks it."""
    over, ground, paper = 4, tuple(int(INK[at:at + 2], 16) for at in (1, 3, 5)), (255, 255, 255)
    edge = size * over
    radius, inset, bar = (0 if bleed else 0.22 * edge), (0.31 if bleed else 0.25) * edge, 0.125 * edge
    left, right, top, bottom, slant = inset, edge - inset, inset, edge - inset, 0.19 * edge
    covered, lettered = [[0.0] * size for _ in range(size)], [[0.0] * size for _ in range(size)]

    def add(row, start, stop):
        start, stop = max(0.0, start), min(float(edge), stop)
        for x in range(int(start // over), min(size, int(-(-stop // over)))):
            row[x] += max(0.0, min(stop, (x + 1) * over) - max(start, x * over))

    for sub in range(edge):
        y, at = sub + 0.5, sub // over
        gap = 0.0
        if y < radius or y > edge - radius:
            away = radius - y if y < radius else y - (edge - radius)
            gap = radius - math.sqrt(max(0.0, radius * radius - away * away))
        add(covered[at], gap, edge - gap)
        if top <= y < top + bar or bottom - bar <= y < bottom:
            add(lettered[at], left, right)
        elif top + bar <= y < bottom - bar:
            along = (y - top - bar) / (bottom - top - 2 * bar)
            stop = right - along * (right - left - slant)
            add(lettered[at], stop - slant, stop)
    rows = bytearray()
    for y in range(size):
        rows.append(0)
        for x in range(size):
            there, letter = covered[y][x] / (over * over), min(1.0, lettered[y][x] / (over * over))
            rows += bytes(round(ground[c] + (paper[c] - ground[c]) * letter) for c in range(3)) + bytes([round(255 * min(1.0, there))])
    chunk = lambda kind, data: struct.pack(">I", len(data)) + kind + data + struct.pack(">I", zlib.crc32(kind + data))
    return (b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", size, size, 8, 6, 0, 0, 0)) +
            chunk(b"IDAT", zlib.compress(bytes(rows), 9)) + chunk(b"IEND", b""))


# ---- the page ---------------------------------------------------------------------------------------------------------

def page_files(payload, policy):
    """review_desk.html as the portable page: (index.html, desk.js). No script stays inline, so a strict policy can hold."""
    html = review_desk.page(payload)
    opened, closed = html.find("<script>"), html.rfind("</script>")
    if html.count("<script") != 1 or opened < 0 or closed < opened or "</head>" not in html or "<title>ZAL review desk</title>" not in html:
        raise Refusal("review_desk.html no longer has one inline script and the head this build rewrites")
    script = (HERE / "portable_desk.js").read_text(encoding="utf-8") + "\n" + html[opened + len("<script>"):closed]
    head = ((f'<meta http-equiv="Content-Security-Policy" content="{policy}">\n' if policy else "") +
            f'<meta name="theme-color" content="{INK}">\n<meta name="description" content="{SAYS}">\n'
            '<link rel="icon" href="icons/icon-48.png">\n<link rel="apple-touch-icon" href="icons/icon-192.png">\n'
            + ('<link rel="manifest" href="manifest.webmanifest">\n' if payload["portable"]["kind"] == "web" else "") +
            '<link rel="stylesheet" href="portable_desk.css">\n')
    html = html[:opened] + '<script src="desk.js"></script>' + html[closed + len("</script>"):]
    html = html.replace("<title>ZAL review desk</title>", f"<title>{NAME}</title>", 1).replace("</head>", head + "</head>", 1)
    return html.encode("utf-8"), script.encode("utf-8")


def filled(name, slots):
    """One of this folder's templates with each of its slots, which must occur exactly once, filled in."""
    text = (HERE / name).read_text(encoding="utf-8")
    for slot, value in slots.items():
        if text.count(slot) != 1:
            raise Refusal(f"{name} does not have exactly one {slot}")
        text = text.replace(slot, value)
    return text.encode("utf-8")


def notices(pin):
    return (f"{NAME}\n\nZAL and this desk are part of ZenoFCIS and carry its license (MIT OR Apache-2.0).\n\n"
            f"The folder {RUNTIME_FOLDER}/ holds the unmodified files of {pin.get('package')} {pin.get('version')}, "
            f"a build of CPython for WebAssembly, published under {pin.get('license')}.\nIts source: {pin.get('source')}\n"
            "It contains CPython and other components under their own licenses; the project's repository lists them.\n"
            "portable-desk-build.json names every file of this build with its SHA-256.\n").encode("utf-8")


def build(runtime, out, version="0.1.0", pin=None, examples=None):
    """Write the web folder, the extension folder and its zip under `out`; returns the build record."""
    if not VERSION.match(version):
        raise Refusal("The version needs one to four numbers separated by dots, as an extension manifest requires")
    pin = read_pin() if pin is None else pin
    engine = verified_runtime(runtime, pin)
    examples = carried_examples() if examples is None else examples
    archive, sources, checker, results = zal_sources(examples)
    told = {result["file"]: result for result in results}
    shown = [{**example, "name": told[example["file"]]["name"], "profile": told[example["file"]]["profile"],
              "status": told[example["file"]]["status"], "digest": told[example["file"]]["digest"]} for example in examples]
    shared = {"portable_desk_engine.js": (HERE / "portable_desk_engine.js").read_bytes(), "portable_desk.css": (HERE / "portable_desk.css").read_bytes(),
              SOURCES_NAME: archive, "NOTICES.txt": notices(pin),
              **{f"{RUNTIME_FOLDER}/{name}": data for name, data in engine.items()},
              **{f"icons/icon-{size}.png": mark(size) for size in (16, 32, 48, 128, 192, 512)}, "icons/maskable-512.png": mark(512, bleed=True)}
    about = {"version": version, "checker": checker, "runtime": {key: pin.get(key) for key in ("package", "version", "license")}, "examples": shown}

    def flavour(kind, policy):
        index, script = page_files({"mode": "device", "bundles": [], "portable": {**about, "kind": kind}}, policy)
        return {**shared, "index.html": index, "desk.js": script}

    web = flavour("web", WEB_POLICY)
    web["manifest.webmanifest"] = json.dumps({
        "name": NAME, "short_name": SHORT_NAME, "description": SAYS, "start_url": "./index.html", "scope": "./", "display": "standalone",
        "background_color": PAPER, "theme_color": INK,
        "icons": [{"src": "icons/icon-192.png", "sizes": "192x192", "type": "image/png"}, {"src": "icons/icon-512.png", "sizes": "512x512", "type": "image/png"},
                  {"src": "icons/maskable-512.png", "sizes": "512x512", "type": "image/png", "purpose": "maskable"}]}, indent=1).encode()
    kept = sorted(web)                                                 # every file of the web folder is kept for offline use
    stamp = sha256(json.dumps({name: sha256(data) for name, data in sorted(web.items())}, sort_keys=True).encode())[:16]
    web["service_worker.js"] = filled("portable_desk_service_worker.js", {
        "__ZAL_PORTABLE_DESK_STAMP__": stamp, "/*__ZAL_PORTABLE_DESK_FILES__*/[]": json.dumps(["./", *kept])})

    extension = flavour("extension", None)                             # an extension's policy is in its manifest
    extension["extension_background.js"] = (HERE / "portable_desk_extension.js").read_bytes()
    extension["manifest.json"] = json.dumps({
        "manifest_version": 3, "name": NAME, "short_name": SHORT_NAME, "version": version, "description": SAYS[:132],
        "icons": {str(size): f"icons/icon-{size}.png" for size in (16, 32, 48, 128)},
        "action": {"default_title": "Open the ZAL desk", "default_icon": {str(size): f"icons/icon-{size}.png" for size in (16, 32)}},
        "background": {"service_worker": "extension_background.js", "scripts": ["extension_background.js"]},
        "content_security_policy": {"extension_pages": EXTENSION_POLICY},
        "browser_specific_settings": {"gecko": {"id": ADD_ON_ID, "strict_min_version": "121.0", "data_collection_permissions": {"required": ["none"]}},
                                      "gecko_android": {"strict_min_version": "121.0"}}}, indent=1).encode()

    out = Path(out)
    for folder, files in (("web", web), ("extension", extension)):
        if (out / folder).exists():
            shutil.rmtree(out / folder)
        for name, data in files.items():
            (out / folder / name).parent.mkdir(parents=True, exist_ok=True)
            (out / folder / name).write_bytes(data)
    (out / "portable-desk-extension.zip").write_bytes(packed(extension))
    record = {"format": "zal/portable-desk-build/1", "version": version, "checker": checker, "authority": "none",
              "runtime": {**{key: pin.get(key) for key in ("package", "version", "license", "source")}, "files": pin["files"]},
              "sources": sources, "examples": [{key: example[key] for key in ("file", "status", "digest")} for example in shown],
              "web": {name: sha256(data) for name, data in sorted(web.items())},
              "extension": {name: sha256(data) for name, data in sorted(extension.items())}}
    (out / "portable-desk-build.json").write_text(json.dumps(record, indent=1) + "\n", encoding="utf-8")
    return record


def main(argv=None):
    parser = argparse.ArgumentParser(description="Build the portable ZAL desk as a web app and a browser extension")
    parser.add_argument("--runtime", type=Path, required=True, help="the unpacked runtime that portable_desk_runtime.json pins")
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--version", default="0.1.0")
    options = parser.parse_args(argv)
    try:
        record = build(options.runtime, options.out, options.version)
    except (Refusal, OSError) as error:
        print(f"refused: {error}", file=sys.stderr)
        return 2
    size = lambda folder: sum(path.stat().st_size for path in (options.out / folder).rglob("*") if path.is_file())
    print(f"wrote {options.out}: web {size('web'):,} bytes, extension {size('extension'):,} bytes; "
          f"{len(record['sources'])} ZAL source files, {len(record['examples'])} examples, checker {record['checker'][:12]}; authority: none")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
