// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.
use {
    anyhow::Result,
    crc32fast::Hasher,
    fatfs::{Dir, FileSystem, FormatVolumeOptions, FsOptions, ReadWriteSeek},
    flate2::{Compression, bufread::ZlibDecoder, bufread::ZlibEncoder},
    fscommon::BufStream,
    gpt::mbr::{PartRecord, ProtectiveMBR},
    std::{
        collections::BTreeMap,
        fs::File,
        io::{BufRead, BufReader, BufWriter, Cursor, Read, Seek, SeekFrom, Write},
        path::Path,
    },
};

mod blkx;
mod koly;
mod xml;

pub use crate::{blkx::*, koly::*, xml::*};

pub struct DmgReader<R: Read + Seek> {
    koly: KolyTrailer,
    xml: Plist,
    r: R,
}

impl DmgReader<BufReader<File>> {
    pub fn open(path: &Path) -> Result<Self> {
        let r = BufReader::with_capacity(10 * 1024 * 1024, File::open(path)?);
        Self::new(r)
    }
}

impl<R: Read + Seek + BufRead> DmgReader<R> {
    pub fn new(mut r: R) -> Result<Self> {
        let koly = KolyTrailer::read_from(&mut r)?;
        r.seek(SeekFrom::Start(koly.plist_offset))?;
        let mut xml = Vec::with_capacity(koly.plist_length as usize);
        (&mut r).take(koly.plist_length).read_to_end(&mut xml)?;
        let xml: Plist = plist::from_reader_xml(&xml[..])?;
        Ok(Self { koly, xml, r })
    }

    pub fn koly(&self) -> &KolyTrailer {
        &self.koly
    }

    pub fn plist(&self) -> &Plist {
        &self.xml
    }

    pub fn sector(&mut self, chunk: &BlkxChunk) -> Result<Box<dyn Read + '_>> {
        let ty = chunk.ty().expect("unknown chunk type");
        match ty {
            ChunkType::Ignore | ChunkType::Zero => {
                Ok(Box::new(std::io::repeat(0).take(chunk.sector_count * 512)))
            }
            ChunkType::Comment => Ok(Box::new(std::io::empty())),
            ChunkType::Raw => {
                self.r.seek(SeekFrom::Start(chunk.compressed_offset))?;
                Ok(Box::new((&mut self.r).take(chunk.compressed_length)))
            }
            ChunkType::Zlib => {
                self.r.seek(SeekFrom::Start(chunk.compressed_offset))?;
                let compressed_chunk = (&mut self.r).take(chunk.compressed_length);
                Ok(Box::new(ZlibDecoder::new(compressed_chunk)))
            }
            ChunkType::Adc | ChunkType::Bzlib | ChunkType::Lzfse => unimplemented!(),
            ChunkType::Term => Ok(Box::new(std::io::empty())),
        }
    }

    pub fn data_checksum(&mut self) -> Result<u32> {
        self.r.seek(SeekFrom::Start(self.koly.data_fork_offset))?;
        let mut data_fork = Vec::with_capacity(self.koly.data_fork_length as usize);
        (&mut self.r)
            .take(self.koly.data_fork_length)
            .read_to_end(&mut data_fork)?;
        Ok(crc32fast::hash(&data_fork))
    }

    pub fn partition_table(&self, i: usize) -> Result<BlkxTable> {
        self.plist().partitions()[i].table()
    }

    pub fn partition_name(&self, i: usize) -> &str {
        &self.plist().partitions()[i].name
    }

    pub fn partition_data(&mut self, i: usize) -> Result<Vec<u8>> {
        let table = self.plist().partitions()[i].table()?;
        let mut partition = vec![];
        for chunk in &table.chunks {
            std::io::copy(&mut self.sector(chunk)?, &mut partition)?;
        }
        Ok(partition)
    }

    pub fn copy_partition_to<W: Write>(&mut self, i: usize, mut writer: W) -> Result<u64> {
        let table = self.plist().partitions()[i].table()?;
        let mut total = 0;
        let mut buffer = vec![0u8; 1024 * 1024];
        for chunk in &table.chunks {
            let mut sector_reader = self.sector(chunk)?;
            loop {
                let n = sector_reader.read(&mut buffer)?;
                if n == 0 {
                    break;
                }
                writer.write_all(&buffer[..n])?;
                total += n as u64;
            }
        }
        Ok(total)
    }

    pub fn into_partition_reader(self, i: usize) -> Result<DmgPartitionReader<R>> {
        let table = self.plist().partitions()[i].table()?;
        let total_size = table
            .chunks
            .iter()
            .filter(|c| c.ty() != Some(ChunkType::Term))
            .map(|c| c.sector_count * 512)
            .sum::<u64>();
        Ok(DmgPartitionReader {
            r: self.r,
            chunks: table.chunks,
            pos: 0,
            total_size,
            cache: BTreeMap::new(),
            cache_order: Vec::new(),
        })
    }
}

