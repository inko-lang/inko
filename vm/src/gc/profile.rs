use gc::trace_result::TraceResult;
use timer::Timer;

#[derive(Debug, Eq, PartialEq)]
pub enum CollectionType {
    /// A young generation collection.
    Young,

    /// A young + full collection.
    Full,

    /// A mailbox collection.
    Mailbox,

    /// A collection for a finished process.
    Finished,
}

pub struct Profile {
    /// The type of garbage collection that was performed.
    pub collection_type: CollectionType,

    /// The number of marked objects.
    pub marked: usize,

    /// The number of evacuated objects.
    pub evacuated: usize,

    /// The number of objects promoted to the full generation.
    pub promoted: usize,

    /// The total garbage collection time.
    pub total: Timer,

    /// The time spent preparing a collection.
    pub prepare: Timer,

    /// The time spent tracing through live objects.
    pub trace: Timer,

    /// The time spent reclaiming blocks.
    pub reclaim: Timer,

    /// The total time the process was suspended.
    pub suspended: Timer,
}

impl Profile {
    pub fn new(collection_type: CollectionType) -> Self {
        Profile {
            collection_type: collection_type,
            marked: 0,
            evacuated: 0,
            promoted: 0,
            total: Timer::now(),
            prepare: Timer::new(),
            trace: Timer::new(),
            reclaim: Timer::new(),
            suspended: Timer::now(),
        }
    }

    pub fn young() -> Self {
        Self::new(CollectionType::Young)
    }

    pub fn full() -> Self {
        Self::new(CollectionType::Full)
    }

    pub fn mailbox() -> Self {
        Self::new(CollectionType::Mailbox)
    }

    pub fn finished() -> Self {
        Self::new(CollectionType::Finished)
    }

    pub fn populate_tracing_statistics(&mut self, result: TraceResult) {
        self.marked = result.marked;
        self.evacuated = result.evacuated;
        self.promoted = result.promoted;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gc::trace_result::TraceResult;

    #[test]
    fn test_new() {
        let profile = Profile::new(CollectionType::Young);

        assert_eq!(profile.collection_type, CollectionType::Young);
        assert_eq!(profile.marked, 0);
        assert_eq!(profile.evacuated, 0);
        assert_eq!(profile.promoted, 0);
    }

    #[test]
    fn test_young() {
        let profile = Profile::young();

        assert_eq!(profile.collection_type, CollectionType::Young);
    }

    #[test]
    fn test_full() {
        let profile = Profile::full();

        assert_eq!(profile.collection_type, CollectionType::Full);
    }

    #[test]
    fn test_mailbox() {
        let profile = Profile::mailbox();

        assert_eq!(profile.collection_type, CollectionType::Mailbox);
    }

    #[test]
    fn test_populate_tracing_statistics() {
        let mut profile = Profile::new(CollectionType::Young);

        profile.populate_tracing_statistics(TraceResult::with(1, 2, 3));

        assert_eq!(profile.marked, 1);
        assert_eq!(profile.evacuated, 2);
        assert_eq!(profile.promoted, 3);
    }
}
