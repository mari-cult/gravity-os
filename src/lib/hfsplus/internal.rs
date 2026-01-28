use crate::{Read, ReadExt, Result, Write, WriteExt};

#[derive(Debug, Copy, Clone)]
pub struct HFSPlusBSDInfo {
    pub ownerID: u32,
    pub groupID: u32,
    pub adminFlags: u8,
    pub ownerFlags: u8,
    pub fileMode: u16,
    pub special: u32,
}

impl HFSPlusBSDInfo {
    pub fn import(source: &mut dyn Read) -> Result<Self> {
        Ok(Self {
            ownerID: source.read_u32_be()?,
            groupID: source.read_u32_be()?,
            adminFlags: source.read_u8()?,
            ownerFlags: source.read_u8()?,
            fileMode: source.read_u16_be()?,
            special: source.read_u32_be()?,
        })
    }
}

pub const S_ISUID: u16 = 0o0004000;
pub const S_ISGID: u16 = 0o0002000;
pub const S_ISTXT: u16 = 0o0001000;

pub const S_IRWXU: u16 = 0o0000700;
pub const S_IRUSR: u16 = 0o0000400;
pub const S_IWUSR: u16 = 0o0000200;
pub const S_IXUSR: u16 = 0o0000100;

pub const S_IRWXG: u16 = 0o0000070;
pub const S_IRGRP: u16 = 0o0000040;
pub const S_IWGRP: u16 = 0o0000020;
pub const S_IXGRP: u16 = 0o0000010;

pub const S_IRWXO: u16 = 0o0000007;
pub const S_IROTH: u16 = 0o0000004;
pub const S_IWOTH: u16 = 0o0000002;
pub const S_IXOTH: u16 = 0o0000001;

pub const S_IFMT: u16 = 0o0170000;
pub const S_IFIFO: u16 = 0o0010000;
pub const S_IFCHR: u16 = 0o0020000;
pub const S_IFDIR: u16 = 0o0040000;
pub const S_IFBLK: u16 = 0o0060000;
pub const S_IFREG: u16 = 0o0100000;
pub const S_IFLNK: u16 = 0o0120000;
pub const S_IFSOCK: u16 = 0o0140000;
pub const S_IFWHT: u16 = 0o0160000;

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct HFSPlusForkData {
    pub logicalSize: u64,
    pub clumpSize: u32,
    pub totalBlocks: u32,
    pub extents: HFSPlusExtentRecord,
}

pub type HFSPlusExtentRecord = [HFSPlusExtentDescriptor; 8];

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct HFSPlusExtentDescriptor {
    pub startBlock: u32,
    pub blockCount: u32,
}

impl HFSPlusForkData {
    pub fn import(source: &mut dyn Read) -> Result<Self> {
        Ok(Self {
            logicalSize: source.read_u64_be()?,
            clumpSize: source.read_u32_be()?,
            totalBlocks: source.read_u32_be()?,
            extents: import_record(source)?,
        })
    }

    pub fn export(&self, source: &mut dyn Write) -> Result<()> {
        source.write_u64_be(self.logicalSize)?;
        source.write_u32_be(self.clumpSize)?;
        source.write_u32_be(self.totalBlocks)?;
        export_record(&self.extents, source)?;
        Ok(())
    }
}

pub fn import_record(source: &mut dyn Read) -> Result<HFSPlusExtentRecord> {
    Ok([
        HFSPlusExtentDescriptor::import(source)?,
        HFSPlusExtentDescriptor::import(source)?,
        HFSPlusExtentDescriptor::import(source)?,
        HFSPlusExtentDescriptor::import(source)?,
        HFSPlusExtentDescriptor::import(source)?,
        HFSPlusExtentDescriptor::import(source)?,
        HFSPlusExtentDescriptor::import(source)?,
        HFSPlusExtentDescriptor::import(source)?,
    ])
}

pub fn export_record(record: &[HFSPlusExtentDescriptor], source: &mut dyn Write) -> Result<()> {
    for r in record {
        r.export(source)?;
    }
    Ok(())
}

impl HFSPlusExtentDescriptor {
    pub fn import(source: &mut dyn Read) -> Result<Self> {
        Ok(Self {
            startBlock: source.read_u32_be()?,
            blockCount: source.read_u32_be()?,
        })
    }

