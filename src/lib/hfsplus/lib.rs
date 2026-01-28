#![no_std]

#[cfg(not(target_os = "none"))]
extern crate std;

extern crate alloc;

use alloc::format;
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec;
use alloc::vec::Vec;
use core::cmp::Ordering;
use core::fmt;
use core::marker::PhantomData;
use spin::Mutex;
use unicode_normalization::UnicodeNormalization;

mod hfs_strings;
pub mod internal;

pub use crate::internal::*;
use hfs_strings::fast_unicode_compare;

pub enum SeekFrom {
    Start(u64),
    Current(i64),
    End(i64),
}

#[derive(Debug)]
pub enum Error {
    InvalidData(String),
    BadNode,
    InvalidRecordKey,
    InvalidRecordType,
    UnsupportedOperation,
    KeyNotFound,
}

pub type Result<T> = core::result::Result<T, Error>;

pub trait Read {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize>;
    fn read_exact(&mut self, mut buf: &mut [u8]) -> Result<()> {
        while !buf.is_empty() {
            match self.read(buf) {
                Ok(0) => break,
                Ok(n) => {
                    let tmp = buf;
                    buf = &mut tmp[n..];
                }
                Err(e) => return Err(e),
            }
        }
        if !buf.is_empty() {
            Err(Error::InvalidData(String::from("Unexpected EOF")))
        } else {
            Ok(())
        }
    }
}

pub trait Write {
    fn write(&mut self, buf: &[u8]) -> Result<usize>;
    fn write_all(&mut self, mut buf: &[u8]) -> Result<()> {
        while !buf.is_empty() {
            match self.write(buf) {
                Ok(0) => break,
                Ok(n) => buf = &buf[n..],
                Err(e) => return Err(e),
            }
        }
        if !buf.is_empty() {
            Err(Error::InvalidData(String::from("Failed to write all data")))
        } else {
            Ok(())
        }
    }
}

pub trait Seek {
    fn seek(&mut self, pos: SeekFrom) -> Result<u64>;
}

pub trait ReadExt: Read {
    fn read_u16_be(&mut self) -> Result<u16> {
        let mut buf = [0u8; 2];
        self.read_exact(&mut buf)?;
        Ok(u16::from_be_bytes(buf))
    }
    fn read_u32_be(&mut self) -> Result<u32> {
        let mut buf = [0u8; 4];
        self.read_exact(&mut buf)?;
        Ok(u32::from_be_bytes(buf))
    }
    fn read_u64_be(&mut self) -> Result<u64> {
        let mut buf = [0u8; 8];
        self.read_exact(&mut buf)?;
        Ok(u64::from_be_bytes(buf))
    }
    fn read_i16_be(&mut self) -> Result<i16> {
        let mut buf = [0u8; 2];
        self.read_exact(&mut buf)?;
        Ok(i16::from_be_bytes(buf))
    }
    fn read_i32_be(&mut self) -> Result<i32> {
        let mut buf = [0u8; 4];
        self.read_exact(&mut buf)?;
        Ok(i32::from_be_bytes(buf))
    }
    fn read_u8(&mut self) -> Result<u8> {
        let mut buf = [0u8; 1];
        self.read_exact(&mut buf)?;
        Ok(buf[0])
    }
    fn read_i8(&mut self) -> Result<i8> {
        let mut buf = [0u8; 1];
        self.read_exact(&mut buf)?;
        Ok(buf[0] as i8)
    }
}

impl<T: Read + ?Sized> ReadExt for T {}

pub trait WriteExt: Write {
    fn write_u16_be(&mut self, n: u16) -> Result<()> {
        self.write_all(&n.to_be_bytes())
    }
    fn write_u32_be(&mut self, n: u32) -> Result<()> {
        self.write_all(&n.to_be_bytes())
    }
    fn write_u64_be(&mut self, n: u64) -> Result<()> {
        self.write_all(&n.to_be_bytes())
    }
    fn write_i8(&mut self, n: i8) -> Result<()> {
        self.write_all(&[n as u8])
    }
    fn write_u8(&mut self, n: u8) -> Result<()> {
        self.write_all(&[n])
    }
}

impl<T: Write + ?Sized> WriteExt for T {}

pub struct Cursor<T> {
    inner: T,
    pos: u64,
}

