#![macro_use]

macro_rules! io_error_code {
    ($process: expr, $io_error: expr) => ({
        let code = $crate::error_codes::from_io_error($io_error);

        $process.allocate_without_prototype($crate::object_value::error(code))
    });
}

macro_rules! invalid_utf8_error_code {
    ($process: expr) => ({
        let code = $crate::error_codes::STRING_INVALID_UTF8;

        $process.allocate_without_prototype($crate::object_value::error(code))
    });
}

macro_rules! attribute_error {
    ($reg: expr, $name: expr) => (
        panic!(
            "The object in register {} does not define the attribute \"{}\"",
            $reg,
            $name
        );
    )
}
