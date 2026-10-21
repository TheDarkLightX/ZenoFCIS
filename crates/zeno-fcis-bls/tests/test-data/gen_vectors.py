#!/usr/bin/env python3
"""Generate BLS12-381 min_pk test vectors with py_ecc 8.0.0 (G2Basic, G2ProofOfPossession).

Output: vectors.tsv, one case per line, tab separated:
  name  op  dst  a  b  c  expect
Fields a/b/c are comma separated lists of 0x-prefixed hex ("0x" alone is the empty string,
"-" is an empty list).  expect is ACCEPT/REJECT for verifying ops, hex for computing ops.

ops: verify(a=[pk] b=[msg] c=[sig]); fast_aggregate_verify(a=pks b=[msg] c=[sig]);
     aggregate_verify_basic / aggregate_verify_pop(a=pks b=msgs c=[sig]);
     pop_verify(a=[pk] c=[proof], dst=POP_TAG, message is pk);
     key_validate(a=[pk]); sign(a=[sk] b=[msg]); sk_to_pk(a=[sk]); keygen(a=[ikm] b=[info]);
     agg_sigs(a=sigs); agg_pks(a=pks)
"""
import hashlib
import sys

from py_ecc.bls import G2Basic, G2ProofOfPossession
from py_ecc.bls.g2_primitives import (
    G1_to_pubkey,
    G2_to_signature,
    pubkey_to_G1,
    subgroup_check,
)
from py_ecc.bls.point_compression import decompress_G2
from py_ecc.optimized_bls12_381 import G1, add, curve_order, field_modulus as P, is_inf, multiply, neg
from py_ecc.fields import optimized_bls12_381_FQ as FQ

POP = G2ProofOfPossession
BASIC = G2Basic
DST_NUL = BASIC.DST
DST_POP = POP.DST
POP_TAG = POP.POP_TAG

rows = []


def h(b):
    return "0x" + b.hex()


def lst(items):
    return ",".join(h(x) for x in items) if items else "-"


def add_row(name, op, dst, a, b, c, expect):
    rows.append("\t".join([name, op, h(dst), lst(a), lst(b), lst(c), expect]))


def acc(v):
    return "ACCEPT" if v else "REJECT"


def sk_bytes(sk):
    return sk.to_bytes(32, "big")


# ---- keys -------------------------------------------------------------------
ikms = [hashlib.sha256(b"zf26-blst-compat-ikm-%d" % i).digest() for i in range(4)]
sks = [BASIC.KeyGen(ikm) for ikm in ikms]  # 3 signers + attacker
pks = [BASIC.SkToPk(sk) for sk in sks]
sk1, sk2, sk3, ska = sks
pk1, pk2, pk3, pka = pks
assert all(BASIC.KeyValidate(p) for p in pks)

m = [b"zeno-oracle round 1: price=4217", b"zeno-oracle round 2: price=4219", b"zeno-oracle round 3: price=4221"]
m_empty = b""
m_long = bytes((i * 7 + 3) % 256 for i in range(1000))
m_same = b"zeno-oracle round 9: median=4220"

# ---- parity: key derivation, pk, sign --------------------------------------
for i, (ikm, sk) in enumerate(zip(ikms, sks)):
    add_row(f"keygen-parity-{i}", "keygen", b"", [ikm], [b""], [], h(sk_bytes(sk)))
add_row("keygen-parity-info", "keygen", b"", [ikms[0]], [b"zf26 key info"], [], h(sk_bytes(BASIC.KeyGen(ikms[0], b"zf26 key info"))))
for i, (sk, pk) in enumerate(zip(sks, pks)):
    add_row(f"sk-to-pk-parity-{i}", "sk_to_pk", b"", [sk_bytes(sk)], [], [], h(pk))
for i in range(3):
    add_row(f"sign-parity-basic-{i}", "sign", DST_NUL, [sk_bytes(sks[i])], [m[i]], [], h(BASIC.Sign(sks[i], m[i])))
    add_row(f"sign-parity-pop-{i}", "sign", DST_POP, [sk_bytes(sks[i])], [m_same], [], h(POP.Sign(sks[i], m_same)))
    add_row(f"sign-parity-poptag-{i}", "sign", POP_TAG, [sk_bytes(sks[i])], [pks[i]], [], h(POP.PopProve(sks[i])))
add_row("sign-parity-empty-msg", "sign", DST_NUL, [sk_bytes(sk1)], [m_empty], [], h(BASIC.Sign(sk1, m_empty)))
add_row("sign-parity-long-msg", "sign", DST_NUL, [sk_bytes(sk1)], [m_long], [], h(BASIC.Sign(sk1, m_long)))

