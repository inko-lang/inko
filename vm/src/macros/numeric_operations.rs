#![macro_use]

macro_rules! to_expr {
    ($e: expr) => ($e);
}

macro_rules! num_op {
    ($vm: expr, $process: expr, $ins: expr, $op: tt, $tname: ident,
     $as_name: ident, $ensure: ident, $proto: ident) => ({
        let register = try_vm_error!($ins.arg(0), $ins);
        let receiver_ptr = instruction_object!($ins, $process, 1);
        let arg_ptr = instruction_object!($ins, $process, 2);

        let receiver_ref = receiver_ptr.get();
        let receiver = receiver_ref.get();

        let arg_ref = arg_ptr.get();
        let arg = arg_ref.get();

        $ensure!($ins, receiver, arg);

        let result = to_expr!(receiver.value.$as_name() $op arg.value.$as_name());

        let obj = write_lock!($process)
            .allocate(object_value::$tname(result), $vm.state.$proto.clone());

        write_lock!($process).set_register(register, obj);
    });
}

macro_rules! num_bool_op {
    ($vm: expr, $process: expr, $ins: expr, $op: tt, $as_name: ident,
     $ensure: ident) => ({
        let register = try_vm_error!($ins.arg(0), $ins);
        let receiver_ptr = instruction_object!($ins, $process, 1);
        let arg_ptr = instruction_object!($ins, $process, 2);

        let receiver_ref = receiver_ptr.get();
        let receiver = receiver_ref.get();

        let arg_ref = arg_ptr.get();
        let arg = arg_ref.get();

        $ensure!($ins, receiver, arg);

        let result = to_expr!(receiver.value.$as_name() $op arg.value.$as_name());

        let boolean = if result {
            $vm.state.true_object.clone()
        }
        else {
            $vm.state.false_object.clone()
        };

        write_lock!($process).set_register(register, boolean);
    });
}

macro_rules! integer_op {
    ($vm: expr, $process: expr, $ins: expr, $op: tt) => ({
        num_op!($vm, $process, $ins, $op, integer, as_integer, ensure_integers,
                integer_prototype);
    });
}

macro_rules! integer_bool_op {
    ($vm: expr, $process: expr, $ins: expr, $op: tt) => ({
        num_bool_op!($vm, $process, $ins, $op, as_integer, ensure_integers);
    });
}

macro_rules! float_op {
    ($vm: expr, $process: expr, $ins: expr, $op: tt) => ({
        num_op!($vm, $process, $ins, $op, float, as_float, ensure_floats,
                float_prototype);
    });
}

macro_rules! float_bool_op {
    ($vm: expr, $process: expr, $ins: expr, $op: tt) => ({
        num_bool_op!($vm, $process, $ins, $op, as_float, ensure_floats);
    });
}
