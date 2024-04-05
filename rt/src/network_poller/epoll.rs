use crate::network_poller::Interest;
use crate::process::ProcessPointer;
use rustix::event::epoll::{
    add, create, delete, modify, wait, CreateFlags, EventData, EventFlags,
    EventVec,
};
use rustix::fd::{AsFd, OwnedFd};
use rustix::io::Errno;

fn flags_for(interest: Interest) -> EventFlags {
    let flags = match interest {
        Interest::Read => EventFlags::IN,
        Interest::Write => EventFlags::OUT,
    };

    flags | EventFlags::ET | EventFlags::ONESHOT
}

pub(crate) type Events = EventVec;

pub(crate) struct Poller {
    fd: OwnedFd,
}

impl Poller {
    pub(crate) fn new() -> Poller {
        let fd = create(CreateFlags::CLOEXEC).expect("epoll_create() failed");

        Poller { fd }
    }

    pub(crate) fn poll(&self, events: &mut Events) -> Vec<ProcessPointer> {
        match wait(&self.fd, events, -1) {
            Ok(_) | Err(Errno::INTR) => {}
            Err(_) => panic!("epoll_wait() failed"),
        }

        let procs = events
            .iter()
            .map(|e| unsafe { ProcessPointer::new(e.data.u64() as *mut _) })
            .collect();

        events.clear();
        procs
    }

    pub(crate) fn add(
        &self,
        process: ProcessPointer,
        source: impl AsFd,
        interest: Interest,
    ) {
        let data = EventData::new_u64(process.identifier() as _);

        add(&self.fd, source, data, flags_for(interest))
            .expect("epoll_ctl() failed");
    }

    pub(crate) fn modify(
        &self,
        process: ProcessPointer,
        source: impl AsFd,
        interest: Interest,
    ) {
        let data = EventData::new_u64(process.identifier() as _);

        modify(&self.fd, source, data, flags_for(interest))
            .expect("epoll_ctl() failed");
    }

    pub(crate) fn delete(&self, source: impl AsFd) {
        delete(&self.fd, source).expect("epoll_ctl() failed");
    }
}
