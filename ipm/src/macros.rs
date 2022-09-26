macro_rules! fail {
    ($message:expr $(,$arg:expr)*) => {
        return Err(Error::new(format!($message $(,$arg)*)))
    };
}

macro_rules! error {
    ($message:expr $(,$arg:expr)*) => {
        Error::new(format!($message $(,$arg)*))
    };
}
