# SHM IPC example in Rust

This is an example implementatation of synchronized queue for inter-process communication in shared memory 
for UNIX POSIX-complaint operating systems.

Motivation is to try out rust in unix systems programming

### Goals:
- Safe IPC queue implementation
- Rusty wrappers for pthread sync primitives

### Non-goals:
- Fast IPC queue
- Production-ready code
- Full-featured wrappers

## Library support
Currently this example relies on master branch of rust-lang/libc because of 
<https://github.com/rust-lang/libc/commit/532d80cdc139ea56351e21685bcfba2d3c93e34d>

Required bindings will be availible in version 0.2.21

## Usage example
``` rust
fn child(queue: Shm<Queue<i32>>) {
    let pid = process::pid();
    queue.push(pid).unwrap()
}

fn main() {    
    // Create new queue in shm
    let queue = Shm::new(Queue::pshared())
        .unwrap();

    // Spawn processes
    for _ in 0..10 {
        process::spawn(|| {
            child(queue.clone());
        }).unwrap();    
    }

    while let Ok(Some(pid)) = queue.timed_pop(Duration::from_millis(10)) {
        println!("PID {}", pid);
    }   
}
```