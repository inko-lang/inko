//! Configuration for heap generations.
use immix::block::BLOCK_SIZE;

pub struct GenerationConfig {
    /// The maximum number of blocks that can be allocated before triggering a
    /// garbage collection.
    pub threshold: usize,

    /// The number of blocks that have been allocated.
    pub block_allocations: usize,

    /// The percentage of blocks that should be used (relative to the threshold)
    /// before incrementing the threshold.
    pub growth_threshold: f64,

    /// The factor to grow the allocation threshold by.
    pub growth_factor: f64,

    /// Boolean indicating if this generation should be collected.
    pub collect: bool,
}

impl GenerationConfig {
    pub fn new(bytes: usize, percentage: f64, growth_factor: f64) -> Self {
        GenerationConfig {
            threshold: bytes / BLOCK_SIZE,
            block_allocations: 0,
            collect: false,
            growth_threshold: percentage,
            growth_factor,
        }
    }

    pub fn should_increment(&self) -> bool {
        let percentage = self.block_allocations as f64 / self.threshold as f64;

        self.allocation_threshold_exceeded()
            || percentage >= self.growth_threshold
    }

    pub fn increment_threshold(&mut self) {
        self.threshold =
            (self.threshold as f64 * self.growth_factor).ceil() as usize;
    }

    pub fn allocation_threshold_exceeded(&self) -> bool {
        self.block_allocations >= self.threshold
    }

    pub fn increment_allocations(&mut self) {
        self.block_allocations += 1;

        if self.allocation_threshold_exceeded() && !self.collect {
            self.collect = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_should_increment_with_too_many_blocks() {
        let mut config = GenerationConfig::new(BLOCK_SIZE, 0.9, 2.0);

        assert_eq!(config.should_increment(), false);

        config.block_allocations = 10;

        assert!(config.should_increment());
    }

    #[test]
    fn test_should_increment_with_large_usage_percentage() {
        let mut config = GenerationConfig::new(BLOCK_SIZE * 10, 0.9, 2.0);

        assert_eq!(config.should_increment(), false);

        config.block_allocations = 9;

        assert!(config.should_increment());
    }

    #[test]
    fn test_increment_threshold() {
        let mut config = GenerationConfig::new(BLOCK_SIZE, 0.9, 2.0);

        assert_eq!(config.threshold, 1);

        config.increment_threshold();

        assert_eq!(config.threshold, 2);
    }

    #[test]
    fn test_allocation_threshold_exceeded() {
        let mut config = GenerationConfig::new(BLOCK_SIZE, 0.9, 2.0);

        assert_eq!(config.allocation_threshold_exceeded(), false);

        config.block_allocations = 5;

        assert!(config.allocation_threshold_exceeded());
    }

    #[test]
    fn test_increment_allocations() {
        let mut config = GenerationConfig::new(BLOCK_SIZE, 0.9, 2.0);

        config.increment_allocations();

        assert_eq!(config.block_allocations, 1);
        assert!(config.collect);
    }
}