impl<T: AsRef<[u8]>> Cursor<T> {
    pub fn new(inner: T) -> Self {
        Self { inner, pos: 0 }
    }
}

impl<T: AsRef<[u8]>> Read for Cursor<T> {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
        let inner = self.inner.as_ref();
        if self.pos >= inner.len() as u64 {
            return Ok(0);
        }
        let n = core::cmp::min(buf.len(), (inner.len() as u64 - self.pos) as usize);
        buf[..n].copy_from_slice(&inner[self.pos as usize..self.pos as usize + n]);
        self.pos += n as u64;
        Ok(n)
    }
}

impl<T: AsRef<[u8]>> Seek for Cursor<T> {
    fn seek(&mut self, pos: SeekFrom) -> Result<u64> {
        let inner = self.inner.as_ref();
        let new_pos = match pos {
            SeekFrom::Start(s) => s as i64,
            SeekFrom::Current(c) => self.pos as i64 + c,
            SeekFrom::End(e) => inner.len() as i64 + e,
        };
        if new_pos < 0 {
            return Err(Error::InvalidData(String::from("Invalid seek")));
        }
        self.pos = new_pos as u64;
        Ok(self.pos)
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct HFSString(pub Vec<u16>);

impl fmt::Debug for HFSString {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        for &c in &self.0 {
            if let Some(ch) = core::char::from_u32(c as u32) {
                write!(f, "{}", ch)?;
            } else {
                write!(f, "\\u{{{:04X}}}", c)?;
            }
        }
        Ok(())
    }
}

impl fmt::Display for HFSString {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        for &c in &self.0 {
            if let Some(ch) = core::char::from_u32(c as u32) {
                write!(f, "{}", ch)?;
            }
        }
        Ok(())
    }
}

impl PartialOrd for HFSString {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for HFSString {
    fn cmp(&self, other: &Self) -> Ordering {
        fast_unicode_compare(&self.0[..], &other.0[..])
    }
}

pub trait HFSStringTrait:
    fmt::Debug + fmt::Display + Ord + PartialOrd + Eq + PartialEq + Clone + Sized
{
    fn from_vec(v: Vec<u16>) -> Self;
    fn as_slice(&self) -> &[u16];
}

impl HFSStringTrait for HFSString {
    fn from_vec(v: Vec<u16>) -> Self {
        HFSString(v)
    }
    fn as_slice(&self) -> &[u16] {
        &self.0
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct HFSStringBinary(pub Vec<u16>);

impl fmt::Debug for HFSStringBinary {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        for &c in &self.0 {
            if let Some(ch) = core::char::from_u32(c as u32) {
                write!(f, "{}", ch)?;
            } else {
                write!(f, "\\u{{{:04X}}}", c)?;
            }
        }
        Ok(())
    }
}

impl fmt::Display for HFSStringBinary {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        for &c in &self.0 {
            if let Some(ch) = core::char::from_u32(c as u32) {
                write!(f, "{}", ch)?;
            }
        }
        Ok(())
    }
}

impl PartialOrd for HFSStringBinary {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for HFSStringBinary {
    fn cmp(&self, other: &Self) -> Ordering {
        self.0.cmp(&other.0)
    }
}

impl HFSStringTrait for HFSStringBinary {
    fn from_vec(v: Vec<u16>) -> Self {
        HFSStringBinary(v)
    }
    fn as_slice(&self) -> &[u16] {
        &self.0
    }
}

pub trait Key: fmt::Debug + Ord + PartialOrd + Eq + PartialEq {
    fn import(source: &mut dyn Read) -> Result<Self>
    where
        Self: Sized;
    fn export(&self, source: &mut dyn Write) -> Result<()>;
}

pub trait Record<K> {
    fn import(source: &mut dyn Read, key: K) -> Result<Self>
    where
        Self: Sized;
    fn export(&self, source: &mut dyn Write) -> Result<()>;
    fn get_key(&self) -> &K;
}

pub struct IndexRecord<K> {
    pub key: K,
    pub node_id: u32,
}

impl<K: Key> Record<K> for IndexRecord<K> {
    fn import(source: &mut dyn Read, key: K) -> Result<Self> {
        let node_id = source.read_u32_be()?;
        Ok(IndexRecord { key, node_id })
    }

    fn export(&self, _source: &mut dyn Write) -> Result<()> {
        Err(Error::UnsupportedOperation)
    }

