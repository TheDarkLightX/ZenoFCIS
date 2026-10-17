# Fitting fused quotient

`mul_div_floor(MAX, MAX, MAX)` returns `MAX`, with `MAX = 2^63 - 1`.
The standalone checked expression `div_floor(MAX * MAX, MAX)` refuses at its
multiplication. The two graphs are distinct and must not be rewritten into
one another. The law's checked i128 multiplication accommodates this pair of
i64 operands and independently checks the floor quotient.

This full-size example has a domain beyond the finite checker's tuple limit.
Its listed boundary inputs and the operator refinement are separate evidence;
there is no complete application-domain theorem. Generation, execution and
replay on the changed development source remain pending qualification.
