#![macro_use]

macro_rules! to_expr {
    ($e: expr) => ($e);
}

macro_rules! num_op {
    ($vm: expr, $process: expr, $ins: expr, $op: tt, $tname: ident,
     $as_name: ident, $ensure: ident, $proto: ident) => ({
        let register = $ins.arg(0)?;
        let receiver_ptr = $process.get_register($ins.arg(1)?)?;
        let arg_ptr = $process.get_register($ins.arg(2)?)?;

        let receiver = receiver_ptr.get();
        let arg = arg_ptr.get();

        $ensure!($ins, receiver, arg);

        let result = to_expr!(receiver.value.$as_name() $op arg.value.$as_name());

        let obj = $process
            .allocate(object_value::$tname(result), $vm.state.$proto.clone());

        $process.set_register(register, obj);

        Ok(Action::None)
    });
}

macro_rules! num_bool_op {
    ($vm: expr, $process: expr, $ins: expr, $op: tt, $as_name: ident,
     $ensure: ident) => ({
        let register = $ins.arg(0)?;
        let receiver_ptr = $process.get_register($ins.arg(1)?)?;
        let arg_ptr = $process.get_register($ins.arg(2)?)?;

        let receiver = receiver_ptr.get();
        let arg = arg_ptr.get();

        $ensure!($ins, receiver, arg);

        let result = to_expr!(receiver.value.$as_name() $op arg.value.$as_name());

        let boolean = if result {
            $vm.state.true_object.clone()
        }
        else {
            $vm.state.false_object.clone()
        };

        $process.set_register(register, boolean);

        Ok(Action::None)
    });
}

macro_rules! integer_op {
    ($vm: expr, $process: expr, $ins: expr, $op: tt) => ({
        num_op!($vm, $process, $ins, $op, integer, as_integer, ensure_integers,
                integer_prototype)
    });
}

macro_rules! integer_bool_op {
    ($vm: expr, $process: expr, $ins: expr, $op: tt) => ({
        num_bool_op!($vm, $process, $ins, $op, as_integer, ensure_integers)
    });
}

macro_rules! float_op {
    ($vm: expr, $process: expr, $ins: expr, $op: tt) => ({
        num_op!($vm, $process, $ins, $op, float, as_float, ensure_floats,
                float_prototype)
    });
}

macro_rules! float_bool_op {
    ($vm: expr, $process: expr, $ins: expr, $op: tt) => ({
        num_bool_op!($vm, $process, $ins, $op, as_float, ensure_floats)
    });
}
