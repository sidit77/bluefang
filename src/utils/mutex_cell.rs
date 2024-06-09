use parking_lot::Mutex;

#[derive(Default)]
pub struct MutexCell<T>(Mutex<T>);

impl<T> MutexCell<T> {
    pub const fn new(value: T) -> Self {
        Self(Mutex::new(value))
    }

    pub fn replace(&self, value: T) -> T {
        std::mem::replace(&mut *self.0.lock(), value)
    }

    pub fn set(&self, value: T) {
        *self.0.lock() = value;
    }
}

impl<T: Copy> MutexCell<T> {
    pub fn get(&self) -> T {
        *self.0.lock()
    }
}

impl<T: Default> MutexCell<T> {
    pub fn take(&self) -> T {
        std::mem::take(&mut *self.0.lock())
    }
}