pub struct DmgPartitionReader<R: Read + Seek + BufRead> {
    r: R,
    chunks: Vec<BlkxChunk>,
    pos: u64,
    total_size: u64,
    cache: BTreeMap<usize, Vec<u8>>,
    cache_order: Vec<usize>,
}

const MAX_CACHE_CHUNKS: usize = 256;

impl<R: Read + Seek + BufRead> DmgPartitionReader<R> {
    fn get_chunk_at_pos(&self, pos: u64) -> Option<(usize, &BlkxChunk)> {
        let sector = pos / 512;

        // Fast path: check if we're still in the last accessed chunk or the next one
        if let Some(&last_idx) = self.cache_order.last() {
            let c = &self.chunks[last_idx];
            if sector >= c.sector_number && sector < c.sector_number + c.sector_count {
                return Some((last_idx, c));
            }
            // Try next chunk too for sequential access
            if last_idx + 1 < self.chunks.len() {
                let c = &self.chunks[last_idx + 1];
                if sector >= c.sector_number && sector < c.sector_number + c.sector_count {
                    return Some((last_idx + 1, c));
                }
            }
        }

        // Binary search for chunk
        let result = self.chunks.binary_search_by(|c| {
            if sector < c.sector_number {
                std::cmp::Ordering::Greater
            } else if sector >= c.sector_number + c.sector_count {
                std::cmp::Ordering::Less
            } else {
                std::cmp::Ordering::Equal
            }
        });

        match result {
            Ok(idx) => Some((idx, &self.chunks[idx])),
            Err(_) => None,
        }
    }

    fn load_chunk(&mut self, idx: usize) -> Result<()> {
        if self.cache.contains_key(&idx) {
            // Update cache order for LRU
            if let Some(pos) = self.cache_order.iter().position(|&i| i == idx) {
                self.cache_order.remove(pos);
            }
            self.cache_order.push(idx);
            return Ok(());
        }

        let chunk = &self.chunks[idx];
        let mut data = Vec::with_capacity((chunk.sector_count * 512) as usize);

        let ty = chunk.ty().expect("unknown chunk type");
        match ty {
            ChunkType::Ignore | ChunkType::Zero => {
                data.resize((chunk.sector_count * 512) as usize, 0);
            }
            ChunkType::Comment => {}
            ChunkType::Raw => {
                self.r.seek(SeekFrom::Start(chunk.compressed_offset))?;
                data.resize(chunk.compressed_length as usize, 0);
                self.r.read_exact(&mut data)?;
            }
            ChunkType::Zlib => {
                self.r.seek(SeekFrom::Start(chunk.compressed_offset))?;
                let compressed_chunk = (&mut self.r).take(chunk.compressed_length);
                let mut decoder = ZlibDecoder::new(compressed_chunk);
                decoder.read_to_end(&mut data)?;
            }
            _ => unimplemented!("Unsupported chunk type for seeking reader: {:?}", ty),
        }

        if self.cache.len() >= MAX_CACHE_CHUNKS {
            if !self.cache_order.is_empty() {
                let oldest = self.cache_order.remove(0);
                self.cache.remove(&oldest);
            }
        }

        self.cache.insert(idx, data);
        self.cache_order.push(idx);
        Ok(())
    }
}

