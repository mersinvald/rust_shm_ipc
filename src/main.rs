extern crate nix;
extern crate rand;

mod shm;
mod queue;
mod process;
mod pthread;

use shm::Shm;
use queue::Queue;
use pthread::PthreadPrimitiveConstructor;
use rand::{SeedableRng, StdRng, Rng};

use std::time::Duration;

fn producer(queue: Shm<Queue<(i32, u32)>>) {
    let pid = process::pid();
    let mut rnd: StdRng = SeedableRng::from_seed(&[pid as usize][..]);

    queue.push((pid, rnd.gen()))
         .unwrap()
}

fn main() {    
    // Create new queue in shm
    let queue = Shm::new(Queue::pshared())
        .unwrap();

    // Spawn random numbers producers
    for _ in 0..10 {
        process::spawn(|| {
            producer(queue.clone());
        }).unwrap();    
    }

    while let Ok(Some(msg)) = queue.timed_pop(Duration::from_millis(10)) {
        println!("PID {}: {}", msg.0, msg.1);
    }   
}