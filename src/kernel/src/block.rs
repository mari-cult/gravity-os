//! Block device traits

pub trait BlockReader: Send + Sync {
    fn read_at(&self, offset: u64, buf: &mut [u8]) -> bool;
}
