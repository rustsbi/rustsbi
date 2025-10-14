use core::mem::MaybeUninit;

/// Size of the FIFO buffer.
const FIFO_SIZE: usize = 16;

#[derive(Debug)]
pub enum FifoError {
    Empty,
    Full,
}

/// A fixed-size FIFO (First In First Out) queue implementation.
pub struct Fifo<T: Copy + Clone> {
    data: [MaybeUninit<T>; FIFO_SIZE],
    head: usize,
    tail: usize,
    count: usize,
}

impl<T: Copy + Clone> Fifo<T> {
    #[inline]
    pub const fn new() -> Self {
        // Initialize array with uninitialized values
        let data = [MaybeUninit::uninit(); FIFO_SIZE];
        Self {
            data,
            head: 0,
            tail: 0,
            count: 0,
        }
    }

    #[inline]
    pub fn is_full(&self) -> bool {
        self.count == FIFO_SIZE
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.count == 0
    }

    pub fn push(&mut self, element: T) -> Result<(), FifoError> {
        if self.is_full() {
            return Err(FifoError::Full);
        }

        // Write element and update tail position
        self.data[self.tail].write(element);
        self.tail = (self.tail + 1) % FIFO_SIZE;
        self.count += 1;

        Ok(())
    }

    pub fn pop(&mut self) -> Result<T, FifoError> {
        if self.is_empty() {
            return Err(FifoError::Empty);
        }

        // unsafe: Take ownership of element at head
        let element = unsafe { self.data[self.head].assume_init_read() };

        // Update head position
        self.head = (self.head + 1) % FIFO_SIZE;
        self.count -= 1;

        Ok(element)
    }
}