    pub fn export(&self, source: &mut dyn Write) -> Result<()> {
        source.write_u32_be(self.startBlock)?;
        source.write_u32_be(self.blockCount)?;
        Ok(())
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct HFSPlusVolumeHeader {
    pub signature: u16,
    pub version: u16,
    pub attributes: u32,
    pub lastMountedVersion: u32,
    pub journalInfoBlock: u32,
    pub createDate: u32,
    pub modifyDate: u32,
    pub backupDate: u32,
    pub checkedDate: u32,
    pub fileCount: u32,
    pub folderCount: u32,
    pub blockSize: u32,
    pub totalBlocks: u32,
    pub freeBlocks: u32,
    pub nextAllocation: u32,
    pub rsrcClumpSize: u32,
    pub dataClumpSize: u32,
    pub nextCatalogID: u32,
    pub writeCount: u32,
    pub encodingsBitmap: u64,
    pub finderInfo: [u32; 8],
    pub allocationFile: HFSPlusForkData,
    pub extentsFile: HFSPlusForkData,
    pub catalogFile: HFSPlusForkData,
    pub attributesFile: HFSPlusForkData,
    pub startupFile: HFSPlusForkData,
}

impl HFSPlusVolumeHeader {
    pub fn import(source: &mut dyn Read) -> Result<Self> {
        Ok(Self {
            signature: source.read_u16_be()?,
            version: source.read_u16_be()?,
            attributes: source.read_u32_be()?,
            lastMountedVersion: source.read_u32_be()?,
            journalInfoBlock: source.read_u32_be()?,
            createDate: source.read_u32_be()?,
            modifyDate: source.read_u32_be()?,
            backupDate: source.read_u32_be()?,
            checkedDate: source.read_u32_be()?,
            fileCount: source.read_u32_be()?,
            folderCount: source.read_u32_be()?,
            blockSize: source.read_u32_be()?,
            totalBlocks: source.read_u32_be()?,
            freeBlocks: source.read_u32_be()?,
            nextAllocation: source.read_u32_be()?,
            rsrcClumpSize: source.read_u32_be()?,
            dataClumpSize: source.read_u32_be()?,
            nextCatalogID: source.read_u32_be()?,
            writeCount: source.read_u32_be()?,
            encodingsBitmap: source.read_u64_be()?,
            finderInfo: [
                source.read_u32_be()?,
                source.read_u32_be()?,
                source.read_u32_be()?,
                source.read_u32_be()?,
                source.read_u32_be()?,
                source.read_u32_be()?,
                source.read_u32_be()?,
                source.read_u32_be()?,
            ],
            allocationFile: HFSPlusForkData::import(source)?,
            extentsFile: HFSPlusForkData::import(source)?,
            catalogFile: HFSPlusForkData::import(source)?,
            attributesFile: HFSPlusForkData::import(source)?,
            startupFile: HFSPlusForkData::import(source)?,
        })
    }
}

pub const HFSP_SIGNATURE: u16 = 0x482b;
pub const HFSX_SIGNATURE: u16 = 0x4858;

#[derive(Debug, PartialEq, Eq)]
pub struct BTNodeDescriptor {
    pub fLink: u32,
    pub bLink: u32,
    pub kind: i8,
    pub height: u8,
    pub numRecords: u16,
    pub reserved: u16,
}

impl BTNodeDescriptor {
    pub fn import(source: &mut dyn Read) -> Result<Self> {
        Ok(Self {
            fLink: source.read_u32_be()?,
            bLink: source.read_u32_be()?,
            kind: source.read_i8()?,
            height: source.read_u8()?,
            numRecords: source.read_u16_be()?,
            reserved: source.read_u16_be()?,
        })
    }

    pub fn export(&self, source: &mut dyn Write) -> Result<()> {
        source.write_u32_be(self.fLink)?;
        source.write_u32_be(self.bLink)?;
        source.write_i8(self.kind)?;
        source.write_u8(self.height)?;
        source.write_u16_be(self.numRecords)?;
        source.write_u16_be(self.reserved)?;
        Ok(())
    }
}

pub const kBTLeafNode: i8 = -1;
pub const kBTIndexNode: i8 = 0;
pub const kBTHeaderNode: i8 = 1;
pub const kBTMapNode: i8 = 2;

pub const kBTHeaderNodeKind: u8 = 1;
pub const kBTLeafNodeKind: u8 = 255; // -1 as u8

#[derive(Debug, PartialEq, Eq)]
pub struct BTHeaderRec {
    pub treeDepth: u16,
    pub rootNode: u32,
    pub leafRecords: u32,
    pub firstLeafNode: u32,
    pub lastLeafNode: u32,
    pub nodeSize: u16,
    pub maxKeyLength: u16,
    pub totalNodes: u32,
    pub freeNodes: u32,
    pub reserved1: u16,
    pub clumpSize: u32,
    pub btreeType: u8,
    pub keyCompareType: u8,
    pub attributes: u32,
    pub reserved3: [u32; 16],
}

impl BTHeaderRec {
    pub fn import(source: &mut dyn Read) -> Result<Self> {
        Ok(Self {
            treeDepth: source.read_u16_be()?,
            rootNode: source.read_u32_be()?,
            leafRecords: source.read_u32_be()?,
            firstLeafNode: source.read_u32_be()?,
            lastLeafNode: source.read_u32_be()?,
            nodeSize: source.read_u16_be()?,
            maxKeyLength: source.read_u16_be()?,
            totalNodes: source.read_u32_be()?,
            freeNodes: source.read_u32_be()?,
            reserved1: source.read_u16_be()?,
            clumpSize: source.read_u32_be()?,
            btreeType: source.read_u8()?,
            keyCompareType: source.read_u8()?,
            attributes: source.read_u32_be()?,
            reserved3: [
                source.read_u32_be()?,
                source.read_u32_be()?,
                source.read_u32_be()?,
                source.read_u32_be()?,
                source.read_u32_be()?,
                source.read_u32_be()?,
                source.read_u32_be()?,
                source.read_u32_be()?,
                source.read_u32_be()?,
                source.read_u32_be()?,
                source.read_u32_be()?,
                source.read_u32_be()?,
                source.read_u32_be()?,
                source.read_u32_be()?,
                source.read_u32_be()?,
                source.read_u32_be()?,
            ],
        })
    }

    pub fn export(&self, source: &mut dyn Write) -> Result<()> {
        source.write_u16_be(self.treeDepth)?;
        source.write_u32_be(self.rootNode)?;
        source.write_u32_be(self.leafRecords)?;
        source.write_u32_be(self.firstLeafNode)?;
        source.write_u32_be(self.lastLeafNode)?;
        source.write_u16_be(self.nodeSize)?;
        source.write_u16_be(self.maxKeyLength)?;
        source.write_u32_be(self.totalNodes)?;
        source.write_u32_be(self.freeNodes)?;
        source.write_u16_be(self.reserved1)?;
        source.write_u32_be(self.clumpSize)?;
        source.write_u8(self.btreeType)?;
        source.write_u8(self.keyCompareType)?;
        source.write_u32_be(self.attributes)?;
        for r in &self.reserved3 {
            source.write_u32_be(*r)?;
        }
        Ok(())
    }
}

pub type HFSCatalogNodeID = u32;
pub const kHFSCatalogFileID: HFSCatalogNodeID = 4;
pub const kHFSExtentsFileID: HFSCatalogNodeID = 3;

pub const kHFSPlusFolderRecord: i16 = 0x0001;
pub const kHFSPlusFileRecord: i16 = 0x0002;
pub const kHFSPlusFolderThreadRecord: i16 = 0x0003;
pub const kHFSPlusFileThreadRecord: i16 = 0x0004;

#[derive(Debug, Copy, Clone)]
pub struct HFSPlusCatalogFolder {
    pub flags: u16,
    pub valence: u32,
    pub folderID: HFSCatalogNodeID,
    pub createDate: u32,
    pub contentModDate: u32,
    pub attributeModDate: u32,
    pub accessDate: u32,
    pub backupDate: u32,
    pub permissions: HFSPlusBSDInfo,
    pub userInfo: FolderInfo,
    pub finderInfo: ExtendedFolderInfo,
    pub textEncoding: u32,
    pub reserved: u32,
}

impl HFSPlusCatalogFolder {
    pub fn import(source: &mut dyn Read) -> Result<Self> {
        Ok(Self {
            flags: source.read_u16_be()?,
            valence: source.read_u32_be()?,
            folderID: source.read_u32_be()?,
            createDate: source.read_u32_be()?,
            contentModDate: source.read_u32_be()?,
            attributeModDate: source.read_u32_be()?,
            accessDate: source.read_u32_be()?,
            backupDate: source.read_u32_be()?,
            permissions: HFSPlusBSDInfo::import(source)?,
            userInfo: FolderInfo::import(source)?,
            finderInfo: ExtendedFolderInfo::import(source)?,
            textEncoding: source.read_u32_be()?,
            reserved: source.read_u32_be()?,
        })
    }
}

#[derive(Debug, Copy, Clone)]
pub struct HFSPlusCatalogFile {
    pub flags: u16,
    pub reserved1: u32,
    pub fileID: HFSCatalogNodeID,
    pub createDate: u32,
    pub contentModDate: u32,
    pub attributeModDate: u32,
    pub accessDate: u32,
    pub backupDate: u32,
    pub permissions: HFSPlusBSDInfo,
    pub userInfo: FileInfo,
    pub finderInfo: ExtendedFileInfo,
    pub textEncoding: u32,
    pub reserved2: u32,
    pub dataFork: HFSPlusForkData,
    pub resourceFork: HFSPlusForkData,
}

impl HFSPlusCatalogFile {
    pub fn import(source: &mut dyn Read) -> Result<Self> {
        Ok(Self {
            flags: source.read_u16_be()?,
            reserved1: source.read_u32_be()?,
            fileID: source.read_u32_be()?,
            createDate: source.read_u32_be()?,
            contentModDate: source.read_u32_be()?,
            attributeModDate: source.read_u32_be()?,
            accessDate: source.read_u32_be()?,
            backupDate: source.read_u32_be()?,
            permissions: HFSPlusBSDInfo::import(source)?,
            userInfo: FileInfo::import(source)?,
            finderInfo: ExtendedFileInfo::import(source)?,
            textEncoding: source.read_u32_be()?,
            reserved2: source.read_u32_be()?,
            dataFork: HFSPlusForkData::import(source)?,
            resourceFork: HFSPlusForkData::import(source)?,
        })
    }
}

#[derive(Debug, Copy, Clone)]
pub struct Point {
    pub v: i16,
    pub h: i16,
}
impl Point {
    pub fn import(source: &mut dyn Read) -> Result<Self> {
        Ok(Self {
            v: source.read_i16_be()?,
            h: source.read_i16_be()?,
        })
    }
}

#[derive(Debug, Copy, Clone)]
pub struct Rect {
    pub top: i16,
    pub left: i16,
    pub bottom: i16,
    pub right: i16,
}
impl Rect {
    pub fn import(source: &mut dyn Read) -> Result<Self> {
        Ok(Self {
            top: source.read_i16_be()?,
            left: source.read_i16_be()?,
            bottom: source.read_i16_be()?,
            right: source.read_i16_be()?,
        })
    }
}

#[derive(Debug, Copy, Clone)]
pub struct FileInfo {
    pub fileType: u32,
    pub fileCreator: u32,
    pub finderFlags: u16,
    pub location: Point,
    pub reservedField: u16,
}
impl FileInfo {
    pub fn import(source: &mut dyn Read) -> Result<Self> {
        Ok(Self {
            fileType: source.read_u32_be()?,
            fileCreator: source.read_u32_be()?,
            finderFlags: source.read_u16_be()?,
            location: Point::import(source)?,
            reservedField: source.read_u16_be()?,
        })
    }
}

#[derive(Debug, Copy, Clone)]
pub struct ExtendedFileInfo {
    pub reserved1: [i16; 4],
    pub extendedFinderFlags: u16,
    pub reserved2: i16,
    pub putAwayFolderID: i32,
}
impl ExtendedFileInfo {
    pub fn import(source: &mut dyn Read) -> Result<Self> {
        Ok(Self {
            reserved1: [
                source.read_i16_be()?,
                source.read_i16_be()?,
                source.read_i16_be()?,
                source.read_i16_be()?,
            ],
            extendedFinderFlags: source.read_u16_be()?,
            reserved2: source.read_i16_be()?,
            putAwayFolderID: source.read_i32_be()?,
        })
    }
}

#[derive(Debug, Copy, Clone)]
pub struct FolderInfo {
    pub windowBounds: Rect,
    pub finderFlags: u16,
    pub location: Point,
    pub reservedField: u16,
}
impl FolderInfo {
    pub fn import(source: &mut dyn Read) -> Result<Self> {
        Ok(Self {
            windowBounds: Rect::import(source)?,
            finderFlags: source.read_u16_be()?,
            location: Point::import(source)?,
            reservedField: source.read_u16_be()?,
        })
    }
}

#[derive(Debug, Copy, Clone)]
pub struct ExtendedFolderInfo {
    pub scrollPosition: Point,
    pub reserved1: i32,
    pub extendedFinderFlags: u16,
    pub reserved2: i16,
    pub putAwayFolderID: i32,
}
impl ExtendedFolderInfo {
    pub fn import(source: &mut dyn Read) -> Result<Self> {
        Ok(Self {
            scrollPosition: Point::import(source)?,
            reserved1: source.read_i32_be()?,
            extendedFinderFlags: source.read_u16_be()?,
            reserved2: source.read_i16_be()?,
            putAwayFolderID: source.read_i32_be()?,
        })
    }
}

#[derive(Debug, Copy, Clone)]
pub struct HFSPlusExtentKey {
    pub keyLength: u16,
    pub forkType: u8,
    pub pad: u8,
    pub fileID: u32,
    pub startBlock: u32,
}
impl HFSPlusExtentKey {
    pub fn import(source: &mut dyn Read) -> Result<Self> {
        Ok(Self {
            keyLength: source.read_u16_be()?,
            forkType: source.read_u8()?,
            pad: source.read_u8()?,
            fileID: source.read_u32_be()?,
            startBlock: source.read_u32_be()?,
        })
    }
    pub fn export(&self, source: &mut dyn Write) -> Result<()> {
        source.write_u16_be(self.keyLength)?;
        source.write_u8(self.forkType)?;
        source.write_u8(self.pad)?;
        source.write_u32_be(self.fileID)?;
        source.write_u32_be(self.startBlock)?;
        Ok(())
    }
}

#[derive(Debug, Copy, Clone)]
pub struct ExtentKey(pub HFSPlusExtentKey);

impl ExtentKey {
    pub fn new(file_id: HFSCatalogNodeID, fork_type: u8, start_block: u32) -> Self {
        ExtentKey(HFSPlusExtentKey {
            keyLength: 10,
            forkType: fork_type,
            pad: 0,
            fileID: file_id,
            startBlock: start_block,
        })
    }
}

impl crate::Key for ExtentKey {
    fn import(source: &mut dyn Read) -> Result<Self> {
        Ok(ExtentKey(HFSPlusExtentKey::import(source)?))
    }

    fn export(&self, source: &mut dyn Write) -> Result<()> {
        self.0.export(source)?;
        Ok(())
    }
}

impl core::cmp::PartialOrd for ExtentKey {
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl core::cmp::Ord for ExtentKey {
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        match self.0.fileID.cmp(&other.0.fileID) {
            core::cmp::Ordering::Less => core::cmp::Ordering::Less,
            core::cmp::Ordering::Greater => core::cmp::Ordering::Greater,
            core::cmp::Ordering::Equal => match self.0.forkType.cmp(&other.0.forkType) {
                core::cmp::Ordering::Less => core::cmp::Ordering::Less,
                core::cmp::Ordering::Greater => core::cmp::Ordering::Greater,
                core::cmp::Ordering::Equal => self.0.startBlock.cmp(&other.0.startBlock),
            },
        }
    }
}

impl core::cmp::PartialEq for ExtentKey {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == core::cmp::Ordering::Equal
    }
}

impl core::cmp::Eq for ExtentKey {}

#[derive(Debug, Clone)]
pub struct CatalogKey<S = crate::HFSString> {
    pub _case_match: bool,
    pub parent_id: HFSCatalogNodeID,
    pub node_name: S,
}

impl<S: crate::HFSStringTrait> crate::Key for CatalogKey<S> {
    fn import(source: &mut dyn Read) -> Result<Self> {
        let key_length = source.read_u16_be()?;
        if key_length < 6 {
            return Err(crate::Error::InvalidRecordKey);
        }
        let parent_id = source.read_u32_be()?;
        let count = source.read_u16_be()?;
        let mut node_name = alloc::vec::Vec::with_capacity(count as usize);
        for _ in 0..count as usize {
            node_name.push(source.read_u16_be()?);
        }
        Ok(Self {
            _case_match: false,
            parent_id,
            node_name: S::from_vec(node_name),
        })
    }

