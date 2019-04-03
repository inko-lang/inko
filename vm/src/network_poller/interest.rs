// The type of event a poller should wait for.
pub enum Interest {
    /// We're only interested in read operations.
    Read,

    /// We're only interested in write operations.
    Write,
}