# ---- G2Basic (DST ..._NUL_) -------------------------------------------------
sb = [BASIC.Sign(sks[i], m[i]) for i in range(3)]
for i in range(3):
    assert BASIC.Verify(pks[i], m[i], sb[i])
    add_row(f"basic-verify-{i+1}", "verify", DST_NUL, [pks[i]], [m[i]], [sb[i]], acc(BASIC.Verify(pks[i], m[i], sb[i])))
se = BASIC.Sign(sk1, m_empty)
add_row("basic-verify-empty-msg", "verify", DST_NUL, [pk1], [m_empty], [se], acc(BASIC.Verify(pk1, m_empty, se)))
sl = BASIC.Sign(sk1, m_long)
add_row("basic-verify-long-msg-1000B", "verify", DST_NUL, [pk1], [m_long], [sl], acc(BASIC.Verify(pk1, m_long, sl)))

agg_b = BASIC.Aggregate(sb)
add_row("basic-aggregate-sigs-3", "agg_sigs", b"", sb, [], [], h(agg_b))
add_row("basic-aggregate-verify-3-distinct", "aggregate_verify_basic", DST_NUL, [pk1, pk2, pk3], m, [agg_b], acc(BASIC.AggregateVerify([pk1, pk2, pk3], m, agg_b)))
# duplicate messages: draft Basic scheme forbids; py_ecc rejects.  Signatures are individually valid.
sdup = [BASIC.Sign(sks[0], m[0]), BASIC.Sign(sks[1], m[0]), BASIC.Sign(sks[2], m[0])]
adup = BASIC.Aggregate(sdup)
exp = acc(BASIC.AggregateVerify([pk1, pk2, pk3], [m[0]] * 3, adup))
assert exp == "REJECT"
add_row("basic-aggregate-verify-duplicate-msgs", "aggregate_verify_basic", DST_NUL, [pk1, pk2, pk3], [m[0]] * 3, [adup], exp)
# wrong message / wrong key / wrong dst / tampered sig / one signer missing
assert not BASIC.Verify(pk1, m[1], sb[0])
add_row("basic-neg-wrong-message", "verify", DST_NUL, [pk1], [m[1]], [sb[0]], acc(BASIC.Verify(pk1, m[1], sb[0])))
assert not BASIC.Verify(pk2, m[0], sb[0])
add_row("basic-neg-wrong-pubkey", "verify", DST_NUL, [pk2], [m[0]], [sb[0]], acc(BASIC.Verify(pk2, m[0], sb[0])))
assert not POP.Verify(pk1, m[0], sb[0])
add_row("basic-neg-wrong-dst-pop-dst", "verify", DST_POP, [pk1], [m[0]], [sb[0]], acc(POP.Verify(pk1, m[0], sb[0])))
sp0 = POP.Sign(sk1, m[0])
assert not BASIC.Verify(pk1, m[0], sp0)
add_row("pop-neg-wrong-dst-nul-dst", "verify", DST_NUL, [pk1], [m[0]], [sp0], acc(BASIC.Verify(pk1, m[0], sp0)))
dst_bad = b"BLS_SIG_BLS12381G2_XMD:SHA-256_SSWU_RO_NUL"
add_row("basic-neg-wrong-dst-truncated", "verify", dst_bad, [pk1], [m[0]], [sb[0]], acc(BASIC._CoreVerify(pk1, m[0], sb[0], dst_bad)))
tam = bytearray(sb[0]); tam[60] ^= 0x01; tam = bytes(tam)
add_row("basic-neg-tampered-sig-bit", "verify", DST_NUL, [pk1], [m[0]], [tam], acc(BASIC.Verify(pk1, m[0], tam)))
bad_agg = BASIC.AggregateVerify([pk1, pk2, pk3], [m[0], m[1], m[2]], BASIC.Aggregate([sb[0], sb[1]]))
assert not bad_agg
add_row("basic-neg-aggregate-missing-signer", "aggregate_verify_basic", DST_NUL, [pk1, pk2, pk3], m, [BASIC.Aggregate([sb[0], sb[1]])], "REJECT")
add_row("basic-neg-aggregate-wrong-msg-order", "aggregate_verify_basic", DST_NUL, [pk1, pk2, pk3], [m[1], m[0], m[2]], [agg_b], "REJECT")

# ---- G2ProofOfPossession (DST ..._POP_, POP_TAG) ----------------------------
sp = [POP.Sign(sks[i], m_same) for i in range(3)]
for i in range(3):
    add_row(f"pop-verify-{i+1}", "verify", DST_POP, [pks[i]], [m_same], [sp[i]], acc(POP.Verify(pks[i], m_same, sp[i])))