    fn get_key(&self) -> &K {
        &self.key
    }
}

pub struct HeaderNode {
    pub descriptor: BTNodeDescriptor,
    pub header: BTHeaderRec,
    pub user_data: Vec<u8>,
    pub map: Vec<u8>,
}

pub struct MapNode {
    pub _descriptor: BTNodeDescriptor,
}

pub struct IndexNode<K> {
    pub descriptor: BTNodeDescriptor,
    pub records: Vec<IndexRecord<K>>,
}

pub struct LeafNode<R> {
    pub descriptor: BTNodeDescriptor,
    pub records: Vec<Arc<R>>,
}

pub enum Node<K, R> {
    HeaderNode(HeaderNode),
    MapNode(MapNode),
    IndexNode(IndexNode<K>),
    LeafNode(LeafNode<R>),
}

impl<K: Key, R: Record<K>> Node<K, R> {
    fn load(data: &[u8]) -> Result<Node<K, R>> {
        let mut cursor = Cursor::new(data);
        let node = BTNodeDescriptor::import(&mut cursor)?;
        let num_offsets = (node.numRecords + 1) as usize;
        let last_offset_pos = data.len() - num_offsets * 2;
        let mut offsets = Vec::with_capacity(num_offsets);

        for idx in 0..num_offsets {
            let offset_pos = data.len() - 2 - 2 * idx;
            let offset = u16::from_be_bytes([data[offset_pos], data[offset_pos + 1]]) as usize;
            if offset < 14 || offset > last_offset_pos {
                return Err(Error::InvalidData(String::from(
                    "Invalid record offset value",
                )));
            }
            offsets.push(offset);
        }

        let mut records = Vec::new();
        for idx in 0..num_offsets - 1 {
            let first = offsets[idx];
            let last = offsets[idx + 1];
            records.push(&data[first..last]);
        }

        if node.kind == kBTHeaderNode {
            let mut r0_cursor = Cursor::new(records[0]);
            Ok(Node::HeaderNode(HeaderNode {
                descriptor: node,
                header: BTHeaderRec::import(&mut r0_cursor)?,
                user_data: records[1].to_vec(),
                map: records[2].to_vec(),
            }))
        } else if node.kind == kBTMapNode {
            Ok(Node::MapNode(MapNode { _descriptor: node }))
        } else if node.kind == kBTIndexNode {
            let mut r = Vec::<IndexRecord<K>>::new();
            for record in &records {
                let mut v = Cursor::new(record);
                let r2 = K::import(&mut v)?;
                r.push(IndexRecord {
                    key: r2,
                    node_id: v.read_u32_be()?,
                });
            }
            Ok(Node::IndexNode(IndexNode {
                descriptor: node,
                records: r,
            }))
        } else if node.kind == kBTLeafNode {
            let mut r = Vec::<Arc<R>>::new();
            for record in &records {
                let mut v = Cursor::new(record);
                let r2 = K::import(&mut v)?;
                r.push(Arc::new(R::import(&mut v, r2)?));
            }
            Ok(Node::LeafNode(LeafNode {
                descriptor: node,
                records: r,
            }))
        } else {
            Err(Error::InvalidData(String::from("Invalid Node Type")))
        }
    }
}

pub struct BTree<F: Read + Seek, K, R> {
    pub fork: F,
    pub node_size: u16,
    pub header: HeaderNode,
    _key: PhantomData<K>,
    _record: PhantomData<R>,
}

impl<F: Read + Seek, K: Key, R: Record<K>> BTree<F, K, R> {
    pub fn open(mut fork: F) -> Result<BTree<F, K, R>> {
        let mut buffer = vec![0; 512];
        fork.seek(SeekFrom::Start(0))?;
        fork.read_exact(&mut buffer)?;
        let node_size = u16::from_be_bytes([buffer[32], buffer[33]]);

        let mut full_buffer = vec![0; node_size as usize];
        full_buffer[..512].copy_from_slice(&buffer);
        fork.seek(SeekFrom::Start(512))?;
        fork.read_exact(&mut full_buffer[512..])?;

        let header_node = Node::<K, R>::load(&full_buffer)?;
        let header = match header_node {
            Node::HeaderNode(x) => x,
            _ => return Err(Error::BadNode),
        };
        Ok(BTree {
            fork,
            node_size,
            header,
            _key: PhantomData,
            _record: PhantomData,
        })
    }