    fn export(&self, _source: &mut dyn Write) -> Result<()> {
        Err(crate::Error::UnsupportedOperation)
    }
}

impl<S: Ord> core::cmp::PartialOrd for CatalogKey<S> {
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl<S: Ord> core::cmp::Ord for CatalogKey<S> {
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        match self.parent_id.cmp(&other.parent_id) {
            core::cmp::Ordering::Less => core::cmp::Ordering::Less,
            core::cmp::Ordering::Greater => core::cmp::Ordering::Greater,
            core::cmp::Ordering::Equal => self.node_name.cmp(&other.node_name),
        }
    }
}

impl<S: PartialEq + Ord> core::cmp::PartialEq for CatalogKey<S> {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == core::cmp::Ordering::Equal
    }
}

impl<S: Eq + Ord> core::cmp::Eq for CatalogKey<S> {}

#[derive(Debug, Clone)]
pub enum CatalogBody<S = crate::HFSString> {
    Folder(HFSPlusCatalogFolder),
    File(HFSPlusCatalogFile),
    FolderThread(CatalogKey<S>),
    FileThread(CatalogKey<S>),
}

#[derive(Debug, Clone)]
pub struct CatalogRecord<S = crate::HFSString> {
    pub key: CatalogKey<S>,
    pub body: CatalogBody<S>,
}

