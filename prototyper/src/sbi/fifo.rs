use core::mem::MaybeUninit;

const FIFO_SIZE: usize = 16;

pub enum FifoError {
    Empty,
    Full,
}

pub struct Fifo<T: Copy + Clone> {
    data: [MaybeUninit<T>; FIFO_SIZE],
    size: usize,
    avil: usize,
    tail: usize,
}

impl<T: Copy + Clone> Fifo<T> {
    pub fn new() -> Fifo<T> {
        let data: [MaybeUninit<T>; FIFO_SIZE] = [const { MaybeUninit::uninit() }; FIFO_SIZE];
        Fifo {
            data,
            size: FIFO_SIZE,
            avil: 0,
            tail: 0,
        }
    }
    pub fn is_full(&self) -> bool {
        self.avil == self.size
    }
    pub fn is_empty(&self) -> bool {
        self.avil == 0
    }

    pub fn push(&mut self, new_element: T) -> Result<(), FifoError> {
        if self.is_full() {
            return Err(FifoError::Full);
        }
        self.data[self.tail].write(new_element);
        self.tail += 1;
        self.avil += 1;
        if self.tail >= self.size {
            self.tail -= self.size;
        }

        Ok(())
    }

    pub fn pop(&mut self) -> Result<T, FifoError> {
        if self.is_empty() {
            return Err(FifoError::Empty);
        }
        let raw_head = self.tail as isize - self.avil as isize;
        let head = if raw_head < 0 {
            raw_head + self.size as isize
        } else {
            raw_head
        } as usize;

        self.avil -= 1;
        let result = unsafe { self.data[head].assume_init_ref() }.clone();
        unsafe { self.data[head].assume_init_drop() }
        Ok(result)
    }
}
