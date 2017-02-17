use nix;
use nix::Error;
use nix::Errno;
use ::shm::Shm;
use ::pthread::PthreadPrimitiveConstructor;
use ::pthread::Condvar;

use ::pthread::PthreadWrappingPrimitiveConstructor;
use ::pthread::Mutex;

use std::time::Duration;


pub fn ipc_queue<T: Copy>() -> nix::Result<(Shm<Queue<T>>, Shm<Queue<T>>)> {
    let queue = Shm::new(Queue::pshared())?;
    Ok((queue.clone(), queue))
}

pub struct Queue<T> 
    where T: Copy
{
    buffer: Mutex<RingBuffer<T>>,
    in_cond: Condvar,
    out_cond: Condvar
}

impl<T> PthreadPrimitiveConstructor for Queue<T> 
    where T: Copy 
{
    fn new() -> Self {
        Queue {
            buffer: Mutex::new(RingBuffer::new()),
            in_cond: Condvar::new(),
            out_cond: Condvar::new(),
        }
    }

    fn pshared() -> Self {
        Queue {
            buffer: Mutex::pshared(RingBuffer::new()),
            in_cond: Condvar::pshared(),
            out_cond: Condvar::pshared(),
        }
    }
}

use std::fmt::Debug;

impl<T> Queue<T> 
    where T: Copy + Debug
{
    pub fn push(&self, value: T) -> Result<(), Error> {
        let mut guard = self.buffer.lock()?;
        while guard.write(value).is_err() {
            guard = self.out_cond.wait(guard)?;
        }

        self.in_cond.signal()
    }

    pub fn pop(&self) -> Result<T, Error> {
        let poll_result = {
            let poll_value = || {
                let mut guard = self.buffer.lock()?;
                loop {
                    if let Some(t) = guard.try_read() {
                        return Ok(t);
                    } else {
                        guard = self.in_cond.wait(guard)?;
                    }
                }
            };
            poll_value()
        };

        self.out_cond.signal()?;
        poll_result
    }  

    pub fn try_pop(&self) -> Result<Option<T>, Error> {
        Ok(self.buffer.lock()?.try_read())
    }
    
    pub fn timed_pop(&self, time: Duration) -> Result<Option<T>, Error> {
        let poll_result = {
            let poll_value = || {
                let mut guard = self.buffer.lock()?;
                match guard.try_read() {
                    Some(t) => Ok(Some(t)),
                    None => match self.in_cond.timed_wait(guard, time) {
                        Err(Error::Sys(Errno::ETIMEDOUT)) => Ok(None),
                        Err(err) => Err(err),
                        Ok(mut guard) => Ok(guard.try_read())
                    }
                }
            };
            poll_value()
        };

        self.out_cond.signal()?;
        poll_result
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
enum RingBufferError {
    Overflow
}

const RING_BUFFER_SIZE: usize = 8;

#[derive(Debug)]
struct RingBuffer<T> 
    where T: Copy 
{
    write_idx: RingBufferIdx,
    read_idx:  RingBufferIdx,
    buffer:    [Option<T>; RING_BUFFER_SIZE]
}

impl<T> RingBuffer<T> 
    where T: Copy
{
    pub fn new() -> RingBuffer<T> {
        RingBuffer {
            write_idx: RingBufferIdx::new(0, RING_BUFFER_SIZE),
            read_idx: RingBufferIdx::new(0, RING_BUFFER_SIZE),
            buffer: [None; RING_BUFFER_SIZE]
        }
    }

    pub fn try_read(&mut self) -> Option<T> {
        let current = self.buffer[self.read_idx.get()].take();
        if current.is_some() {
            self.read_idx.forward();
        }
        current
    }

    pub fn write(&mut self, value: T) -> Result<(), RingBufferError> {
        let current = &mut self.buffer[self.write_idx.get()];
        if current.is_none() {
            *current = Some(value);
            self.write_idx.forward();
            Ok(())
        } else {
            Err(RingBufferError::Overflow)
        }
    }
}

#[derive(Copy, Clone, Debug)]
struct RingBufferIdx {
    bufsize: usize,
    idx: usize   
}

impl RingBufferIdx {
    pub fn new(start_idx: usize, bufsize: usize) -> RingBufferIdx {
        RingBufferIdx {
            bufsize: bufsize,
            idx: start_idx
        }
    }

    pub fn forward(&mut self) -> &mut Self {
        self.idx += 1;
        self.idx %= self.bufsize;
        self
    } 

    pub fn get(&self) -> usize {
        self.idx
    }
}

#[cfg(test)]
mod tests {
    mod ring_buffer_idx {
        use super::super::RingBufferIdx;

        #[test]
        fn forward() {
            let test_wrapping = |idx, size, next_idx| {
                let mut rb_idx = RingBufferIdx::new(idx, size);
                assert_eq!(next_idx, rb_idx.forward().get());
            };

            test_wrapping(0, 10, 1);
            test_wrapping(0, 1,  0);
            test_wrapping(9, 10, 0);
        }
    }

    mod ring_buffer {
        use super::super::RING_BUFFER_SIZE;
        use super::super::RingBuffer;
        use super::super::RingBufferError;

        #[test]
        fn rw() {
            let mut rb = RingBuffer::new();
            assert_eq!(None,    rb.try_read());
            assert_eq!(Ok(()),  rb.write(1));
            assert_eq!(Some(1), rb.try_read());
            assert_eq!(None,    rb.try_read());
            assert_eq!(Ok(()),  rb.write(2));
            assert_eq!(Ok(()),  rb.write(3));
            assert_eq!(Ok(()),  rb.write(4));
            assert_eq!(Some(2), rb.try_read());
            assert_eq!(Some(3), rb.try_read());
            assert_eq!(Some(4), rb.try_read());
            assert_eq!(None,    rb.try_read());
        }

        #[test]
        fn overflow() {
            let mut rb = RingBuffer::new();
            for i in 0..RING_BUFFER_SIZE {
                rb.write(i).unwrap();
            }
            assert_eq!(Err(RingBufferError::Overflow), rb.write(RING_BUFFER_SIZE));
        }

        #[test]
        fn overflow_escape() {
            let mut rb = RingBuffer::new();
            for i in 0..RING_BUFFER_SIZE {
                rb.write(i).unwrap();
            }
            assert_eq!(Err(RingBufferError::Overflow), rb.write(RING_BUFFER_SIZE));

            assert_eq!(Some(0), rb.try_read());
            assert_eq!(Ok(()), rb.write(RING_BUFFER_SIZE));
            assert_eq!(Some(RING_BUFFER_SIZE), rb.buffer[0]);
        }
    }

    mod queue {
        use ::pthread::PthreadPrimitiveConstructor;
        use super::super::Queue;
        use std::thread;
        use ::process;
        use ::shm::Shm;

        #[test]
        fn mpsc() {
            let queue = Shm::new(Queue::new())
                .unwrap();
                    
            let producer = {
                let queue = queue.clone();
                thread::spawn(move || {
                    for i in 0..10000 {
                        queue.push(i).unwrap();
                    }
                })
            };

            // consumer
            for i in 0..10000 {
                let v = queue.pop();
                assert_eq!(Ok(i), v);
            }

            producer.join().unwrap();
        }

        #[test]
        fn mpsc_ipc() {
            let queue = Shm::new(Queue::pshared())
                .unwrap();
            
            {
                let queue = queue.clone();
                process::spawn(move || {
                    for i in 0..10000 {
                        queue.push(i).unwrap();
                    }
                }).unwrap();
            }

            // consumer
            for i in 0..10000 {
                let v = queue.pop();
                assert_eq!(Ok(i), v);
            }
        }

        #[test]
        fn try_pop() {
            let queue = Queue::new();
            assert_eq!(Ok(None), queue.try_pop());
            queue.push(1).unwrap();
            assert_eq!(Ok(Some(1)), queue.try_pop());
        }
    }

    #[test]
    fn ipc_queue() {
        let (tx, rx) = super::ipc_queue().unwrap();

        ::process::spawn(move || {
            for i in 0..1000 {
                tx.push(i).unwrap();
            }
        }).unwrap();

        for i in 0..1000 {
            let v = rx.pop();
            assert_eq!(Ok(i), v);
        }
    }
}