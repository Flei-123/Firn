;; ROUND INLINE -- the cross check against round SCHLEUSE.
;;
;; The SAME algorithm as kernel/app/prim.fi of the Osum tree: count the primes
;; below a limit by trial division. Deliberately a pure compute loop without
;; system calls -- what is being measured is the INTERPRETER, not the file
;; system. Trial division and not a sieve because a sieve is memory bound, and
;; then one measures the guest's memory instead of its instructions.
;;
;; The limit is compiled in (200000, the limit round SCHLEUSE measured with).
;; The result is handed back through `proc_exit`, so no WASI write is needed:
;; the count of primes below 200000 is 17984, and 17984 mod 256 = 64.
(module
  (import "wasi_snapshot_preview1" "proc_exit" (func $exit (param i32)))
  (memory 1)
  (export "memory" (memory 0))

  ;; is_prime(n) -> i32
  (func $is_prime (param $n i64) (result i32)
    (local $d i64)
    (if (i64.lt_u (local.get $n) (i64.const 2))
      (then (return (i32.const 0))))
    (if (i64.eqz (i64.rem_u (local.get $n) (i64.const 2)))
      (then (return (i64.eq (local.get $n) (i64.const 2)))))
    (local.set $d (i64.const 3))
    (block $done
      (loop $loop
        (br_if $done
          (i64.gt_u (i64.mul (local.get $d) (local.get $d)) (local.get $n)))
        (if (i64.eqz (i64.rem_u (local.get $n) (local.get $d)))
          (then (return (i32.const 0))))
        (local.set $d (i64.add (local.get $d) (i64.const 2)))
        (br $loop)))
    (i32.const 1))

  (func $main (export "_start")
    (local $i i64)
    (local $c i32)
    (local.set $i (i64.const 0))
    (local.set $c (i32.const 0))
    (block $end
      (loop $l
        (br_if $end (i64.ge_u (local.get $i) (i64.const 200000)))
        (if (call $is_prime (local.get $i))
          (then (local.set $c (i32.add (local.get $c) (i32.const 1)))))
        (local.set $i (i64.add (local.get $i) (i64.const 1)))
        (br $l)))
    ;; 17984 primes below 200000; hand the low byte back as the exit code
    (call $exit (i32.and (local.get $c) (i32.const 255))))
)
