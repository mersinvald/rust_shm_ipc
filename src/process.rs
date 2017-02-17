use nix::unistd::{fork, ForkResult};
use nix::sys::wait::waitpid;
use nix::sys::signal::kill;
use nix::Result;
use std::process::exit;
use nix::libc;

pub use nix::sys::signal::Signal;
pub use nix::sys::wait::*;

pub struct Process {
    pid: i32
}

impl Process {
    pub fn new(pid: i32) -> Process {
        Process { pid }
    }

    pub fn pid(&self) -> i32 {
        self.pid
    }

    pub fn wait(&self, flags: Option<WaitPidFlag>) -> Result<WaitStatus> {
        waitpid(self.pid, flags)
    }

    pub fn sendsig(&self, signal: Signal) -> Result<()> {
        kill(self.pid, signal)
    }
}

pub fn spawn<T: FnOnce()>(child_main: T) -> Result<Process> {
    match fork()? {
        ForkResult::Parent{ child: pid } => Ok(Process::new(pid)),
        ForkResult::Child => {
            child_main();
            exit(0);
        }
    }
}

pub fn pid() -> i32 {
    unsafe {
        libc::getpid()
    }
}

#[cfg(test)]
mod test {
    use std::thread;
    use std::time::Duration;
    use nix::sys::wait::{WaitStatus};
    use nix::sys::signal::{SIGTERM};
    #[test]
    fn spawn() {
        let child = super::spawn(||()).unwrap();
        assert!(child.pid() != 0);
    }

    #[test]
    fn wait_exit() {
        let child = super::spawn(|| thread::sleep(Duration::from_millis(10)))
            .unwrap();
        match child.wait(None).unwrap() {
            WaitStatus::Exited(..) => (),
            other => panic!("Invalid wait result. \
                             expected: WaitStatus::Exited(..), got: {:?}", other),
        }
    }

    #[test]
    fn sendsig() {
        let child = super::spawn(|| thread::sleep(Duration::from_secs(1)))
            .unwrap();
        
        child.sendsig(SIGTERM).unwrap();

        match child.wait(None).unwrap() {
            WaitStatus::Signaled(_, sig, _) => assert_eq!(sig, SIGTERM),
            other => panic!("Invalid wait result. \
                             expected: WaitStatus::Signaled(_, SIGRETM, _), got: {:?}", other),
        }
    }
}