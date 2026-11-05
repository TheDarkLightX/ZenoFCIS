"""Research-only transport experiments across mathematical representations.

Three distinct cases deliberately remain distinct:
1. An invertible algebra isomorphism for convolution over F_97.
2. Restricted syntactic expressibility (additive vs polynomial grammar).
3. A semantics-changing quotient map Z -> Z/97Z, rejected for equality.

All evidence is bounded or formula-specific and grants NO authority to ZAL.
"""
from __future__ import annotations

from dataclasses import dataclass
from hashlib import sha256
import json
from typing import Callable

PRIME = 97
GENERATOR = 5
LENGTHS = (2, 4, 8, 16, 32)


def digest(value: object) -> str:
    data = json.dumps(value, separators=(",", ":"), ensure_ascii=True)
    return sha256(data.encode("ascii")).hexdigest()


def _validate_vector(data: tuple[int, ...]) -> int:
    if (type(data) is not tuple or len(data) not in LENGTHS
            or any(type(x) is not int or not 0 <= x < PRIME for x in data)):
        raise ValueError("expected canonical F_97 vector of supported length")
    return len(data)


def primitive_root(length: int) -> int:
    if type(length) is not int or length not in LENGTHS:
        raise ValueError("unsupported transform size")
    root = pow(GENERATOR, (PRIME - 1) // length, PRIME)
    if pow(root, length, PRIME) != 1 or pow(root, length // 2, PRIME) == 1:
        raise RuntimeError("transform root does not have exact order")
    return root


def direct_cyclic_convolution(left: tuple[int, ...], right: tuple[int, ...]) -> tuple[int, ...]:
    n = _validate_vector(left)
    if _validate_vector(right) != n:
        raise ValueError("incompatible convolution lengths")
    return tuple(sum(left[j] * right[(k - j) % n] for j in range(n)) % PRIME for k in range(n))


def direct_transform(data: tuple[int, ...], inverse: bool = False) -> tuple[int, ...]:
    """Reference O(n^2) discrete Fourier transform; independent of FFT algorithm."""
    n = _validate_vector(data)
    w = primitive_root(n)
    if inverse:
        w = pow(w, -1, PRIME)
    factor = pow(n, -1, PRIME) if inverse else 1
    return tuple((factor * sum(data[j] * pow(w, j * k, PRIME)
                                for j in range(n))) % PRIME for k in range(n))


def fast_transform(data: tuple[int, ...], inverse: bool = False) -> tuple[tuple[int, ...], int]:
    """Radix-2 NTT with explicit data-multiply accounting.

    Costs count butterfly data products and inverse scaling only. Twiddle
    generation, address calculation, adds and Python runtime are excluded.
    """
    n = _validate_vector(data)
    omega = primitive_root(n)
    if inverse:
        omega = pow(omega, -1, PRIME)
    values = list(data)
    j = 0
    for i in range(1, n):
        bit = n >> 1
        while j & bit:
            j ^= bit
            bit >>= 1
        j ^= bit
        if i < j:
            values[i], values[j] = values[j], values[i]
    size = 2
    multiplies = 0
    while size <= n:
        half = size // 2
        step = pow(omega, n // size, PRIME)
        twiddles = [pow(step, k, PRIME) for k in range(half)]
        for offset in range(0, n, size):
            for k, twiddle in enumerate(twiddles):
                even = values[offset + k]
                odd = values[offset + k + half] * twiddle % PRIME
                multiplies += 1
                values[offset + k] = (even + odd) % PRIME
                values[offset + k + half] = (even - odd) % PRIME
        size *= 2
    if inverse:
        scale = pow(n, -1, PRIME)
        values = [(value * scale) % PRIME for value in values]
        multiplies += n
    return tuple(values), multiplies


@dataclass(frozen=True)
class ConvolutionReceipt:
    source_semantics: str
    target_semantics: str
    relation: str
    left_digest: str
    right_digest: str
    spectral_product: tuple[int, ...]
    output: tuple[int, ...]
    direct_data_multiplies: int
    transformed_data_multiplies: int
    authority: str = "none"


def transport_convolution(left: tuple[int, ...], right: tuple[int, ...]) -> ConvolutionReceipt:
    n = _validate_vector(left)
    if _validate_vector(right) != n:
        raise ValueError("mismatched transform lengths")
    a, a_ops = fast_transform(left)
    b, b_ops = fast_transform(right)
    spectral = tuple(x * y % PRIME for x, y in zip(a, b))
    output, inverse_ops = fast_transform(spectral, inverse=True)
    return ConvolutionReceipt(
        "cyclic-convolution/F_97^n", "pointwise-product/F_97^n",
        "invertible-ring-isomorphism", digest(left), digest(right), spectral, output,
        n * n, a_ops + b_ops + n + inverse_ops,
    )


def replay_convolution(left: tuple[int, ...], right: tuple[int, ...],
                       receipt: ConvolutionReceipt) -> bool:
    """Independent source convolution + direct DFT oracle + inverse transport."""
    if type(receipt) is not ConvolutionReceipt or receipt.authority != "none":
        return False
    try:
        n = _validate_vector(left)
        if _validate_vector(right) != n:
            return False
        if (receipt.source_semantics != "cyclic-convolution/F_97^n"
                or receipt.target_semantics != "pointwise-product/F_97^n"
                or receipt.relation != "invertible-ring-isomorphism"
                or receipt.left_digest != digest(left) or receipt.right_digest != digest(right)
                or type(receipt.output) is not tuple or type(receipt.spectral_product) is not tuple
                or _validate_vector(receipt.output) != n
                or _validate_vector(receipt.spectral_product) != n):
            return False
        reference = direct_cyclic_convolution(left, right)
        expected_spectral = tuple((a*b) % PRIME for a,b in zip(
            direct_transform(left), direct_transform(right)))
        if (receipt.output != reference or receipt.spectral_product != expected_spectral
                or direct_transform(receipt.spectral_product, inverse=True) != reference):
            return False
        # Independently specified operation-accounting identity.
        stages = n.bit_length() - 1
        return (type(receipt.direct_data_multiplies) is int
                and type(receipt.transformed_data_multiplies) is int
                and receipt.direct_data_multiplies == n*n
                and receipt.transformed_data_multiplies == 3*(n//2)*stages + 2*n)
    except (ValueError, TypeError, OverflowError):
        return False


def additive_language_admits(expr) -> bool:
    """Intentionally narrow syntax gate, NOT general Presburger expressibility."""
    import representation_cosearch as features
    if not features.valid_expr(expr, 3):
        return False
    todo = [expr]
    while todo:
        node = todo.pop()
        if node.op == "*":
            return False
        todo.extend(node.args)
    return True


@dataclass(frozen=True)
class InterpretationResult:
    source: str
    target: str
    classification: str
    witness: tuple[int, int] | None
    source_truth: bool | None
    target_truth: bool | None
    status: str


def check_equality_modulo(a: int, b: int) -> InterpretationResult:
    """Detect counterexample to 'integer equality iff equal residues'."""
    if type(a) is not int or type(b) is not int or not -1000 <= a <= 1000 or not -1000 <= b <= 1000:
        raise ValueError("bounded integer contract")
    source = a == b
    target = (a - b) % PRIME == 0
    agrees = source == target
    return InterpretationResult("integer-equality/Z", "residue-equality/F_97",
                                "quotient-not-an-isomorphism-of-integer-equality",
                                None if agrees else (a, b), source, target,
                                "instance-agrees-not-general-proof" if agrees else "refuted-by-witness")


def changing_axioms_without_interpretation(source: str, target: str) -> InterpretationResult:
    if (source, target) != ("Euclidean-geometry", "hyperbolic-geometry"):
        raise ValueError("unregistered theory pair")
    return InterpretationResult(source, target, "change-of-axioms", None, None, None,
                                "unproved-no-transport-map")


def demo() -> dict:
    output = {}
    for n in LENGTHS:
        left = tuple((j*j + 2*j) % PRIME for j in range(n))
        right = tuple((3*j + 1) % PRIME for j in range(n))
        receipt = transport_convolution(left, right)
        output[str(n)] = {
            "direct_multiplies": receipt.direct_data_multiplies,
            "transformed_multiplies": receipt.transformed_data_multiplies,
            "direct_replay": replay_convolution(left, right, receipt),
            "source_semantics": receipt.source_semantics,
            "transport": receipt.relation,
        }
    return {
        "schema": "zeno-fcis/representation-transport/1",
        "authority": "none",
        "convolution": output,
        "semantics_change": check_equality_modulo(0, PRIME).__dict__,
        "axioms_change": changing_axioms_without_interpretation(
            "Euclidean-geometry", "hyperbolic-geometry").__dict__,
    }


if __name__ == "__main__":
    print(json.dumps(demo(), indent=2, sort_keys=True))
