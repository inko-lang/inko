//! Collections of multiple process pools.
use pool::{Job, Pool};
use process::RcProcess;

/// The number of process pools to create.
const POOL_AMOUNT: usize = 2;

/// The index of the primary process pool.
pub const PRIMARY_POOL: u8 = 0;

/// The index of the secondary process pool.
pub const SECONDARY_POOL: u8 = 1;

pub struct Pools {
    pools: [Pool<RcProcess>; POOL_AMOUNT],
}

impl Pools {
    pub fn new(primary: u8, secondary: u8) -> Self {
        Pools {
            pools: [
                Pool::new(primary, Some("primary".to_string())),
                Pool::new(secondary, Some("secondary".to_string())),
            ],
        }
    }

    pub fn get(&self, index: u8) -> Option<&Pool<RcProcess>> {
        self.pools.get(index as usize)
    }

    pub fn schedule(&self, process: RcProcess) {
        let pool_id = process.pool_id();

        if let Some(pool) = self.get(pool_id) {
            let job = if let Some(thread_id) = process.thread_id() {
                Job::pinned(process, thread_id)
            } else {
                Job::normal(process)
            };

            pool.schedule(job);
        } else {
            panic!(
                "The pool ID ({}) for process {} is invalid",
                pool_id, process.pid
            );
        }
    }

    pub fn terminate(&self) {
        for pool in &self.pools {
            pool.terminate();
        }
    }

    pub fn pool_id_is_valid(&self, id: u8) -> bool {
        (id as usize) < self.pools.len()
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

        pools.schedule(process.clone());

        assert_eq!(pools.pools[0].inner.queues.len(), 1);
    }

    #[test]
    fn test_schedule_pinned() {
        let pools = Pools::new(2, 1);
        let (_machine, _block, process) = setup();

        process.set_thread_id(1);
        pools.schedule(process);

        assert_eq!(pools.pools[0].inner.global_queue.len(), 0);
        assert_eq!(pools.pools[0].inner.queues[1].len(), 1);
    }

    #[test]
    fn test_terminate() {
        let pools = Pools::new(1, 1);

        pools.terminate();

        assert!(pools.pools[0].inner.queues[0].should_terminate());
        assert!(pools.pools[1].inner.queues[0].should_terminate());
    }

    #[test]
    fn test_pool_id_is_valid() {
        let pools = Pools::new(1, 1);

        assert_eq!(pools.pool_id_is_valid(0), true);
        assert_eq!(pools.pool_id_is_valid(1), true);
        assert_eq!(pools.pool_id_is_valid(2), false);
    }
}