    pub fn get_node(&mut self, node_num: usize) -> Result<Node<K, R>> {
        let mut buffer = vec![0; self.node_size as usize];
        self.fork
            .seek(SeekFrom::Start((node_num * self.node_size as usize) as u64))?;
        self.fork.read_exact(&mut buffer)?;
        Node::<K, R>::load(&buffer)
    }

    pub fn get_record(&mut self, key: &K) -> Result<Arc<R>> {
        self.get_record_node(key, self.header.header.rootNode as usize)
    }

    fn get_record_node(&mut self, key: &K, node_id: usize) -> Result<Arc<R>> {
        let node = self.get_node(node_id)?;
        match node {
            Node::IndexNode(x) => {
                let mut return_record = &x.records[0];
                if key < &return_record.key {
                    return Err(Error::InvalidRecordKey);
                }
                for record in x.records.iter().skip(1) {
                    if key < &record.key {
                        break;
                    }
                    return_record = record;
                }

                self.get_record_node(key, return_record.node_id as usize)
            }
            Node::LeafNode(mut x) => loop {
                for record in &x.records {
                    if key < record.get_key() {
                        return Err(Error::KeyNotFound);
                    } else if key == record.get_key() {
                        return Ok(Arc::clone(record));
                    }
                }
                if x.descriptor.fLink == 0 {
                    return Err(Error::KeyNotFound);
                }
                let next_node = self.get_node(x.descriptor.fLink as usize)?;
                x = match next_node {
                    Node::LeafNode(x) => x,
                    _ => return Err(Error::BadNode),
                };
            },
            _ => Err(Error::InvalidRecordType),
        }
    }

    pub fn get_record_range(&mut self, first: &K, last: &K) -> Result<Vec<Arc<R>>> {
        self.get_record_range_node(first, last, self.header.header.rootNode as usize)
    }

    fn get_record_range_node(
        &mut self,
        first: &K,
        last: &K,
        node_id: usize,
    ) -> Result<Vec<Arc<R>>> {
        let node = self.get_node(node_id)?;
        match node {
            Node::IndexNode(x) => {
                let mut return_record = &x.records[0];
                if &return_record.key >= last {
                    return Ok(Vec::new());
                }
                for record in x.records.iter().skip(1) {
                    if first < &record.key {
                        break;
                    }
                    return_record = record;
                }
                self.get_record_range_node(first, last, return_record.node_id as usize)
            }
            Node::LeafNode(mut x) => {
                let mut return_records = Vec::new();
                loop {
                    for record in &x.records {
                        if record.get_key() >= last {
                            break;
                        } else if record.get_key() >= first {
                            return_records.push(Arc::clone(record));
                        }
                    }
                    if x.records.is_empty()
                        || x.records[x.records.len() - 1].get_key() >= last
                        || x.descriptor.fLink == 0
                    {
                        break;
                    }
                    let next_node = self.get_node(x.descriptor.fLink as usize)?;
                    x = match next_node {
                        Node::LeafNode(x) => x,
                        _ => return Err(Error::InvalidRecordType),
                    };
                }
                Ok(return_records)
            }
            _ => Err(Error::InvalidRecordType),
        }
    }
}

pub type BTreeArc<F, K, R> = Arc<Mutex<BTree<F, K, R>>>;

pub struct Fork<F: Read + Seek> {
    pub file: Arc<Mutex<F>>,

    pub position: u64,

    pub catalog_id: HFSCatalogNodeID,

    pub fork_type: u8,

    pub block_size: u64,

    pub logical_size: u64,

    pub extents: Vec<(u32, u32, u64, u64)>,

