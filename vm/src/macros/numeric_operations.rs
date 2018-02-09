#![macro_use]

macro_rules! to_expr {
    ($e: expr) => ($e);
}

/// Performs an integer operation that won't overflow.
///
/// This macro takes the following arguments:
///
/// * `$process`: the process that is performing the operation.
/// * `$context`: the current ExecutionContext.
/// * `$proto`: the prototype (as an ObjectPointer) to use for allocations.
/// * `$instruction`: the instruction that is executed.
/// * `$op`: the function to use for the operation.
macro_rules! integer_op {
    (
        $process: expr,
        $context: expr,
        $proto: expr,
        $instruction: expr,
        $op: tt
    ) => ({

        let register = $instruction.arg(0);
        let rec_ptr = $context.get_register($instruction.arg(1));
        let arg_ptr = $context.get_register($instruction.arg(2));

        let pointer = if rec_ptr.is_integer() && arg_ptr.is_integer() {
            // int | int

            let rec = rec_ptr.integer_value()?;
            let arg = arg_ptr.integer_value()?;
            let result = to_expr!(rec $op arg);

            if ObjectPointer::integer_too_large(result) {
                $process.allocate(object_value::integer(result), $proto)
            } else {
                ObjectPointer::integer(result)
            }
        } else if rec_ptr.is_integer() && arg_ptr.is_bigint() {
            // int | bigint

            let rec = Integer::from(rec_ptr.integer_value()?);
            let arg = arg_ptr.bigint_value()?;
            let result = to_expr!(rec $op arg);

            $process.allocate(object_value::bigint(result), $proto)
        } else if rec_ptr.is_bigint() && arg_ptr.is_integer() {
            // bigint | int

            let rec = rec_ptr.bigint_value()?.clone();
            let arg = arg_ptr.integer_value()?;

            let result = if arg_ptr.is_in_i32_range() {
                to_expr!(rec $op arg as i32)
            } else {
                to_expr!(rec $op Integer::from(arg))
            };

            $process.allocate(object_value::bigint(result), $proto)
        } else if rec_ptr.is_bigint() && arg_ptr.is_bigint() {
            // bigint | bigint

            let rec = rec_ptr.bigint_value()?.clone();
            let arg = arg_ptr.bigint_value()?;
            let result = to_expr!(rec $op arg);

            $process.allocate(object_value::bigint(result), $proto)
        } else {
            return Err(
                "Integer instructions can only be performed using integers"
                    .to_string()
            );
        };

        $context.set_register(register, pointer);
    });
}

/// Performs an integer shift operation that may overflow.
///
/// This macro takes the following arguments:
///
/// * `$process`: the process that is performing the operation.
/// * `$context`: the current ExecutionContext.
/// * `$proto`: the prototype (as an ObjectPointer) to use for allocations.
/// * `$instruction`: the instruction that is executed.
/// * `$int_op`: the function to use for shifting an integer.
/// * `$bigint_op`: the function to use for shifting a big integer.
macro_rules! integer_shift_op {
    (
        $process: expr,
        $context: expr,
        $proto: expr,
        $instruction: expr,
        $int_op: ident,
        $bigint_op: ident
    ) => ({
        let register = $instruction.arg(0);
        let rec_ptr = $context.get_register($instruction.arg(1));
        let arg_ptr = $context.get_register($instruction.arg(2));

        let pointer = if rec_ptr.is_integer() {
            integer_operations::$int_op($process, rec_ptr, arg_ptr, $proto)?
        } else if rec_ptr.is_bigint() {
            integer_operations::$bigint_op($process, rec_ptr, arg_ptr, $proto)?
        } else {
            return Err("Integer shifting only works with integers".to_string());
        };

        $context.set_register(register, pointer);
    });
}

