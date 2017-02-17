use rand::Rng;
use rand::thread_rng;

use nix::Result;
use nix::sys::mman;
use nix::c_void;
use nix::fcntl;
use nix::unistd::{ftruncate, close};
use nix::sys::stat;
use std::mem;
use std::ptr;
use std::sync::atomic::{AtomicUsize, Ordering};

type RawFd = i32;

#[derive(Debug)]
pub struct Shm<T> {
    inner_ptr: *mut ShmInner<T>,
}

unsafe impl<T> Send for Shm<T> {}

impl<T> Shm<T> {
    pub fn new(obj: T) -> Result<Self> {
        let shm_path = thread_rng()
            .gen_ascii_chars()
            .take(10)
            .collect::<String>();

        let shm_fd = Self::open_shm(&shm_path)?;
        let raw_ptr = Self::mmap_shm(shm_fd)?;

        close(shm_fd)?;

        unsafe {
            ptr::write(raw_ptr, ShmInner::new(obj));
            Ok(Shm {
                inner_ptr: raw_ptr,
            })
        }   
    }

    #[allow(dead_code)]
    pub unsafe fn get_raw(&mut self) -> *mut T {
        (*self.inner_ptr).get_raw_data()
    }

    fn open_shm(shm_name: &str) -> Result<RawFd> {
        let fd = mman::shm_open(shm_name, 
                                fcntl::O_RDWR | fcntl::O_CREAT | fcntl::O_EXCL,
                                stat::Mode::empty())?;                             
        ftruncate(fd, mem::size_of::<ShmInner<T>>() as i64)?;
        Ok(fd)
    }

    fn mmap_shm(fd: i32) -> Result<*mut ShmInner<T>> {
        let void_ptr = mman::mmap(0 as *mut c_void,
                                  mem::size_of::<ShmInner<T>>(),
                                  mman::PROT_READ | mman::PROT_WRITE,
                                  mman::MAP_SHARED,
                                  fd,
                                  0)?;
        Ok(void_ptr as *mut ShmInner<T>)
    }


}

use std::ops::{Deref, DerefMut};

impl<T> Deref for Shm<T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        unsafe {
            &*self.inner_ptr
        }
    }
}

impl<T> DerefMut for Shm<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe {
            &mut *self.inner_ptr
        }
    }
}

impl<T> Clone for Shm<T> {
    fn clone(&self) -> Self {
        unsafe {
            (*self.inner_ptr).increment_ref_ctr()
        };

        Shm {
            inner_ptr: self.inner_ptr
        }
    }
}

impl<T> Drop for Shm<T> {
    fn drop(&mut self) {
        let mut inner = unsafe {
            &mut *self.inner_ptr
        };

        inner.decrement_ref_ctr();

        if inner.ref_count() == 0  {
            // Reading inner data to cause Drop
            unsafe {
                ptr::read(self.inner_ptr);
            }

            // Unmapping 
            mman::munmap(self.inner_ptr as *mut c_void, mem::size_of::<ShmInner<T>>())
                .unwrap();
        }
    }
}

struct ShmInner<T> {
    ref_ctr: AtomicUsize,
    data: T
}

impl<T> ShmInner<T> {
    pub fn new(data: T) -> Self {
        ShmInner {
            ref_ctr: AtomicUsize::new(1),
            data: data
        }
    }

    pub fn increment_ref_ctr(&mut self) {
        self.ref_ctr.fetch_add(1, Ordering::SeqCst);
    }

    pub fn decrement_ref_ctr(&mut self) {
        self.ref_ctr.fetch_sub(1, Ordering::SeqCst);
    }

    pub fn ref_count(&self) -> usize {
        self.ref_ctr.load(Ordering::SeqCst)
    }

    pub fn get_raw_data(&mut self) -> *mut T {
        &mut self.data as *mut T
    }
}

impl<T> Deref for ShmInner<T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        &self.data
    }
}

impl<T> DerefMut for ShmInner<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.data
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ::process;

    #[test]
    fn simple() {
        let one = Shm::new(1).unwrap();
        assert_eq!(1, *one);
    }

    #[test]
    fn ipc() {
        let buffer = Shm::new([0; 10]).unwrap();
        {
            let mut buffer = buffer.clone();
            let child = process::spawn(|| {
                for i in 0..10 {
                    (*buffer)[i] = i;
                }
            }).unwrap();

            child.wait(None).unwrap();
        }

        for i in 0..10 {
            assert_eq!(i, (*buffer)[i]);
        }
    }
}