    _phantom: PhantomData<F>,
}

impl<F: Read + Seek> Clone for Fork<F> {
    fn clone(&self) -> Self {
        Fork {
            file: Arc::clone(&self.file),

            position: self.position,

            catalog_id: self.catalog_id,

            fork_type: self.fork_type,

            block_size: self.block_size,

            logical_size: self.logical_size,

            extents: self.extents.clone(),

            _phantom: PhantomData,
        }
    }
}

impl<F: Read + Seek> Fork<F> {
    pub fn load(
        file: Arc<Mutex<F>>,

        catalog_id: HFSCatalogNodeID,

        fork_type: u8,

        volume: &HFSVolume<F>,

        data: &HFSPlusForkData,
    ) -> Result<Fork<F>> {
        let block_size = volume.header.blockSize as u64;

        let mut extents = Vec::with_capacity(8);

        let mut extent_position = 0;

        let mut extent_block = 0;

        let mut extents_result = Some(data.extents);

        while let Some(extent_list) = extents_result {
            for extent in &extent_list {
                if extent.blockCount == 0 {
                    continue;
                }

                let extent_size = extent.blockCount as u64 * block_size;

                let extent_end = extent_position + extent_size;

                let extent_position_clamped = core::cmp::min(data.logicalSize, extent_position);

                let extent_end_clamped = core::cmp::min(data.logicalSize, extent_end);

                extents.push((
                    extent.startBlock,
                    extent.blockCount,
                    extent_position_clamped,
                    extent_end_clamped,
                ));

                extent_position += extent_size;

                extent_block += extent.blockCount;
            }

            extents_result = None;

            if extent_position < data.logicalSize {
                if let Some(et) = &volume.extents_btree {
                    let search_key = ExtentKey::new(catalog_id, fork_type, extent_block);

                    let extent_record = et.lock().get_record(&search_key)?;

                    extents_result = Some(extent_record.body);
                } else {
                    break;
                }
            }
        }

        Ok(Fork {
            file,

            position: 0,

            catalog_id,

            fork_type,

            block_size,

            logical_size: data.logicalSize,

            extents,

            _phantom: PhantomData,
        })
    }

    pub fn read_all(&mut self) -> Result<Vec<u8>> {
        let mut buffer = vec![0; self.logical_size as usize];

        self.seek(SeekFrom::Start(0))?;

        self.read_exact(&mut buffer)?;

        Ok(buffer)
    }
}

impl<F: Read + Seek> Read for Fork<F> {
    fn read(&mut self, buffer: &mut [u8]) -> Result<usize> {
        if self.logical_size == 0 && !self.extents.is_empty() {

            // Decmpfs compressed file logic was here, but we shifted it to HfsFs::open for now.
        }

        let offset = self.position;

        let mut file = self.file.lock();

        let block_size = self.block_size;

        let mut bytes_read = 0;

        for extent in &self.extents {
            let (start_block, _, extent_begin, extent_end) = *extent;

            if offset >= extent_end {
                continue;
            }

            let extent_offset = if offset > extent_begin {
                offset - extent_begin
            } else {
                0
            };

            file.seek(SeekFrom::Start(
                start_block as u64 * block_size + extent_offset,
            ))?;

            let bytes_remaining = buffer.len() - bytes_read;

            let available_in_extent = extent_end - offset - bytes_read as u64; // Corrected available in extent relative to current read progress

            let bytes_to_read = core::cmp::min(available_in_extent, bytes_remaining as u64);

            file.read_exact(&mut buffer[bytes_read as usize..bytes_read + bytes_to_read as usize])?;

            bytes_read += bytes_to_read as usize;

            if bytes_read >= buffer.len() {
                break;
            }
        }

        // Allow short reads (do not error if EOF is reached before buffer is full)

        self.position += bytes_read as u64;

        // DEBUG: Print first 16 bytes of any file read in kernel

        if self.position == bytes_read as u64 && bytes_read >= 16 {

            // We can't use std::eprintln in kernel (target_os=none).

            // But we want to see this in QEMU logs.

            // The kernel has kprintln!

            // But this is a library.
        }

        // Handle Decmpfs header if we just read from resource fork

        if self.fork_type == 0xFF && self.position == bytes_read as u64 && bytes_read >= 16 {
            #[cfg(not(target_os = "none"))]

            std::eprintln!(
                "DEBUG: Resource fork header: {:02x} {:02x} {:02x} {:02x} {:02x} {:02x} {:02x} {:02x}",
                buffer[0],
                buffer[1],
                buffer[2],
                buffer[3],
                buffer[4],
                buffer[5],
                buffer[6],
                buffer[7]
            );

            let magic = u32::from_be_bytes([buffer[0], buffer[1], buffer[2], buffer[3]]);

            if magic == 0x636d7066 {
                // 'cmpf'

                let compression_type =
                    u32::from_be_bytes([buffer[4], buffer[5], buffer[6], buffer[7]]);

                let uncompressed_size = u64::from_be_bytes([
                    buffer[8], buffer[9], buffer[10], buffer[11], buffer[12], buffer[13],
                    buffer[14], buffer[15],
                ]);

                if compression_type == 1 {
                    // Type 1: Data is inline in the header after the 16 bytes

                    let actual_data_size =
                        core::cmp::min(bytes_read - 16, uncompressed_size as usize);

                    buffer.copy_within(16..16 + actual_data_size, 0);

                    return Ok(actual_data_size);
                }
            }
        }

        Ok(bytes_read)
    }
}

impl<F: Read + Seek> Seek for Fork<F> {
    fn seek(&mut self, pos: SeekFrom) -> Result<u64> {
        let new_position = match pos {
            SeekFrom::Start(x) => x,

            SeekFrom::Current(x) => (self.position as i64 + x) as u64,

            _ => return Err(Error::UnsupportedOperation),
        };

        self.position = new_position;

        Ok(new_position)
    }
}

pub enum CatalogBTreeEnum<F: Read + Seek> {
    CaseFolding(BTreeArc<Fork<F>, CatalogKey<HFSString>, CatalogRecord<HFSString>>),