impl<S: crate::HFSStringTrait> crate::Record<CatalogKey<S>> for CatalogRecord<S> {
    fn import(source: &mut dyn Read, key: CatalogKey<S>) -> Result<Self> {
        let record_type = source.read_i16_be()?;
        let body = match record_type {
            kHFSPlusFolderRecord => CatalogBody::Folder(HFSPlusCatalogFolder::import(source)?),
            kHFSPlusFileRecord => CatalogBody::File(HFSPlusCatalogFile::import(source)?),
            kHFSPlusFolderThreadRecord => {
                let _reserved = source.read_i16_be()?;
                let parent_id = source.read_u32_be()?;
                let count = source.read_u16_be()?;
                let mut node_name = alloc::vec::Vec::with_capacity(count as usize);
                for _ in 0..count as usize {
                    node_name.push(source.read_u16_be()?);
                }
                let to_key = CatalogKey {
                    _case_match: false,
                    parent_id,
                    node_name: S::from_vec(node_name),
                };
                CatalogBody::FolderThread(to_key)
            }
            kHFSPlusFileThreadRecord => {
                let _reserved = source.read_i16_be()?;
                let parent_id = source.read_u32_be()?;
                let count = source.read_u16_be()?;
                let mut node_name = alloc::vec::Vec::with_capacity(count as usize);
                for _ in 0..count as usize {
                    node_name.push(source.read_u16_be()?);
                }
                let to_key = CatalogKey {
                    _case_match: false,
                    parent_id,
                    node_name: S::from_vec(node_name),
                };
                CatalogBody::FileThread(to_key)
            }
            _ => {
                return Err(crate::Error::InvalidRecordType);
            }
        };
        Ok(CatalogRecord { key, body })
    }

    fn export(&self, _source: &mut dyn Write) -> Result<()> {
        Err(crate::Error::UnsupportedOperation)
    }

    fn get_key(&self) -> &CatalogKey<S> {
        &self.key
    }
}

#[derive(Debug, Clone)]
pub struct ExtentRecord {
    pub key: ExtentKey,
    pub body: HFSPlusExtentRecord,
}

impl crate::Record<ExtentKey> for ExtentRecord {
    fn import(source: &mut dyn Read, key: ExtentKey) -> Result<Self> {
        let body = import_record(source)?;
        Ok(ExtentRecord { key, body })
    }

    fn export(&self, source: &mut dyn Write) -> Result<()> {
        export_record(&self.body, source)?;
        Ok(())
    }

    fn get_key(&self) -> &ExtentKey {
        &self.key
    }
}
