//! This example demonstrates how the `HartMask` structure operates in a non-`usize` environment.
//! It simulates a 128-bit RISC-V SBI environment where SBI calls only accept 128-bit parameters.
//! To represent a 128-bit SBI hart mask, we use the `HartMask<u128>` type to complete the SBI call procedure.

use sbi_spec::binary::HartMask;
use std::{
    sync::Mutex,
    thread::{self, JoinHandle},
    time::Duration,
};

/// Number of simulated hardware threads (harts) in the environment.
const N_THREADS: usize = 8;

/// Array of thread handles wrapped in a `Mutex` for safe concurrent access.
/// Each element is an `Option<JoinHandle<()>>`, initialized to `None`.
static THREADS: [Mutex<Option<JoinHandle<()>>>; N_THREADS] =
    [const { Mutex::new(None) }; N_THREADS];

fn main() {
    emulation_init(); // Initialize the simulated SBI environment.
    primary_hart_main(); // Execute the primary hart's logic.
    emulation_finish(); // Clean up the emulation environment.
}

/// Simulates the main logic executed by the primary hart.
fn primary_hart_main() {
    println!("Primary hart is starting");

    // Send an Inter-Processor Interrupt (IPI) to all secondary harts.
    // On a 128-bit RISC-V SBI platform, the `send_ipi` function only accepts
    // `hart_mask` parameters of type `HartMask<u128>`.
    sbi::send_ipi(HartMask::all());

    println!("Primary hart finished");
}

/// Simulates the main logic executed by a secondary hart.
fn secondary_hart_main(hart_id: u128) {
    println!("Secondary hart {} is waiting for interrupt", hart_id);

    // Simulate the "Wait For Interrupt" (WFI) operation.
    // In a real-world scenario, supervisor software might also use the SBI `hart_suspend` function instead.
    wfi();

    // If the secondary harts are woken up by the SBI `send_ipi` function, execution resumes here.
    println!("Secondary hart {} received interrupt", hart_id);
}

/* -- Implementation of a mock SBI runtime -- */

mod sbi {
    use super::{N_THREADS, unpark_thread};
    use sbi_spec::binary::{HartMask, SbiRet};

    /// Mock function to send an IPI to harts specified in the `hart_mask`.
    pub fn send_ipi(hart_mask: HartMask<u128>) -> SbiRet<u128> {
        let (mask, base) = hart_mask.into_inner();

        // If the `hart_mask` specifies all harts, wake up all threads.
        if hart_mask == HartMask::all() {
            for hart_id in 0..N_THREADS as u128 {
                unpark_thread(hart_id);
            }
            return SbiRet::success(0);
        }

        // Or, iterate through each bit in the mask to determine which harts to wake up.
        for bit_offset in 0..128 {
            if (mask & (1 << bit_offset)) != 0 {
                let hart_id = base + bit_offset;
                println!("Hart id {}", hart_id);
                if hart_id < N_THREADS as u128 {
                    unpark_thread(hart_id);
                }
            }
        }

        SbiRet::success(0)
    }
}

/// Initializes the emulation environment by spawning secondary hart threads.
fn emulation_init() {
    println!("Emulation start");

    // Spawn a thread for each secondary hart.
    for i in 0..N_THREADS {
        *THREADS[i].lock().unwrap() = Some(thread::spawn(move || secondary_hart_main(i as u128)));
    }

    // Add a short delay to ensure all threads are properly initialized before the primary hart starts.
    thread::sleep(Duration::from_micros(10));
}

/// Simulates the "Wait For Interrupt" (WFI) operation.
fn wfi() {
    thread::park(); // Blocks the current thread until it is unparked by another thread.
}

/// Cleans up the emulation environment by stopping all secondary harts.
fn emulation_finish() {
    // Add a short delay to ensure all threads have completed their tasks.
    thread::sleep(Duration::from_micros(10));

    // Iterate through all threads, stop them, and wait for their completion.
    for (i, thread) in THREADS.iter().enumerate() {
        if let Some(thread) = thread.lock().unwrap().take() {
            println!("Hart {} stopped", i);
            thread.join().unwrap(); // Wait for the thread to finish execution.
        }
    }
    println!("All harts stopped, emulation finished");
}

/// Unparks (wakes up) a specific hart by its ID.
fn unpark_thread(id: u128) {
    assert!(id < N_THREADS as u128, "Invalid hart ID");

    // Safely access the thread handle and unpark the thread if it exists.
    if let Some(thread) = &*THREADS[id as usize].lock().unwrap() {
        thread.thread().unpark(); // Resumes execution of the parked thread.
    }
}

/* Code execution result analysis:
   The primary hart sends an IPI to all secondary harts using `HartMask::all()`, which
   represents a mask where all bits are set to 1. This triggers the `send_ipi` function
   to wake up all secondary harts. As a result, the output will be:
   - Primary hart is starting
   - Secondary hart 0 is waiting for interrupt
   - Secondary hart 1 is waiting for interrupt
   - ...
   - Secondary hart 7 is waiting for interrupt
   - Secondary hart 0 received interrupt
   - Secondary hart 1 received interrupt
   - ...
   - Secondary hart 7 received interrupt
   - Primary hart finished
   - Hart 0 stopped
   - Hart 1 stopped
   - ...
   - Hart 7 stopped
   - All harts stopped, emulation finished

   To test a scenario where only specific harts receive the IPI, modify the `send_ipi`
   call to use a custom `HartMask` with specific bits set. For example:
   `HartMask::from_raw(0b1010, 0)` would wake up harts with IDs 1 and 3.
*/