    Binary(BTreeArc<Fork<F>, CatalogKey<HFSStringBinary>, CatalogRecord<HFSStringBinary>>),
}

fn convert_key(k: CatalogKey<HFSStringBinary>) -> CatalogKey<HFSString> {
    CatalogKey {
        _case_match: k._case_match,
        parent_id: k.parent_id,
        node_name: HFSString(k.node_name.0),
    }
}

fn convert_record(rec: CatalogRecord<HFSStringBinary>) -> CatalogRecord<HFSString> {
    CatalogRecord {
        key: convert_key(rec.key),
        body: match rec.body {
            CatalogBody::Folder(f) => CatalogBody::Folder(f),
            CatalogBody::File(f) => CatalogBody::File(f),
            CatalogBody::FolderThread(k) => CatalogBody::FolderThread(convert_key(k)),
            CatalogBody::FileThread(k) => CatalogBody::FileThread(convert_key(k)),
        },
    }
}

pub struct HFSVolume<F: Read + Seek> {
    pub file: Arc<Mutex<F>>,

    pub header: HFSPlusVolumeHeader,

    pub catalog_btree: Option<CatalogBTreeEnum<F>>,

    pub extents_btree: Option<BTreeArc<Fork<F>, ExtentKey, ExtentRecord>>,
}

impl<F: Read + Seek> HFSVolume<F> {
    pub fn load(mut file: F) -> Result<Arc<Mutex<HFSVolume<F>>>> {
        file.seek(SeekFrom::Start(1024))?;

        let header = HFSPlusVolumeHeader::import(&mut file)?;

        if header.signature != HFSP_SIGNATURE && header.signature != HFSX_SIGNATURE {
            return Err(Error::InvalidData(String::from("Invalid volume signature")));
        }

        let file_arc = Arc::new(Mutex::new(file));

        let volume = Arc::new(Mutex::new(HFSVolume {
            file: file_arc,

            header,

            catalog_btree: None,

            extents_btree: None,
        }));

        let catalog_data = volume.lock().header.catalogFile;

        let file_clone = Arc::clone(&volume.lock().file);

        let catalog_fork = {
            let vol_guard = volume.lock();

            Fork::load(file_clone, kHFSCatalogFileID, 0, &*vol_guard, &catalog_data)?
        };

        let temp_btree = BTree::<Fork<F>, CatalogKey<HFSString>, CatalogRecord<HFSString>>::open(
            catalog_fork.clone(),
        )?;

        let compare_type = temp_btree.header.header.keyCompareType;

        let catalog_enum = if compare_type == 0xBC {
            let btree = BTree::<
                Fork<F>,
                CatalogKey<HFSStringBinary>,
                CatalogRecord<HFSStringBinary>,
            >::open(catalog_fork)?;

            CatalogBTreeEnum::Binary(Arc::new(Mutex::new(btree)))
        } else {
            CatalogBTreeEnum::CaseFolding(Arc::new(Mutex::new(temp_btree)))
        };

        volume.lock().catalog_btree = Some(catalog_enum);

        let extents_data = volume.lock().header.extentsFile;

        let file_clone_ext = Arc::clone(&volume.lock().file);

        let extents_fork = {
            let vol_guard = volume.lock();

            Fork::load(
                file_clone_ext,
                kHFSExtentsFileID,
                0,
                &*vol_guard,
                &extents_data,
            )?
        };

        volume.lock().extents_btree = Some(Arc::new(Mutex::new(BTree::open(extents_fork)?)));

        Ok(volume)
    }

