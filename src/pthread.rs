use nix::Result;
use nix::Error; 
use nix::Errno;
use std::cell::UnsafeCell;
use std::mem;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use std::fmt;

use nix::libc::{
    PTHREAD_COND_INITIALIZER,
    pthread_cond_t,
    pthread_cond_init,
    pthread_cond_wait,
    pthread_cond_timedwait,
    pthread_cond_signal,
    pthread_condattr_init,
    pthread_condattr_setpshared,

    PTHREAD_MUTEX_INITIALIZER,
    pthread_mutex_t,
    pthread_mutex_init,
    pthread_mutex_lock,
    pthread_mutex_unlock,
    pthread_mutexattr_init,
    pthread_mutexattr_setpshared,

    PTHREAD_RWLOCK_INITIALIZER,
    pthread_rwlock_t,
    pthread_rwlock_rdlock,
    pthread_rwlock_wrlock,
    pthread_rwlock_unlock,

    timespec,
    time_t,
    c_long
};

pub trait PthreadPrimitiveConstructor {
    fn new() -> Self;
    fn pshared() -> Self;
}

pub trait PthreadWrappingPrimitiveConstructor<T> {
    fn new(data: T) -> Self;
    fn pshared(data: T) -> Self;
}

impl PthreadPrimitiveConstructor for pthread_cond_t {
    fn new() -> Self {
        PTHREAD_COND_INITIALIZER
    }

    fn pshared() -> Self {
        unsafe {
            let condattr = UnsafeCell::new(mem::uninitialized());
            pthread_condattr_init(condattr.get());
            pthread_condattr_setpshared(condattr.get(), 1);
            let cond = UnsafeCell::new(mem::uninitialized());
            pthread_cond_init(cond.get(), condattr.get());
            cond.into_inner()
        }
    }
}

impl PthreadPrimitiveConstructor for pthread_mutex_t {
    fn new() -> Self {
        PTHREAD_MUTEX_INITIALIZER
    }

    fn pshared() -> Self {
        unsafe {
            let mutexattr = UnsafeCell::new(mem::uninitialized());
            pthread_mutexattr_init(mutexattr.get());
            pthread_mutexattr_setpshared(mutexattr.get(), 1);
            let mutex = UnsafeCell::new(mem::uninitialized());
            pthread_mutex_init(mutex.get(), mutexattr.get());
            mutex.into_inner()
        }
    }
}

impl PthreadPrimitiveConstructor for pthread_rwlock_t {
    fn new() -> Self {
        PTHREAD_RWLOCK_INITIALIZER
    }

    // Need posix_rwlockattr_t and related functions support in libc crate
    fn pshared() -> Self {
        unimplemented!()
    }
}

pub struct Condvar(UnsafeCell<pthread_cond_t>);

unsafe impl Sync for Condvar {}

impl PthreadPrimitiveConstructor for Condvar {
    fn new() -> Condvar {
        Condvar(UnsafeCell::new(pthread_cond_t::new()))
    }

    fn pshared() -> Condvar {
        Condvar(UnsafeCell::new(pthread_cond_t::pshared()))
    }
}

impl Condvar {
    pub fn wait<'a, T>(&self, guard: MutexGuard<'a, T>) -> Result<MutexGuard<'a, T>> {
        let status = unsafe {
            pthread_cond_wait(
                self.0.get(),
                guard.0.lock.get()
            )
        };

        if status != 0 {
            Err(Error::Sys(Errno::from_i32(status)))
        } else {
            Ok(guard)
        }
    }

    pub fn timed_wait<'a, T>(&self, guard: MutexGuard<'a, T>, duration: Duration) -> Result<MutexGuard<'a, T>> {
        let abstime = SystemTime::now() + duration; 
        let duration = abstime.duration_since(UNIX_EPOCH)
            .unwrap();

        let tv = timespec {
            tv_sec:  duration.as_secs() as time_t, 
            tv_nsec: duration.subsec_nanos() as c_long
        };

        let status = unsafe {
            pthread_cond_timedwait(
                self.0.get(),
                guard.0.lock.get(),
                &tv as *const _
            )
        };

        if status != 0 {
            Err(Error::Sys(Errno::from_i32(status)))
        } else {
            Ok(guard)
        }
    }

    pub fn signal(&self) -> Result<()> {
        let status = unsafe {
            pthread_cond_signal(
                self.0.get()
            )
        };

        if status != 0 {
            Err(Error::Sys(Errno::from_i32(status)))
        } else {
            Ok(())
        }
    }
}

use std::ops::{Deref, DerefMut, Drop};

pub struct Mutex<T> {
    lock: UnsafeCell<pthread_mutex_t>,
    data: UnsafeCell<T>
}

unsafe impl<T> Sync for Mutex<T> {}

impl<T> PthreadWrappingPrimitiveConstructor<T> for Mutex<T> {
    fn new(data: T) -> Self {
        Mutex {
            lock: UnsafeCell::new(pthread_mutex_t::new()),
            data: UnsafeCell::new(data)
        }
    }

