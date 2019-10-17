//! Configuration for heap generations.
use crate::config::Config;

pub struct GenerationConfig {
    /// The maximum number of blocks that can be allocated before triggering a
    /// garbage collection.
    pub threshold: u32,

    /// The number of blocks that have been allocated since the last garbage
    /// collection.
    pub block_allocations: u32,
}

impl GenerationConfig {
    pub fn new(threshold: u32) -> Self {
        GenerationConfig {
            threshold,
            block_allocations: 0,
        }
    }

    /// Returns true if the allocation threshold should be increased.
    ///
    /// The `blocks` argument should specify the current number of live blocks.
    pub fn should_increase_threshold(
        &self,
        blocks: usize,
        growth_threshold: f64,
    ) -> bool {
        let percentage = blocks as f64 / f64::from(self.threshold);

        percentage >= growth_threshold
    }

    pub fn increment_threshold(&mut self, growth_factor: f64) {
        self.threshold =
            (f64::from(self.threshold) * growth_factor).ceil() as u32;
    }

    pub fn update_after_collection(
        &mut self,
        config: &Config,
        blocks: usize,
    ) -> bool {
        let max = config.heap_growth_threshold;
        let factor = config.heap_growth_factor;

        self.block_allocations = 0;

        if self.should_increase_threshold(blocks, max) {
            self.increment_threshold(factor);
            true
        } else {
            false
        }
    }

    pub fn allocation_threshold_exceeded(&self) -> bool {
        self.block_allocations >= self.threshold
    }

    pub fn increment_allocations(&mut self) {
        self.block_allocations += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_should_increase_threshold_with_too_many_blocks() {
        let config = GenerationConfig::new(1);

        assert_eq!(config.should_increase_threshold(0, 0.9), false);

        assert!(config.should_increase_threshold(10, 0.9));
    }

    #[test]
    fn test_should_increase_threshold_with_large_usage_percentage() {
        let config = GenerationConfig::new(10);

        assert_eq!(config.should_increase_threshold(1, 0.9), false);

        assert!(config.should_increase_threshold(9, 0.9));
    }

    #[test]
    fn test_increment_threshold() {
        let mut config = GenerationConfig::new(1);

        assert_eq!(config.threshold, 1);

        config.increment_threshold(2.0);

        assert_eq!(config.threshold, 2);
    }

    #[test]
    fn test_allocation_threshold_exceeded() {
        let mut config = GenerationConfig::new(1);

        assert_eq!(config.allocation_threshold_exceeded(), false);

        config.block_allocations = 5;

        assert!(config.allocation_threshold_exceeded());
    }

    #[test]
    fn test_increment_allocations() {
        let mut config = GenerationConfig::new(1);

        config.increment_allocations();

        assert_eq!(config.block_allocations, 1);
        assert!(config.allocation_threshold_exceeded());
    }

    #[test]
    fn test_update_after_collection() {
        let mut gen_config = GenerationConfig::new(1);
        let mut vm_config = Config::new();

        assert_eq!(gen_config.update_after_collection(&vm_config, 0), false);

        vm_config.heap_growth_factor = 2.0;
        gen_config.threshold = 4;
        gen_config.block_allocations = 4;

        assert!(gen_config.update_after_collection(&vm_config, 4));
        assert_eq!(gen_config.threshold, 8);
    }
}