proofs = [POP.PopProve(sk) for sk in sks]
for i in range(3):
    assert POP.PopVerify(pks[i], proofs[i])
    add_row(f"pop-proof-verify-{i+1}", "pop_verify", POP_TAG, [pks[i]], [], [proofs[i]], acc(POP.PopVerify(pks[i], proofs[i])))
assert not POP.PopVerify(pk1, proofs[1])
add_row("pop-proof-neg-other-signers-proof", "pop_verify", POP_TAG, [pk1], [], [proofs[1]], "REJECT")
bad_tag_proof = POP.Sign(sk1, pk1)  # signed with the ordinary POP_ DST instead of POP_TAG: domain separation
assert not POP.PopVerify(pk1, bad_tag_proof)
add_row("pop-proof-neg-wrong-domain", "pop_verify", POP_TAG, [pk1], [], [bad_tag_proof], "REJECT")

agg_p = POP.Aggregate(sp)
add_row("pop-aggregate-sigs-3", "agg_sigs", b"", sp, [], [], h(agg_p))
agg_pk = POP._AggregatePKs([pk1, pk2, pk3])
add_row("pop-aggregate-pks-3", "agg_pks", b"", [pk1, pk2, pk3], [], [], h(agg_pk))
add_row("pop-fast-aggregate-verify-3", "fast_aggregate_verify", DST_POP, [pk1, pk2, pk3], [m_same], [agg_p], acc(POP.FastAggregateVerify([pk1, pk2, pk3], m_same, agg_p)))
assert POP.FastAggregateVerify([pk1, pk2, pk3], m_same, agg_p)
add_row("pop-fast-aggregate-neg-wrong-msg", "fast_aggregate_verify", DST_POP, [pk1, pk2, pk3], [m[0]], [agg_p], acc(POP.FastAggregateVerify([pk1, pk2, pk3], m[0], agg_p)))
agg_p2 = POP.Aggregate(sp[:2])
add_row("pop-fast-aggregate-neg-missing-signer", "fast_aggregate_verify", DST_POP, [pk1, pk2, pk3], [m_same], [agg_p2], acc(POP.FastAggregateVerify([pk1, pk2, pk3], m_same, agg_p2)))
add_row("pop-fast-aggregate-neg-extra-pk", "fast_aggregate_verify", DST_POP, [pk1, pk2, pk3, pka], [m_same], [agg_p], acc(POP.FastAggregateVerify([pk1, pk2, pk3, pka], m_same, agg_p)))
# duplicate pk (allowed with PoP): signer 1 twice
agg_dup = POP.Aggregate([sp[0], sp[0]])
add_row("pop-fast-aggregate-same-pk-twice", "fast_aggregate_verify", DST_POP, [pk1, pk1], [m_same], [agg_dup], acc(POP.FastAggregateVerify([pk1, pk1], m_same, agg_dup)))
# POP aggregate verify over distinct messages and over repeated messages
sd = [POP.Sign(sks[i], m[i]) for i in range(3)]
agg_pd = POP.Aggregate(sd)
add_row("pop-aggregate-verify-3-distinct", "aggregate_verify_pop", DST_POP, [pk1, pk2, pk3], m, [agg_pd], acc(POP.AggregateVerify([pk1, pk2, pk3], m, agg_pd)))
add_row("pop-aggregate-verify-3-same-msg", "aggregate_verify_pop", DST_POP, [pk1, pk2, pk3], [m_same] * 3, [agg_p], acc(POP.AggregateVerify([pk1, pk2, pk3], [m_same] * 3, agg_p)))
add_row("pop-aggregate-neg-wrong-msg", "aggregate_verify_pop", DST_POP, [pk1, pk2, pk3], [m[0], m[0], m[2]], [agg_pd], acc(POP.AggregateVerify([pk1, pk2, pk3], [m[0], m[0], m[2]], agg_pd)))

# ---- rogue key / PoP missing -------------------------------------------------
a_pt = multiply(G1, ska)
rogue_pt = add(a_pt, neg(pubkey_to_G1(pk1)))
pk_rogue = G1_to_pubkey(rogue_pt)
assert POP.KeyValidate(pk_rogue)
sig_rogue = POP.Sign(ska, m_same)  # = a*H(m); verifies under pk1 + pk_rogue = a*G1
assert POP.FastAggregateVerify([pk1, pk_rogue], m_same, sig_rogue)
add_row("rogue-key-fast-aggregate-math-accepts", "fast_aggregate_verify", DST_POP, [pk1, pk_rogue], [m_same], [sig_rogue], "ACCEPT")
# attacker's best proofs for pk_rogue (it does not know its discrete log)
proof_try1 = POP.PopProve(ska)  # proof for pk_a, not pk_rogue
proof_try2 = POP._CoreSign(ska, pk_rogue, POP_TAG)  # a*H_pop(pk_rogue): wrong key
for nm, pr in (("proof-for-other-key", proof_try1), ("proof-signed-with-a", proof_try2)):
    assert not POP.PopVerify(pk_rogue, pr)
    add_row(f"rogue-key-pop-neg-{nm}", "pop_verify", POP_TAG, [pk_rogue], [], [pr], "REJECT")

