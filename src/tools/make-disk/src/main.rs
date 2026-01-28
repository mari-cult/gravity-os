use apple_dmg::{ChunkType, DmgReader};
use clap::Parser;
use flate2::bufread::ZlibDecoder;
use hfsplus::HFSVolume;
use indicatif::{ProgressBar, ProgressStyle};
use memmap2::MmapMut;
use rayon::prelude::*;
use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::os::unix::fs::FileExt;
use std::path::PathBuf;
use thiserror::Error;

#[derive(Clone, Copy)]
struct SafePtr(*mut u8);
unsafe impl Send for SafePtr {}
unsafe impl Sync for SafePtr {}

#[derive(Error, Debug)]
pub enum DiskError {
    #[error("IO error: {0}")]
    Io(#[from] io::Error),
    #[error("DMG error: {0}")]
    Dmg(String),
    #[error("HFS+ error: {0}")]
    Hfs(String),
}

#[derive(Parser, Debug)]
#[command()]
struct Args {
    /// Path to iOS rootfs DMG
    #[arg(long)]
    ios_dmg: PathBuf,

    /// Output disk image path
    #[arg(long, default_value = "disk.img")]
    output: PathBuf,

    /// Disk size in MB
    #[arg(long, default_value_t = 1536)]
    size_mb: u64,

    /// Rootfs offset in MB
    #[arg(long, default_value_t = 400)]
    rootfs_offset_mb: u64,
}

struct OffsetFile {
    file: File,
    offset: u64,
}

impl Read for OffsetFile {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.file.read(buf)
    }
}

impl Seek for OffsetFile {
    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
        let actual_pos = match pos {
            SeekFrom::Start(s) => SeekFrom::Start(self.offset + s),
            SeekFrom::Current(c) => SeekFrom::Current(c),
            SeekFrom::End(e) => SeekFrom::End(e),
        };
        let new_pos = self.file.seek(actual_pos)?;
        Ok(new_pos.saturating_sub(self.offset))
    }
}

impl hfsplus::Read for OffsetFile {
    fn read(&mut self, buf: &mut [u8]) -> hfsplus::Result<usize> {
        Read::read(self, buf).map_err(|e| hfsplus::Error::InvalidData(e.to_string()))
    }
}

impl hfsplus::Seek for OffsetFile {
    fn seek(&mut self, pos: hfsplus::SeekFrom) -> hfsplus::Result<u64> {
        let std_pos = match pos {
            hfsplus::SeekFrom::Start(s) => SeekFrom::Start(s),
            hfsplus::SeekFrom::Current(c) => SeekFrom::Current(c),
            hfsplus::SeekFrom::End(e) => SeekFrom::End(e),
        };
        Seek::seek(self, std_pos).map_err(|e| hfsplus::Error::InvalidData(e.to_string()))
    }
}

fn main() -> Result<(), DiskError> {
    let args = Args::parse();

    println!("Reading DMG {}...", args.ios_dmg.display());
    let mut dmg = DmgReader::open(&args.ios_dmg).map_err(|e| DiskError::Dmg(e.to_string()))?;

    // Find the HFS+ partition efficiently.
    let mut hfs_partition_index = None;
    let mut hfs_table = None;
    for i in 0..dmg.plist().partitions().len() {
        let table = dmg
            .partition_table(i)
            .map_err(|e| DiskError::Dmg(e.to_string()))?;
        if let Some(chunk) = table
            .chunks
            .iter()
            .find(|c| c.ty() != Some(ChunkType::Comment))
        {
            let mut reader = dmg
                .sector(chunk)
                .map_err(|e| DiskError::Dmg(e.to_string()))?;
            let mut header = vec![0u8; 2048];
            let _ = reader.read_exact(&mut header);
            if header.len() >= 1026
                && (&header[1024..1026] == b"H+" || &header[1024..1026] == b"HX")
            {
                hfs_partition_index = Some(i);
                hfs_table = Some(table);
                break;
            }
        }
    }

    let hfs_partition_index = hfs_partition_index
        .ok_or_else(|| DiskError::Hfs("No HFS+ partition found in DMG".to_string()))?;
    let hfs_table = hfs_table.unwrap();

    println!("Creating disk image {}...", args.output.display());
    let output_file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(true)
        .open(&args.output)?;
    output_file.set_len(args.size_mb * 1024 * 1024)?;

    // Mmap the output file for parallel writes
    let mut mmap = unsafe { MmapMut::map_mut(&output_file)? };
    let mmap_ptr = SafePtr(mmap.as_mut_ptr());

    println!("Parallel streaming HFS+ rootfs to disk image using mmap...");
    let rootfs_offset = (args.rootfs_offset_mb * 1024 * 1024) as usize;
    let dmg_file = File::open(&args.ios_dmg)?;

    let pb = ProgressBar::new(hfs_table.chunks.len() as u64);
    pb.set_style(ProgressStyle::default_bar()
        .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} chunks ({eta})")
        .unwrap()
        .progress_chars("#>-"));

    hfs_table
        .chunks
        .par_iter()
        .try_for_each(|chunk| -> Result<(), DiskError> {
            let p = mmap_ptr;
            let ty = chunk
                .ty()
                .ok_or_else(|| DiskError::Dmg("Unknown chunk type".to_string()))?;
            let output_pos = rootfs_offset + (chunk.sector_number * 512) as usize;

            match ty {
                ChunkType::Zero | ChunkType::Ignore => {
                    // Already zeroed
                }
                ChunkType::Raw => {
                    let mut data = vec![0u8; chunk.compressed_length as usize];
                    dmg_file.read_exact_at(&mut data, chunk.compressed_offset)?;
                    unsafe {
                        let dest = p.0.add(output_pos);
                        core::ptr::copy_nonoverlapping(data.as_ptr(), dest, data.len());
                    }
                }
                ChunkType::Zlib => {
                    let mut compressed_data = vec![0u8; chunk.compressed_length as usize];
                    dmg_file.read_exact_at(&mut compressed_data, chunk.compressed_offset)?;

                    let mut decoder = ZlibDecoder::new(&compressed_data[..]);
                    let mut decompressed_data =
                        Vec::with_capacity((chunk.sector_count * 512) as usize);
                    decoder.read_to_end(&mut decompressed_data)?;
                    unsafe {
                        let dest = p.0.add(output_pos);
                        core::ptr::copy_nonoverlapping(
                            decompressed_data.as_ptr(),
                            dest,
                            decompressed_data.len(),
                        );
                    }
                }
                ChunkType::Comment | ChunkType::Term => {}
                _ => return Err(DiskError::Dmg(format!("Unsupported chunk type: {:?}", ty))),
            }
            pb.inc(1);
            Ok(())
        })?;
    pb.finish_with_message("Rootfs decompressed");
    mmap.flush()?;
    drop(mmap);

    println!("Extracting shared cache from decompressed image...");
    let read_file = File::open(&args.output)?;
    let offset_reader = OffsetFile {
        file: read_file,
        offset: rootfs_offset as u64,
    };

    let volume = HFSVolume::load(offset_reader).map_err(|e| DiskError::Hfs(format!("{:?}", e)))?;

    println!("Listing /sbin...");
    let vol_lock = volume.lock();
    match vol_lock.list_dir("/sbin") {
        Ok(entries) => {
            for (name, record) in entries {
                println!("  - {} ({:?})", name, record.body);
            }
        }
        Err(e) => println!("Failed to list /sbin: {:?}", e),
    }

    println!("Disk image created: {}", args.output.display());
    println!("Done!");
    Ok(())
}