impl<R: Read + Seek + BufRead> Read for DmgPartitionReader<R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }

        let (idx, chunk_start_bytes, chunk_sector_count) = match self.get_chunk_at_pos(self.pos) {
            Some((idx, chunk)) => (idx, chunk.sector_number * 512, chunk.sector_count),
            None => return Ok(0),
        };

        if let Err(e) = self.load_chunk(idx) {
            return Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                e.to_string(),
            ));
        }

        let chunk_data = &self.cache[&idx];
        let offset_in_chunk = (self.pos - chunk_start_bytes) as usize;
        let available = chunk_data.len().saturating_sub(offset_in_chunk);

        if available == 0 {
            self.pos = chunk_start_bytes + chunk_sector_count * 512;
            return self.read(buf);
        }

        let n = std::cmp::min(buf.len(), available);
        buf[..n].copy_from_slice(&chunk_data[offset_in_chunk..offset_in_chunk + n]);
        self.pos += n as u64;
        Ok(n)
    }
}

impl<R: Read + Seek + BufRead> Seek for DmgPartitionReader<R> {
    fn seek(&mut self, pos: SeekFrom) -> std::io::Result<u64> {
        let new_pos = match pos {
            SeekFrom::Start(s) => s as i64,
            SeekFrom::Current(c) => self.pos as i64 + c,
            SeekFrom::End(e) => self.total_size as i64 + e,
        };

        if new_pos < 0 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "negative seek",
            ));
        }

        self.pos = new_pos as u64;
        Ok(self.pos)
    }
}

impl<R: Read + Seek + BufRead> hfsplus::Read for DmgPartitionReader<R> {
    fn read(&mut self, buf: &mut [u8]) -> hfsplus::Result<usize> {
        Read::read(self, buf).map_err(|e| hfsplus::Error::InvalidData(e.to_string()))
    }
}

impl<R: Read + Seek + BufRead> hfsplus::Seek for DmgPartitionReader<R> {
    fn seek(&mut self, pos: hfsplus::SeekFrom) -> hfsplus::Result<u64> {
        let std_pos = match pos {
            hfsplus::SeekFrom::Start(s) => SeekFrom::Start(s),
            hfsplus::SeekFrom::Current(c) => SeekFrom::Current(c),
            hfsplus::SeekFrom::End(e) => SeekFrom::End(e),
        };
        Seek::seek(self, std_pos).map_err(|e| hfsplus::Error::InvalidData(e.to_string()))
    }
}

pub struct DmgWriter<W: Write + Seek> {
    xml: Plist,
    w: W,
    data_hasher: Hasher,
    main_hasher: Hasher,
    sector_number: u64,
    compressed_offset: u64,
}

impl DmgWriter<BufWriter<File>> {
    pub fn create(path: &Path) -> Result<Self> {
        let w = BufWriter::new(File::create(path)?);
        Ok(Self::new(w))
    }
}

impl<W: Write + Seek> DmgWriter<W> {
    pub fn new(w: W) -> Self {
        Self {
            xml: Default::default(),
            w,
            data_hasher: Hasher::new(),
            main_hasher: Hasher::new(),
            sector_number: 0,
            compressed_offset: 0,
        }
    }

    pub fn create_fat32(mut self, fat32: &[u8]) -> Result<()> {
        anyhow::ensure!(fat32.len() % 512 == 0);
        let sector_count = fat32.len() as u64 / 512;
        let mut mbr = ProtectiveMBR::new();
        let mut partition = PartRecord::new_protective(Some(sector_count.try_into()?));
        partition.os_type = 11;
        mbr.set_partition(0, partition);
        let mbr = mbr.to_bytes().to_vec();
        self.add_partition("Master Boot Record (MBR : 0)", &mbr)?;
        self.add_partition("FAT32 (FAT32 : 1)", fat32)?;
        self.finish()?;
        Ok(())
    }

