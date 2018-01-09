//! Collections of multiple process pools.
use pool::Pool;
use process::RcProcess;

/// The number of process pools to create.
const POOL_AMOUNT: usize = 2;

/// The index of the primary process pool.
pub const PRIMARY_POOL: usize = 0;

/// The index of the secondary process pool.
pub const SECONDARY_POOL: usize = 1;

pub struct Pools {
    pools: [Pool<RcProcess>; POOL_AMOUNT],
}

impl Pools {
    pub fn new(primary: usize, secondary: usize) -> Self {
        Pools {
            pools: [
                Pool::new(primary, Some("primary".to_string())),
                Pool::new(secondary, Some("secondary".to_string())),
            ],
        }
    }

    pub fn get(&self, index: usize) -> Option<&Pool<RcProcess>> {
        self.pools.get(index)
    }

    pub fn schedule(&self, process: RcProcess) {
        if let Some(pool) = self.get(process.pool_id) {
            process.scheduled();
            pool.schedule(process);
        } else {
            panic!(
                "The pool ID ({}) for process {} is invalid",
                process.pool_id, process.pid
            );
        }
    }

    pub fn terminate(&self) {
        for pool in self.pools.iter() {
            pool.terminate();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vm::test::setup;

    #[test]
    fn test_new() {
        let pools = Pools::new(1, 1);

        assert_eq!(pools.pools[0].inner.queues.len(), 1);
        assert_eq!(pools.pools[1].inner.queues.len(), 1);
    }

    #[test]
    fn test_get_invalid() {
        let pools = Pools::new(1, 1);

        assert!(pools.get(2).is_none());
    }

    #[test]
    fn test_get_valid() {
        let pools = Pools::new(1, 1);

        assert!(pools.get(0).is_some());
    }

    #[test]
    fn test_schedule() {
        let pools = Pools::new(1, 1);
        let (_machine, _block, process) = setup();

        process.running();
        pools.schedule(process.clone());

        assert_eq!(pools.pools[0].inner.queues.len(), 1);
        assert_eq!(process.available_for_execution(), true);
    }

    #[test]
    fn test_terminate() {
        let pools = Pools::new(1, 1);

        pools.terminate();

        assert_eq!(pools.pools[0].inner.should_process(), false);
        assert_eq!(pools.pools[1].inner.should_process(), false);
    }
}
