(module
  ;; Real Wasm arithmetic source, not a hand-written CLIF fixture.
  (func $sat (export "sat") (param $x i64) (param $y i64) (result i64)
    (local $sum i64) (local $overflow i32)
    local.get $x
    local.get $y
    i64.add
    local.set $sum
    local.get $sum
    local.get $x
    i64.xor
    local.get $sum
    local.get $y
    i64.xor
    i64.and
    i64.const 0
    i64.lt_s
    local.set $overflow
    i64.const -9223372036854775808
    i64.const 9223372036854775807
    local.get $x
    i64.const 0
    i64.lt_s
    select
    local.get $sum
    local.get $overflow
    select
  )
  (func (export "workload") (param $n i32) (result i64)
    (local $i i32) (local $acc i64)
    i64.const 1
    local.set $acc
    (block $done
      (loop $loop
        local.get $i
        local.get $n
        i32.ge_u
        br_if $done
        local.get $acc
        i64.const 9223372036854775800
        i64.const -9223372036854775799
        local.get $i
        i32.const 1
        i32.and
        select
        call $sat
        local.set $acc
        local.get $i
        i32.const 1
        i32.add
        local.set $i
        br $loop
      )
    )
    local.get $acc
  )
)
