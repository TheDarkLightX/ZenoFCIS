"""Keep only the scalar API exports in a compiler-produced Wasm module.

LLD exports memory by default even without --export-memory. Rewrite only
the export section, copying every other section and every function export
byte for byte. The build's Node tests independently validate and execute the
finished module and reject imports or any nonfunction export.
"""

FUNCTIONS = frozenset(name.encode() for name in (
    "demo_abi_version", "demo_begin", "demo_write", "demo_step",
    "demo_read", "demo_state", "demo_reset",
))
HEADER = b"\0asm\x01\0\0\0"


def read_u32(data: bytes, offset: int) -> tuple[int, int]:
    value = 0
    for shift in range(0, 35, 7):
        if offset >= len(data):
            raise ValueError("truncated Wasm integer")
        byte = data[offset]
        offset += 1
        value |= (byte & 127) << shift
        if value > 0xffffffff:
            raise ValueError("Wasm integer exceeds u32")
        if byte < 128:
            return value, offset
    raise ValueError("unterminated Wasm integer")


def u32(value: int) -> bytes:
    if not 0 <= value <= 0xffffffff:
        raise ValueError("Wasm integer exceeds u32")
    result = bytearray()
    while value >= 128:
        result.append((value & 127) | 128)
        value >>= 7
    result.append(value)
    return bytes(result)


def private_exports(payload: bytes) -> bytes:
    count, offset = read_u32(payload, 0)
    kept = []
    names = set()
    for _ in range(count):
        start = offset
        length, offset = read_u32(payload, offset)
        end = offset + length
        if end >= len(payload):
            raise ValueError("truncated Wasm export")
        name, kind = payload[offset:end], payload[end]
        index, offset = read_u32(payload, end + 1)
        if name in names:
            raise ValueError("duplicate Wasm export")
        names.add(name)
        if kind == 0 and name in FUNCTIONS:
            kept.append(payload[start:offset])
        elif (name, kind, index) == (b"memory", 2, 0):
            pass
        elif kind == 3 and name in (b"__heap_base", b"__data_end"):
            pass
        else:
            raise ValueError(f"unexpected Wasm export: {name!r}, kind {kind}")
    if offset != len(payload) or not FUNCTIONS.issubset(names):
        raise ValueError("incomplete Wasm export section")
    return u32(len(kept)) + b"".join(kept)


def private_module(module: bytes) -> bytes:
    if module[:8] != HEADER:
        raise ValueError("expected a Wasm version 1 module")
    output = bytearray(HEADER)
    offset = 8
    found = False
    while offset < len(module):
        start, section = offset, module[offset]
        length, payload = read_u32(module, offset + 1)
        end = payload + length
        if end > len(module):
            raise ValueError("truncated Wasm section")
        if section == 7:
            if found:
                raise ValueError("duplicate Wasm export section")
            found = True
            exports = private_exports(module[payload:end])
            output.extend(b"\x07" + u32(len(exports)) + exports)
        else:
            output.extend(module[start:end])
        offset = end
    if not found:
        raise ValueError("missing Wasm export section")
    return bytes(output)
