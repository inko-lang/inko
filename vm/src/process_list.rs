use process::RcProcess;

pub struct ProcessList {
    processes: Vec<Option<RcProcess>>,
    indexes: Vec<usize>,
}

impl ProcessList {
    pub fn new() -> ProcessList {
        ProcessList {
            processes: Vec::new(),
            indexes: Vec::new(),
        }
    }

    pub fn reserve_pid(&mut self) -> usize {
        if self.indexes.len() == 0 {
            self.processes.len()
        } else {
            self.indexes.pop().unwrap()
        }
    }

    pub fn add(&mut self, index: usize, process: RcProcess) {
        if index >= self.processes.len() {
            self.processes.insert(index, Some(process));
        } else {
            self.processes[index] = Some(process);
        }
    }

    pub fn remove(&mut self, process: RcProcess) {
        let index = process.pid;

        self.processes[index] = None;
        self.indexes.push(index);
    }

    pub fn get(&self, index: usize) -> Option<RcProcess> {
        let found = self.processes.get(index);

        if found.is_some() {
            found.unwrap().clone()
        } else {
            None
        }
    }
}