# ---- key validation / encoding edge cases -----------------------------------
for i in range(3):
    add_row(f"key-valid-{i+1}", "key_validate", b"", [pks[i]], [], [], "ACCEPT")
inf_pk = bytes([0xC0]) + bytes(47)
assert not BASIC.KeyValidate(inf_pk)
add_row("key-neg-infinity", "key_validate", b"", [inf_pk], [], [], "REJECT")
inf_sig = bytes([0xC0]) + bytes(95)
add_row("verify-neg-infinity-pk-infinity-sig", "verify", DST_NUL, [inf_pk], [m[0]], [inf_sig], acc(BASIC.Verify(inf_pk, m[0], inf_sig)))
add_row("verify-neg-infinity-pk-valid-sig", "verify", DST_NUL, [inf_pk], [m[0]], [sb[0]], acc(BASIC.Verify(inf_pk, m[0], sb[0])))
add_row("verify-neg-valid-pk-infinity-sig", "verify", DST_NUL, [pk1], [m[0]], [inf_sig], acc(BASIC.Verify(pk1, m[0], inf_sig)))
add_row("pop-fast-aggregate-neg-infinity-pk-member", "fast_aggregate_verify", DST_POP, [pk1, inf_pk], [m_same], [sp[0]], acc(POP.FastAggregateVerify([pk1, inf_pk], m_same, sp[0])))

