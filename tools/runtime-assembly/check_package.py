#!/usr/bin/env python3
"""Smoke the copied release executable through its checked launcher over loopback."""
import argparse
import http.client
import json
import os
from pathlib import Path
import secrets
import subprocess
import sys
import tempfile
import time


def require(condition):
    if not condition:
        raise ValueError("PackageAcceptanceFailed")


def call(path, credential, body=None):
    connection = http.client.HTTPConnection("127.0.0.1", 8080, timeout=2)
    try:
        connection.request("POST" if body is not None else "GET", path, body,
                           {"Authorization": "Bearer " + credential})
        reply = connection.getresponse()
        return reply.status, reply.read()
    finally:
        connection.close()


def check(package, scratch):
    assembly = json.loads((package / "assembly.json").read_text())
    if assembly["profile"] == "task-status-sqlite-local-test/1":
        initial, context, denied = ["150", "0"], ["170"], ["160", "0", "1"]
        steps = [("operator", ["160", "0", "1"], ["151", "1"]),
                 ("adapter", ["161", "1", "1"], ["152", "1"]),
                 ("adapter", ["162", "1", "1"], ["153", "1"])]
        destination = "local-test"
    elif assembly["profile"] == "inventory-reservation-sqlite-local-simulation/1":
        initial, context, denied = ["0", "0"], ["1"], ["153", "3"]
        steps = [("operator", ["153", "3"], ["3", "0"]),
                 ("operator", ["150", "3"], ["0", "3"]),
                 ("operator", ["152", "2"], ["0", "1"])]
        destination = "warehouse"
    else:
        raise ValueError("PackageUnsupportedProfile")
    # Fresh data and random ephemeral secrets; never written into package/logs.
    with tempfile.TemporaryDirectory(prefix="assembly-launch-", dir=scratch) as temporary:
        root = Path(temporary)
        environment = {"PATH": os.environ.get("PATH", "/usr/bin:/bin"), "ZENO_ASSEMBLY_DATA": str(root / "data")}
        credentials = {}
        for party in assembly["configuration"]["parties"]:
            name = party["name"]
            credentials[name] = secrets.token_hex(32)
            path = root / name
            path.write_text(credentials[name] + "\n")
            path.chmod(0o600)
            environment["ZENO_ASSEMBLY_" + name.upper() + "_SECRET"] = str(path)

        def snapshot(name="operator"):
            status, raw = call("/state", credentials[name])
            require(status == 200)
            return json.loads(raw)

        def frame(state, key, payload):
            return (b"\x01" + bytes.fromhex(state["contract"]) + bytes([key]) * 32
                    + int(state["revision"]).to_bytes(8, "big") + payload)

        def stop(process):
            process.terminate()
            try:
                process.wait(timeout=5)
            except subprocess.TimeoutExpired:
                process.kill()
                process.wait()

        def start(log):
            process = subprocess.Popen([sys.executable, package / "launch.py"], env=environment,
                                       stdin=subprocess.DEVNULL, stdout=log, stderr=log)
            try:
                deadline = time.monotonic() + 20
                while True:
                    if process.poll() is not None:
                        raise ValueError("PackageLaunchRefused")
                    try:
                        state = snapshot()
                        return process, state
                    except (OSError, http.client.HTTPException):
                        if time.monotonic() >= deadline:
                            raise ValueError("PackageLaunchUnavailable") from None
                        time.sleep(0.05)
            except BaseException:
                stop(process)
                raise

        with (root / "server.log").open("wb") as log:
            process, state = start(log)
            originals = []
            try:
                require(state["values"] == initial and state["context"] == context)
                require(call("/state", "0" * 64)[0] == 401)
                observer = snapshot("observer")
                payload = json.dumps(observer["values"] + denied + observer["context"]).encode()
                status, raw = call("/preview", credentials["observer"], frame(observer, 23, payload))
                require(status == 200)
                preview = json.loads(raw)
                require(preview["reply"]["class"] == "Reject")
                status, _ = call("/command", credentials["observer"], frame(observer, 23, bytes.fromhex(preview["command"])))
                denied_state = snapshot()
                require(status == 422 and denied_state["values"] == initial)
                require(denied_state["revision"] == state["revision"] and denied_state["pending"] == state["pending"])
                for key, (name, command, expected) in enumerate(steps, 41):
                    before = snapshot(name)
                    payload = json.dumps(before["values"] + command + before["context"]).encode()
                    status, raw = call("/preview", credentials[name], frame(before, key, payload))
                    require(status == 200)
                    preview = json.loads(raw)
                    require(preview["reply"]["class"] == "Accept")
                    original = frame(before, key, bytes.fromhex(preview["command"]))
                    status, raw = call("/command", credentials[name], original)
                    require(status == 200)
                    reply = json.loads(raw)
                    committed = snapshot()
                    require(committed["values"] == expected)
                    require(int(committed["revision"]) == int(before["revision"]) + 1)
                    status, repeated = call("/command", credentials[name], original)
                    require(status == 200 and json.loads(repeated)["packet"] == reply["packet"])
                    repeated_state = snapshot()
                    require(all(repeated_state[field] == committed[field] for field in ["values", "revision", "pending"]))
                    originals.append((name, original, reply["packet"]))
                final = snapshot()
                require(final["pending"] == "0")
                receiver = root / "data/receiver"
                ledger = {p.name: p.read_bytes() for p in receiver.iterdir() if p.is_file()}
                envelopes = [json.loads(data) for name, data in ledger.items() if name.endswith(".json")]
                require(len(envelopes) == 1 and envelopes[0]["destination_text"] == destination)
                # An intent ACK is not success: task success above is an explicit
                # authenticated Adapter observation, never inferred from the file.
            finally:
                stop(process)
            process, reopened = start(log)
            try:
                require(reopened["values"] == final["values"] and reopened["revision"] == final["revision"] and reopened["pending"] == "0")
                for name, original, packet in originals:
                    status, repeated = call("/command", credentials[name], original)
                    require(status == 200 and json.loads(repeated)["packet"] == packet)
                recovered = snapshot()
                require(all(recovered[field] == final[field] for field in ["values", "revision", "pending"]))
                require(recovered["pending"] == "0")
                require({p.name: p.read_bytes() for p in receiver.iterdir() if p.is_file()} == ledger)
            finally:
                stop(process)
    print("package-launch: PASS (exact release binary, loopback, authorization, positive lifecycle, local intent, exact retry and completed-request restart)")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--package", required=True, type=Path)
    parser.add_argument("--scratch", required=True, type=Path)
    args = parser.parse_args()
    check(args.package.resolve(), args.scratch.resolve())
