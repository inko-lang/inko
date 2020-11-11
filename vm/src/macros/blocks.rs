#![macro_use]

macro_rules! prepare_block_arguments {
    ($old_context: expr, $new_context: expr, $start_reg: expr, $amount: expr) => {{
        if $amount > 0 {
            for (index, register) in
                ($start_reg..($start_reg + $amount)).enumerate()
            {
                $new_context.set_local(
                    index as u16,
                    $old_context.get_register(register),
                );
            }
        }
    }};
}