    fn pshared(data: T) -> Self {
        Mutex {
            lock: UnsafeCell::new(pthread_mutex_t::pshared()),
            data: UnsafeCell::new(data)
        }
    }
}

impl<T> Mutex<T> { 
    pub fn lock(&self) -> Result<MutexGuard<T>> {
        MutexGuard::new(self)
    }
}

impl<T> fmt::Debug for Mutex<T> 
    where T: fmt::Debug
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Mutex {{ data: {:?} }}", self.data)
    }
}

pub struct MutexGuard<'a, T: 'a>(&'a Mutex<T>);

impl<'a, T> MutexGuard<'a, T> {
    pub fn new(mutex: &'a Mutex<T>) -> Result<Self> {
        let status = unsafe {
            pthread_mutex_lock(
                mutex.lock.get()
            )
        };

        if status != 0 {
            Err(Error::Sys(Errno::from_i32(status)))
        } else {
            Ok(MutexGuard(mutex))
        }
    }
}

impl<'a, T> fmt::Debug for MutexGuard<'a, T> 
    where T: fmt::Debug
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "MutexGuard {{ mutex: {:?} }}", self.0)
    }
}


impl<'a, T> Deref for MutexGuard<'a, T> {
    type Target = T;
    fn deref(&self) -> &T {
        unsafe {
            &*self.0.data.get()
        }
    }
}

impl<'a, T> DerefMut for MutexGuard<'a, T> {
    fn deref_mut(&mut self) -> &mut T {
        unsafe {
            &mut *self.0.data.get()
        }
    }
}

impl<'a, T> Drop for MutexGuard<'a, T> {
    fn drop(&mut self) {
        unsafe {
            pthread_mutex_unlock(
                self.0.lock.get()
            );
        }
    }
}

pub struct RwLock<T> {
    lock: UnsafeCell<pthread_rwlock_t>,
    data: UnsafeCell<T>,
}

unsafe impl<T> Sync for RwLock<T> {}

impl<T> PthreadWrappingPrimitiveConstructor<T> for RwLock<T> {
    fn new(data: T) -> Self {
        RwLock {
            lock: UnsafeCell::new(pthread_rwlock_t::new()),
            data: UnsafeCell::new(data)
        }
    }

    // Need posix_rwlockattr_t and related functions support in libc crate
    fn pshared(_: T) -> Self {
        unimplemented!()
    }
}

#[allow(dead_code)]
impl<T> RwLock<T> { 
    pub fn read(&self) -> Result<RwLockReadGuard<T>> {
        RwLockReadGuard::new(self)
    }

    pub fn write(&self) -> Result<RwLockWriteGuard<T>> {
        RwLockWriteGuard::new(self)
    }
}

#[allow(dead_code)]
pub struct RwLockReadGuard<'a, T: 'a>  (&'a RwLock<T>);

#[allow(dead_code)]
pub struct RwLockWriteGuard<'a, T: 'a> (&'a RwLock<T>);

#[allow(dead_code)]
impl<'a, T> RwLockReadGuard<'a, T> {
    pub fn new(rwlock: &'a RwLock<T>) -> Result<Self> {
        let status = unsafe {
            pthread_rwlock_rdlock(
                rwlock.lock.get()
            )
        };

        if status != 0 {
            Err(Error::Sys(Errno::from_i32(status)))
        } else {
            Ok(RwLockReadGuard(rwlock))
        }
    }
}

#[allow(dead_code)]
impl<'a, T> RwLockWriteGuard<'a, T> {
    pub fn new(rwlock: &'a RwLock<T>) -> Result<Self> {
        let status = unsafe {
            pthread_rwlock_wrlock(
                rwlock.lock.get()
            )
        };

        if status != 0 {
            Err(Error::Sys(Errno::from_i32(status)))
        } else {
            Ok(RwLockWriteGuard(rwlock))
        }
    }
}



impl<'a, T> Deref for RwLockReadGuard<'a, T> {
    type Target = T;
    fn deref(&self) -> &T {
        unsafe {
            &*self.0.data.get()
        }
    }
}

impl<'a, T> Deref for RwLockWriteGuard<'a, T> {
    type Target = T;
    fn deref(&self) -> &T {
        unsafe {
            &*self.0.data.get()
        }
    }
}

impl<'a, T> DerefMut for RwLockWriteGuard<'a, T> {
    fn deref_mut(&mut self) -> &mut T {
        unsafe {
            &mut *self.0.data.get()
        }
    }
}

impl<'a, T> Drop for RwLockReadGuard<'a, T> {
    fn drop(&mut self) {
        unsafe {
            pthread_rwlock_unlock(
                self.0.lock.get()
            );
        }
    }
}

impl<'a, T> Drop for RwLockWriteGuard<'a, T> {
    fn drop(&mut self) {
        unsafe {
            pthread_rwlock_unlock(
                self.0.lock.get()
            );
        }
    }
}