    pub fn add_partition(&mut self, name: &str, bytes: &[u8]) -> Result<()> {
        anyhow::ensure!(bytes.len() % 512 == 0);
        let id = self.xml.partitions().len() as u32;
        let name = name.to_string();
        let mut table = BlkxTable::new(id, self.sector_number, crc32fast::hash(bytes));
        for chunk in bytes.chunks(2048 * 512) {
            let mut encoder = ZlibEncoder::new(chunk, Compression::best());
            let mut compressed = vec![];
            encoder.read_to_end(&mut compressed)?;
            let compressed_length = compressed.len() as u64;
            let sector_count = chunk.len() as u64 / 512;
            self.w.write_all(&compressed)?;
            self.data_hasher.update(&compressed);
            table.add_chunk(BlkxChunk::new(
                ChunkType::Zlib,
                self.sector_number,
                sector_count,
                self.compressed_offset,
                compressed_length,
            ));
            self.sector_number += sector_count;
            self.compressed_offset += compressed_length;
        }
        table.add_chunk(BlkxChunk::term(self.sector_number, self.compressed_offset));
        self.main_hasher.update(&table.checksum.data[..4]);
        self.xml
            .add_partition(Partition::new(id as i32 - 1, name, table));
        Ok(())
    }

    pub fn finish(mut self) -> Result<()> {
        let mut xml = vec![];
        plist::to_writer_xml(&mut xml, &self.xml)?;
        let pos = self.w.stream_position()?;
        let data_digest = self.data_hasher.finalize();
        let main_digest = self.main_hasher.finalize();
        let koly = KolyTrailer::new(
            pos,
            self.sector_number,
            pos,
            xml.len() as _,
            data_digest,
            main_digest,
        );
        self.w.write_all(&xml)?;
        koly.write_to(&mut self.w)?;
        Ok(())
    }
}

// https://wiki.samba.org/index.php/UNIX_Extensions#Storing_symlinks_on_Windows_servers
fn symlink(target: &str) -> Result<Vec<u8>> {
    let xsym = format!(
        "XSym\n{:04}\n{:x}\n{}\n",
        target.len(),
        md5::compute(target.as_bytes()),
        target,
    );
    let mut xsym = xsym.into_bytes();
    anyhow::ensure!(xsym.len() <= 1067);
    xsym.resize(1067, b' ');
    Ok(xsym)
}

fn add_dir<T: ReadWriteSeek>(src: &Path, dest: &Dir<'_, T>) -> Result<()> {
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let file_name = entry.file_name();
        let file_name = file_name.to_str().unwrap();
        let source = src.join(file_name);
        let file_type = entry.file_type()?;
        if file_type.is_dir() {
            let d = dest.create_dir(file_name)?;
            add_dir(&source, &d)?;
        } else if file_type.is_file() {
            let mut f = dest.create_file(file_name)?;
            std::io::copy(&mut File::open(source)?, &mut f)?;
        } else if file_type.is_symlink() {
            let target = std::fs::read_link(&source)?;
            let xsym = symlink(target.to_str().unwrap())?;
            let mut f = dest.create_file(file_name)?;
            std::io::copy(&mut &xsym[..], &mut f)?;
        }
    }
    Ok(())
}

pub fn create_dmg(dir: &Path, dmg: &Path, volume_label: &str, total_sectors: u32) -> Result<()> {
    let mut fat32 = vec![0; total_sectors as usize * 512];
    {
        let mut volume_label_bytes = [0; 11];
        let end = std::cmp::min(volume_label_bytes.len(), volume_label.len());
        volume_label_bytes[..end].copy_from_slice(&volume_label.as_bytes()[..end]);
        let volume_options = FormatVolumeOptions::new()
            .volume_label(volume_label_bytes)
            .bytes_per_sector(512)
            .total_sectors(total_sectors);
        let mut disk = BufStream::new(Cursor::new(&mut fat32));
        fatfs::format_volume(&mut disk, volume_options)?;
        let fs = FileSystem::new(disk, FsOptions::new())?;
        let file_name = dir.file_name().unwrap().to_str().unwrap();
        let dest = fs.root_dir().create_dir(file_name)?;
        add_dir(dir, &dest)?;
    }
    DmgWriter::create(dmg)?.create_fat32(&fat32)
}