# non-subgroup G1 point on E(Fp): y^2 = x^3 + 4, cofactor h != 1
def find_g1_nonsubgroup():
    x = 7
    while True:
        rhs = (pow(x, 3, P) + 4) % P
        y = pow(rhs, (P + 1) // 4, P)
        if y * y % P == rhs:
            pt = (FQ(x), FQ(y), FQ(1))
            if not is_inf(multiply(pt, curve_order)):
                a_flag = 1 if y * 2 > P else 0
                z = x | (a_flag << 381) | (1 << 383)
                return z.to_bytes(48, "big")
        x += 1

ns_pk = find_g1_nonsubgroup()
assert not BASIC.KeyValidate(ns_pk)
add_row("key-neg-non-subgroup-g1", "key_validate", b"", [ns_pk], [], [], "REJECT")
add_row("verify-neg-non-subgroup-pk", "verify", DST_NUL, [ns_pk], [m[0]], [sb[0]], acc(BASIC.Verify(ns_pk, m[0], sb[0])))

# not on curve: x with x^3+4 a non-residue
def find_g1_off_curve():
    x = 5
    while True:
        rhs = (pow(x, 3, P) + 4) % P
        y = pow(rhs, (P + 1) // 4, P)
        if y * y % P != rhs:
            return ((1 << 383) | x).to_bytes(48, "big")
        x += 1

off_pk = find_g1_off_curve()
assert not BASIC.KeyValidate(off_pk)
add_row("key-neg-not-on-curve", "key_validate", b"", [off_pk], [], [], "REJECT")
# encoding faults
unc = bytearray(pk1); unc[0] &= 0x7F
add_row("key-neg-compression-flag-cleared", "key_validate", b"", [bytes(unc)], [], [], acc(BASIC.KeyValidate(bytes(unc))))
xge = (((1 << 383) | P)).to_bytes(48, "big")
add_row("key-neg-x-equals-p-noncanonical", "key_validate", b"", [xge], [], [], acc(BASIC.KeyValidate(xge)))
infbody = bytearray(inf_pk); infbody[47] = 1
add_row("key-neg-infinity-flag-nonzero-body", "key_validate", b"", [bytes(infbody)], [], [], acc(BASIC.KeyValidate(bytes(infbody))))
infa = bytearray(inf_pk); infa[0] |= 0x20
add_row("key-neg-infinity-with-sign-flag", "key_validate", b"", [bytes(infa)], [], [], acc(BASIC.KeyValidate(bytes(infa))))
add_row("key-neg-length-47", "key_validate", b"", [pk1[:47]], [], [], acc(BASIC.KeyValidate(pk1[:47])))
add_row("key-neg-length-49", "key_validate", b"", [pk1 + b"\x00"], [], [], acc(BASIC.KeyValidate(pk1 + b"\x00")))
add_row("verify-neg-sig-length-95", "verify", DST_NUL, [pk1], [m[0]], [sb[0][:95]], acc(BASIC.Verify(pk1, m[0], sb[0][:95])))
add_row("verify-neg-sig-length-97", "verify", DST_NUL, [pk1], [m[0]], [sb[0] + b"\x00"], acc(BASIC.Verify(pk1, m[0], sb[0] + b"\x00")))
sunc = bytearray(sb[0]); sunc[0] &= 0x7F
add_row("verify-neg-sig-compression-flag-cleared", "verify", DST_NUL, [pk1], [m[0]], [bytes(sunc)], acc(BASIC.Verify(pk1, m[0], bytes(sunc))))

# non-subgroup G2 signature point on the twist E'(Fp2)
def find_g2_nonsubgroup():
    x1 = 3
    while True:
        z1 = (1 << 383) | x1
        for x0 in range(1, 40):
            try:
                pt = decompress_G2((z1, x0))
            except (ValueError, AssertionError, ZeroDivisionError):
                continue
            if not subgroup_check(pt):
                return G2_to_signature(pt)
        x1 += 1

ns_sig = find_g2_nonsubgroup()
add_row("verify-neg-non-subgroup-sig", "verify", DST_NUL, [pk1], [m[0]], [ns_sig], acc(BASIC.Verify(pk1, m[0], ns_sig)))
add_row("aggregate-sigs-neg-non-subgroup-member", "agg_sigs", b"", [sb[0], ns_sig], [], [], "REJECT")
# off-curve G2: x with no y
def find_g2_off_curve():
    x1 = 3
    while True:
        z1 = (1 << 383) | x1
        for x0 in range(1, 40):
            try:
                decompress_G2((z1, x0))
            except (ValueError, AssertionError, ZeroDivisionError):
                return z1.to_bytes(48, "big") + x0.to_bytes(48, "big")
        x1 += 1

off_sig = find_g2_off_curve()
add_row("verify-neg-sig-not-on-curve", "verify", DST_NUL, [pk1], [m[0]], [off_sig], acc(BASIC.Verify(pk1, m[0], off_sig)))

# ---- small-order (torsion) components: validly-signed message, key/sig shifted by a torsion point ----
from py_ecc.optimized_bls12_381 import Z1, Z2
from py_ecc.bls.g2_primitives import signature_to_G2
H1 = 0x396C8C005555E1568C00AAAB0000AAAB
H2 = 0x5D543A95414E7F1091D50792876A202CD91DE4547085ABAA68A205B2E5A7DDFA628F1CB4D9E82EF21537E293A6691AE1616EC6E786F0C70CF1C38E31C7238E5


def g1_torsion3():
    x = 11
    while True:
        rhs = (pow(x, 3, P) + 4) % P
        y = pow(rhs, (P + 1) // 4, P)
        if y * y % P == rhs:
            t = multiply((FQ(x), FQ(y), FQ(1)), curve_order * (H1 // 3))
            if not is_inf(t):
                assert is_inf(multiply(t, 3))
                return t
        x += 1


def g2_torsion13():
    x1 = 3
    while True:
        z1 = (1 << 383) | x1
        for x0 in range(1, 40):
            try:
                pt = decompress_G2((z1, x0))
            except (ValueError, AssertionError, ZeroDivisionError):
                continue
            u = multiply(pt, curve_order * (H2 // 169))  # 13-primary component, order | 169
            t = u if is_inf(multiply(u, 13)) else multiply(u, 13)
            if not is_inf(t):
                assert is_inf(multiply(t, 13))
                return t
        x1 += 1


t3 = g1_torsion3()
pk_t = G1_to_pubkey(add(pubkey_to_G1(pk1), t3))
assert not BASIC.KeyValidate(pk_t)
add_row("verify-neg-torsion-pk-order3", "verify", DST_NUL, [pk_t], [m[0]], [sb[0]], acc(BASIC.Verify(pk_t, m[0], sb[0])))
t13 = g2_torsion13()
sig_t = G2_to_signature(add(signature_to_G2(sb[0]), t13))
add_row("verify-neg-torsion-sig-order13", "verify", DST_NUL, [pk1], [m[0]], [sig_t], acc(BASIC.Verify(pk1, m[0], sig_t)))

with open(sys.argv[1], "w") as f:
    f.write("\n".join(rows) + "\n")
print(len(rows), "cases written to", sys.argv[1])
