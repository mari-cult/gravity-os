#![allow(dead_code)]

use alloc::vec::Vec;
use core::alloc::Layout;

/// A simple compressed memory allocator (zram-like).
/// It takes input data (e.g. a page), compresses it using Zstd,
/// and stores it. It returns a handle to the stored data.
pub struct ZAllocator {
    // In a real zsmalloc, we would have size classes and dedicated pages.
    // Here we wrap the system allocator but provide compression.
    // We use a simple counter for handles.
    // For now, let's simpler: just provide compress/decompress helpers
    // and maybe a simple store if needed.
}

// Global buffer for compression to avoid constant reallocation?
// Thread safety issues if we have multiple cores.
// Let's stick to allocating for now.

/// Compress data using Zstd.
/// Returns a Vec<u8> containing the compressed data.
pub fn zcompress(data: &[u8]) -> Result<Vec<u8>, &'static str> {
    // zstd::encode_all is a convenient helper if available.
    // With default-features=false, we might only have low-level APIs.
    // Using level 1 for speed.
    match zstd::stream::encode_all(data, 1) {
        Ok(compressed) => Ok(compressed),
        Err(_) => Err("Compression failed"),
    }
}

/// Decompress data using Zstd.
pub fn zdecompress(data: &[u8]) -> Result<Vec<u8>, &'static str> {
    match zstd::stream::decode_all(data) {
        Ok(decompressed) => Ok(decompressed),
        Err(_) => Err("Decompression failed"),
    }
}

/// A "ZRAM" block device simulator.
/// Stores pages in a compressed format in memory.
pub struct ZRamDevice {
    blocks: Vec<Option<Vec<u8>>>,
    block_size: usize,
}

impl ZRamDevice {
    pub fn new(num_blocks: usize, block_size: usize) -> Self {
        let mut blocks = Vec::with_capacity(num_blocks);
        for _ in 0..num_blocks {
            blocks.push(None);
        }
        Self { blocks, block_size }
    }

    pub fn write_block(&mut self, index: usize, data: &[u8]) -> Result<(), &'static str> {
        if index >= self.blocks.len() || data.len() != self.block_size {
            return Err("Invalid argument");
        }

        // Compress
        let compressed = zcompress(data)?;
        // Store
        self.blocks[index] = Some(compressed);
        Ok(())
    }

    pub fn read_block(&self, index: usize, out: &mut [u8]) -> Result<(), &'static str> {
        if index >= self.blocks.len() || out.len() != self.block_size {
            return Err("Invalid argument");
        }

        if let Some(ref compressed) = self.blocks[index] {
            match zstd::stream::decode_all(&compressed[..]) {
                Ok(decompressed) => {
                    if decompressed.len() != self.block_size {
                        return Err("Decompressed size mismatch");
                    }
                    out.copy_from_slice(&decompressed);
                    Ok(())
                }
                Err(_) => Err("Decompression failed"),
            }
        } else {
            // Block not present, return zeros?
            out.fill(0);
            Ok(())
        }
    }
}