#[cfg(test)]
mod tests {
    use {super::*, gpt::disk::LogicalBlockSize};

    static DMG: &[u8] = include_bytes!("../assets/example.dmg");

    fn print_dmg<R: Read + Seek>(dmg: &DmgReader<R>) -> Result<()> {
        println!("{:?}", dmg.koly());
        println!("{:?}", dmg.plist());
        for partition in dmg.plist().partitions() {
            let table = partition.table()?;
            println!("{table:?}");
            println!("table checksum 0x{:x}", u32::from(table.checksum));
            for (i, chunk) in table.chunks.iter().enumerate() {
                println!("{i} {chunk:?}");
            }
        }
        Ok(())
    }

    #[test]
    fn read_koly_trailer() -> Result<()> {
        let koly = KolyTrailer::read_from(&mut Cursor::new(DMG))?;
        //println!("{:#?}", koly);
        let mut bytes = [0; 512];
        koly.write_to(&mut &mut bytes[..])?;
        let koly2 = KolyTrailer::read_from(&mut Cursor::new(&bytes))?;
        assert_eq!(koly, koly2);
        Ok(())
    }

    #[test]
    fn only_read_dmg() -> Result<()> {
        let mut dmg = DmgReader::new(Cursor::new(DMG))?;
        print_dmg(&dmg)?;
        assert_eq!(
            UdifChecksum::new(dmg.data_checksum()?),
            dmg.koly().data_fork_digest
        );
        let mut buffer = vec![];
        let mut dmg2 = DmgWriter::new(Cursor::new(&mut buffer));
        for i in 0..dmg.plist().partitions().len() {
            let data = dmg.partition_data(i)?;
            let name = dmg.partition_name(i);
            dmg2.add_partition(name, &data)?;
        }
        dmg2.finish()?;
        let mut dmg2 = DmgReader::new(Cursor::new(buffer))?;
        print_dmg(&dmg2)?;
        assert_eq!(
            UdifChecksum::new(dmg2.data_checksum()?),
            dmg2.koly().data_fork_digest
        );
        for i in 0..dmg.plist().partitions().len() {
            let table = dmg.partition_table(i)?;
            let data = dmg.partition_data(i)?;
            let expected = u32::from(table.checksum);
            let calculated = crc32fast::hash(&data);
            assert_eq!(expected, calculated);
        }
        assert_eq!(dmg.koly().main_digest, dmg2.koly().main_digest);
        println!("data crc32 0x{:x}", u32::from(dmg.koly().data_fork_digest));
        println!("main crc32 0x{:x}", u32::from(dmg.koly().main_digest));
        Ok(())
    }

    #[test]
    fn read_dmg_partition_mbr() -> Result<()> {
        let mut dmg = DmgReader::new(Cursor::new(DMG))?;
        let mbr = dmg.partition_data(0)?;
        println!("{mbr:?}");
        let mbr = ProtectiveMBR::from_bytes(&mbr, LogicalBlockSize::Lb512)?;
        println!("{mbr:?}");
        Ok(())
    }

    #[test]
    fn read_dmg_partition_fat32() -> Result<()> {
        let mut dmg = DmgReader::new(Cursor::new(DMG))?;
        let fat32 = dmg.partition_data(1)?;
        let fs = FileSystem::new(Cursor::new(fat32), FsOptions::new())?;
        println!("volume: {}", fs.volume_label());
        for entry in fs.root_dir().iter() {
            let entry = entry?;
            println!("{}", entry.file_name());
        }
        Ok(())
    }

    #[test]
    fn checksum() -> Result<()> {
        let mut dmg = DmgReader::new(Cursor::new(DMG))?;
        assert_eq!(
            UdifChecksum::new(dmg.data_checksum()?),
            dmg.koly().data_fork_digest
        );
        for i in 0..dmg.plist().partitions().len() {
            let table = dmg.partition_table(i)?;
            let data = dmg.partition_data(i)?;
            let expected = u32::from(table.checksum);
            let calculated = crc32fast::hash(&data);
            assert_eq!(expected, calculated);
        }
        Ok(())
    }
}
