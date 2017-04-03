#![macro_use]

macro_rules! to_expr {
    ($e: expr) => ($e);
}

macro_rules! num_bool_op {
    ($vm: expr, $process: expr, $ins: expr, $op: tt, $as_name: ident) => ({
        let register = $ins.arg(0);
        let receiver_ptr = $process.get_register($ins.arg(1));
        let arg_ptr = $process.get_register($ins.arg(2));
        let result = to_expr!(receiver_ptr.$as_name().unwrap() $op arg_ptr.$as_name().unwrap());

        let boolean = if result {
            $vm.state.true_object.clone()
        }
        else {
            $vm.state.false_object.clone()
        };

        $process.set_register(register, boolean);
    });
}

macro_rules! integer_op {
    ($process: expr, $ins: expr, $op: tt) => ({
        let register = $ins.arg(0);
        let receiver_ptr = $process.get_register($ins.arg(1));
        let arg_ptr = $process.get_register($ins.arg(2));
        let result = to_expr!(receiver_ptr.integer_value().unwrap() $op arg_ptr.integer_value().unwrap());

        $process.set_register(register, ObjectPointer::integer(result));
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
        let result = to_expr!(receiver_ptr.float_value().unwrap() $op arg_ptr.float_value().unwrap());

        let obj = $process
            .allocate(object_value::float(result), $vm.state.float_prototype);

        $process.set_register(register, obj);
    });
}

macro_rules! float_bool_op {
    ($vm: expr, $process: expr, $ins: expr, $op: tt) => ({
        num_bool_op!($vm, $process, $ins, $op, float_value)
    });
}