    pub fn get_path_record(&self, filename: &str) -> Result<CatalogRecord> {
        match self.catalog_btree.as_ref().unwrap() {
            CatalogBTreeEnum::CaseFolding(btree) => {
                self.get_path_record_impl(filename, &mut *btree.lock())
            }

            CatalogBTreeEnum::Binary(btree) => {
                let rec = self.get_path_record_impl(filename, &mut *btree.lock())?;

                Ok(convert_record(rec))
            }
        }
    }

    fn get_path_record_impl<S>(
        &self,
        filename: &str,
        btree: &mut BTree<Fork<F>, CatalogKey<S>, CatalogRecord<S>>,
    ) -> Result<CatalogRecord<S>>
    where
        S: HFSStringTrait,
    {
        let parts: Vec<&str> = filename.split('/').filter(|s| !s.is_empty()).collect();

        let mut current_folder_id = 2; // kHFSRootFolderID

        let mut current_record: Option<CatalogRecord<S>> = None;

        if parts.is_empty() {
            // Return root record (Thread (2, "") -> Real (1, "VolumeName"))

            let thread_key = CatalogKey {
                _case_match: false,

                parent_id: 2,

                node_name: S::from_vec(vec![]),
            };

            match btree.get_record(&thread_key) {
                Ok(record) => {
                    if let CatalogBody::FolderThread(ref thread_data) = record.body {
                        let real_record = btree.get_record(thread_data)?;

                        return Ok((*real_record).clone());
                    }
                }

                Err(_) => {

                    // Fallback: search parent 1
                }
            }
        }

        for (i, part) in parts.iter().enumerate() {
            let name_utf16: Vec<u16> = part.nfd().collect::<String>().encode_utf16().collect();

            let key = CatalogKey {
                _case_match: false,

                parent_id: current_folder_id,

                node_name: S::from_vec(name_utf16),
            };

            let record = btree.get_record(&key)?;

            current_record = Some((*record).clone());

            match &record.body {
                CatalogBody::Folder(f) => {
                    current_folder_id = f.folderID;
                }

                CatalogBody::File(f) => {
                    if i != parts.len() - 1 {
                        return Err(Error::KeyNotFound);
                    }
                }

                _ => return Err(Error::InvalidRecordType),
            }
        }

        current_record.ok_or(Error::KeyNotFound)
    }

    pub fn list_dir(&self, path: &str) -> Result<Vec<(String, CatalogRecord)>> {
        let record = self.get_path_record(path)?;

        let folder_id = match record.body {
            CatalogBody::Folder(f) => f.folderID,

            _ => return Err(Error::InvalidRecordType),
        };

        match self.catalog_btree.as_ref().unwrap() {
            CatalogBTreeEnum::CaseFolding(btree) => {
                self.list_dir_impl(folder_id, &mut *btree.lock())
            }

            CatalogBTreeEnum::Binary(btree) => {
                let results = self.list_dir_impl(folder_id, &mut *btree.lock())?;

                Ok(results
                    .into_iter()
                    .map(|(n, r)| (n, convert_record(r)))
                    .collect())
            }
        }
    }

    fn list_dir_impl<S>(
        &self,
        folder_id: HFSCatalogNodeID,
        btree: &mut BTree<Fork<F>, CatalogKey<S>, CatalogRecord<S>>,
    ) -> Result<Vec<(String, CatalogRecord<S>)>>
    where
        S: HFSStringTrait,
    {
        let first_key = CatalogKey {
            _case_match: false,

            parent_id: folder_id,

            node_name: S::from_vec(vec![]),
        };

        let last_key = CatalogKey {
            _case_match: false,

            parent_id: folder_id + 1,

            node_name: S::from_vec(vec![]),
        };

        let records = btree.get_record_range(&first_key, &last_key)?;

        let mut results = Vec::new();

        for r in records {
            if r.key.parent_id == folder_id {
                results.push((format!("{}", r.key.node_name), (*r).clone()));
            }
        }

        Ok(results)
    }
}
