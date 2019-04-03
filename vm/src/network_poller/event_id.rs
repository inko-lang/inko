/// The unique ID of an event obtained by a poller.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct EventId(pub u64);

impl EventId {
    pub fn value(self) -> u64 {
        self.0
    }
}