/// Performs an integer binary operation that may overflow into a bigint.
///
/// This macro takes the following arguments:
///
/// * `$process`: the process that is performing the operation.
/// * `$context`: the current ExecutionContext.
/// * `$proto`: the prototype (as an ObjectPointer) to use for allocations.
/// * `$instruction`: the instruction that is executed.
/// * `$op`: the binary operator to use for non overflowing operations.
/// * `$overflow`: the method to use for an overflowing operation.
macro_rules! integer_overflow_op {
    (
        $process: expr,
        $context: expr,
        $proto: expr,
        $instruction: expr,
        $op: tt,
        $overflow: ident
    ) => ({
        let register = $instruction.arg(0);
        let rec_ptr = $context.get_register($instruction.arg(1));
        let arg_ptr = $context.get_register($instruction.arg(2));

        let result = if rec_ptr.is_integer() && arg_ptr.is_integer() {
            // Example: int + int -> int
            //
            // This will produce a bigint if the produced integer overflowed or
            // doesn't fit in a tagged pointer.

            let rec = rec_ptr.integer_value()?;
            let arg = arg_ptr.integer_value()?;
            let (result, overflowed) = rec.$overflow(arg);

            if overflowed {
                // If the operation overflowed we need to retry it but using
                // big integers.
                let result =
                    to_expr!(Integer::from(rec) $op Integer::from(arg));

                $process.allocate(object_value::bigint(result), $proto)
            } else if ObjectPointer::integer_too_large(result) {
                // An operation that doesn't overflow may still produce a number
                // too large to store in a tagged pointer. In this case we'll
                // allocate the result as a heap integer.
                $process.allocate(object_value::integer(result), $proto)
            } else {
                ObjectPointer::integer(result)
            }
        } else if rec_ptr.is_bigint() && arg_ptr.is_integer() {
            // Example: bigint + int -> bigint

            let rec = rec_ptr.bigint_value()?.clone();
            let arg = arg_ptr.integer_value()?;

            let bigint = if arg_ptr.is_in_i32_range() {
                to_expr!(rec $op arg as i32)
            } else {
                to_expr!(rec $op Integer::from(arg))
            };

            $process.allocate(object_value::bigint(bigint), $proto)
        } else if rec_ptr.is_integer() && arg_ptr.is_bigint() {
            // Example: int + bigint -> bigint

            let rec = Integer::from(rec_ptr.integer_value()?);
            let arg = arg_ptr.bigint_value()?;
            let bigint = to_expr!(rec $op arg);

            $process.allocate(object_value::bigint(bigint), $proto)
        } else if rec_ptr.is_bigint() && arg_ptr.is_bigint() {
            // Example: bigint + bigint -> bigint

            let rec = rec_ptr.bigint_value()?;
            let arg = arg_ptr.bigint_value()?;
            let bigint = to_expr!(rec.clone() $op arg);

            $process.allocate(object_value::bigint(bigint), $proto)
        } else {
            return Err(
                "Integer instructions can only be performed using integers"
                    .to_string()
            );
        };

        $context.set_register(register, result);
    });
}

/// Performs an integer binary boolean operation such as `X == Y`.
///
/// This macro takes the following arguments:
///
/// * `$state`: the VM state as an RcState.
/// * `$context`: the current ExecutionContext.
/// * `$ins`: the current instruction.
/// * `$op`: the binary operator to use (e.g. `==`).
macro_rules! integer_bool_op {
    ($state: expr, $context: expr, $ins: expr, $op: tt) => ({
        let register = $ins.arg(0);
        let rec_ptr = $context.get_register($ins.arg(1));
        let arg_ptr = $context.get_register($ins.arg(2));

        let result = if rec_ptr.is_integer() && arg_ptr.is_integer() {
            // Example: integer < integer

            let rec = rec_ptr.integer_value()?;
            let arg = arg_ptr.integer_value()?;

            to_expr!(rec $op arg)
        } else if rec_ptr.is_integer() && arg_ptr.is_bigint() {
            // Example: integer < bigint

            let rec = Integer::from(rec_ptr.integer_value()?);
            let arg = arg_ptr.bigint_value()?;

            to_expr!(&rec $op arg)
        } else if rec_ptr.is_bigint() && arg_ptr.is_integer() {
            // Example: bigint < integer

            let rec = rec_ptr.bigint_value()?;
            let arg = Integer::from(arg_ptr.integer_value()?);

            to_expr!(rec $op &arg)
        } else if rec_ptr.is_bigint() && arg_ptr.is_bigint() {
            // Example: bigint < bigint

            let rec = rec_ptr.bigint_value()?;
            let arg = arg_ptr.bigint_value()?;

            to_expr!(rec $op arg)
        } else {
            return Err(
                "Integer instructions can only be performed using integers"
                    .to_string()
            );
        };

        let boolean = if result {
            $state.true_object
        }
        else {
            $state.false_object
        };

        $context.set_register(register, boolean);
    });
}

macro_rules! float_op {
    ($state: expr, $process: expr, $ins: expr, $op: tt) => ({
        let register = $ins.arg(0);
        let rec_ptr = $process.get_register($ins.arg(1));
        let arg_ptr = $process.get_register($ins.arg(2));
        let rec = rec_ptr.float_value()?;
        let arg = arg_ptr.float_value()?;
        let result = to_expr!(rec $op arg);

        let obj = $process
            .allocate(object_value::float(result), $state.float_prototype);

        $process.set_register(register, obj);
    });
}

/// Performs a float binary boolean operation such as `X == Y`.
///
/// This macro takes the following arguments:
///
/// * `$state`: the VM state as an RcState.
/// * `$context`: the current ExecutionContext.
/// * `$ins`: the current instruction.
/// * `$op`: the binary operator to use (e.g. `==`).
macro_rules! float_bool_op {
    ($state: expr, $context: expr, $ins: expr, $op: tt) => ({
        let register = $ins.arg(0);
        let rec_ptr = $context.get_register($ins.arg(1));
        let arg_ptr = $context.get_register($ins.arg(2));
        let rec = rec_ptr.float_value()?;
        let arg = arg_ptr.float_value()?;

        let boolean = if to_expr!(rec $op arg) {
            $state.true_object
        }
        else {
            $state.false_object
        };

        $context.set_register(register, boolean);
    });
}
