mod iter;

pub use iter::IteratorExt;

#[macro_export]
macro_rules! ensure {
    ($cond:expr) => {
        if !($cond) {
            return None;
        }
    };
}