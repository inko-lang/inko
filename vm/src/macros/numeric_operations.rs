#![macro_use]

macro_rules! to_expr {
    ($e: expr) => ($e);
}

macro_rules! num_bool_op {
    ($vm: expr, $process: expr, $ins: expr, $op: tt, $as_name: ident) => ({
        let register = $ins.arg(0);
        let receiver_ptr = $process.get_register($ins.arg(1));
        let arg_ptr = $process.get_register($ins.arg(2));
        let result = to_expr!(receiver_ptr.$as_name()? $op arg_ptr.$as_name()?);

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
    ($process: expr, $ins: expr, $op: tt) => ({
        let register = $ins.arg(0);
        let receiver_ptr = $process.get_register($ins.arg(1));
        let arg_ptr = $process.get_register($ins.arg(2));
        let result = to_expr!(receiver_ptr.integer_value()? $op arg_ptr.integer_value()?);

        $process.set_register(register, ObjectPointer::integer(result));

        Ok(Action::None)
    });
}

macro_rules! integer_bool_op {
    ($vm: expr, $process: expr, $ins: expr, $op: tt) => ({
        num_bool_op!($vm, $process, $ins, $op, integer_value)
    });
}

macro_rules! float_op {
    ($vm: expr, $process: expr, $ins: expr, $op: tt) => ({
        let register = $ins.arg(0);
        let receiver_ptr = $process.get_register($ins.arg(1));
        let arg_ptr = $process.get_register($ins.arg(2));
        let result = to_expr!(receiver_ptr.float_value()? $op arg_ptr.float_value()?);

        let obj = $process
            .allocate(object_value::float(result), $vm.state.float_prototype);

        $process.set_register(register, obj);

        Ok(Action::None)
    });
}

macro_rules! float_bool_op {
    ($vm: expr, $process: expr, $ins: expr, $op: tt) => ({
        num_bool_op!($vm, $process, $ins, $op, float_value)
    });
}
