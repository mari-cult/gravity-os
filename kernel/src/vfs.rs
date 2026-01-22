//! Simple Virtual Filesystem abstraction

use crate::tarfs::TarFs;
use alloc::sync::Arc;
use alloc::boxed::Box;
use spin::Mutex;

static VFS: Mutex<Option<Vfs>> = Mutex::new(None);

pub trait File: Send + Sync {
    fn read(&mut self, buf: &mut [u8]) -> usize;
    fn read_at(&self, offset: u64, buf: &mut [u8]) -> usize;
    fn seek(&mut self, pos: u64);
    fn size(&self) -> u64;
}

pub struct Vfs {
    tarfs: Arc<TarFs>,
}

pub struct FileHandle {
    pub file: Box<dyn File>,
}

impl Vfs {
    pub fn new(tarfs: TarFs) -> Self {
        Self {
            tarfs: Arc::new(tarfs),
        }
    }
}

/// Initialize the VFS with a TAR filesystem
pub fn init(tarfs: TarFs) {
    let mut vfs = VFS.lock();
    *vfs = Some(Vfs::new(tarfs));
}

/// Open a file by path
pub fn open(path: &str) -> Option<FileHandle> {
    if path == "/dev/random" || path == "/dev/urandom" {
        return Some(FileHandle { file: Box::new(RandomFile { pos: 0 }) });
    }

    let vfs = VFS.lock();
    let vfs = vfs.as_ref()?;

    if let Some(file) = vfs.tarfs.open(path) {
        Some(FileHandle { file })
    } else {
        if path.contains("IOKit") {
            crate::kprintln!("VFS: Failed to find IOKit. Listing candidates:");
            for file in vfs.tarfs.list() {
                if file.name.contains("IOKit") || file.name.starts_with("System") {
                    crate::kprintln!("  Candidate: '{}'", file.name);
                }
            }
        }
        None
    }
}

struct RandomFile {
    pos: u64,
}

impl File for RandomFile {
    fn read(&mut self, buf: &mut [u8]) -> usize {
        for b in buf.iter_mut() {
            *b = 0x42; // Not very random, but good enough for a stub
        }
        buf.len()
    }
    fn read_at(&self, _offset: u64, buf: &mut [u8]) -> usize {
        for b in buf.iter_mut() {
            *b = 0x42;
        }
        buf.len()
    }
    fn seek(&mut self, pos: u64) {
        self.pos = pos;
    }
    fn size(&self) -> u64 {
        u64::MAX
    }
}

/// Stat a file (returns size)
pub fn stat_size(path: &str) -> Option<usize> {
    let vfs = VFS.lock();
    let vfs = vfs.as_ref()?;

    let file = vfs.tarfs.find(path)?;
    Some(file.size)
}

impl FileHandle {
    /// Read bytes from file
    pub fn read(&mut self, buf: &mut [u8]) -> usize {
        self.file.read(buf)
    }

    /// Read at offset
    pub fn read_at(&self, offset: u64, buf: &mut [u8]) -> usize {
        self.file.read_at(offset, buf)
    }

    /// Seek to position
    pub fn seek(&mut self, pos: u64) {
        self.file.seek(pos);
    }

    /// Get file size
    pub fn size(&self) -> u64 {
        self.file.size()
    }
}
