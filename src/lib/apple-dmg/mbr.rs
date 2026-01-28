use byteorder::{ByteOrder, LittleEndian};

#[derive(Clone, Copy, Debug, Default)]
pub struct PartRecord {
    pub boot_indicator: u8,
    pub start_head: u8,
    pub start_sector: u8,
    pub start_track: u8,
    pub os_type: u8,
    pub end_head: u8,
    pub end_sector: u8,
    pub end_track: u8,
    pub lb_start: u32,
    pub lb_len: u32,
}

impl PartRecord {
    pub fn new_protective(size_sectors: Option<u32>) -> Self {
        let lb_len = size_sectors.unwrap_or(0xFFFFFFFF);
        Self {
            boot_indicator: 0,
            start_head: 0x00,
            start_sector: 0x02,
            start_track: 0x00,
            os_type: 0xEE,
            end_head: 0xFF,
            end_sector: 0xFF,
            end_track: 0xFF,
            lb_start: 1,
            lb_len,
        }
    }

    pub fn write_to(&self, buf: &mut [u8]) {
        buf[0] = self.boot_indicator;
        buf[1] = self.start_head;
        buf[2] = self.start_sector;
        buf[3] = self.start_track;
        buf[4] = self.os_type;
        buf[5] = self.end_head;
        buf[6] = self.end_sector;
        buf[7] = self.end_track;
        LittleEndian::write_u32(&mut buf[8..12], self.lb_start);
        LittleEndian::write_u32(&mut buf[12..16], self.lb_len);
    }
}

#[derive(Debug)]
pub struct ProtectiveMBR {
    pub partitions: [PartRecord; 4],
}

impl ProtectiveMBR {
    pub fn new() -> Self {
        Self {
            partitions: [PartRecord::default(); 4],
        }
    }

    pub fn set_partition(&mut self, index: usize, partition: PartRecord) {
        if index < 4 {
            self.partitions[index] = partition;
        }
    }

    pub fn to_bytes(&self) -> [u8; 512] {
        let mut buf = [0u8; 512];
        // Partition table at offset 446
        for (i, p) in self.partitions.iter().enumerate() {
            let offset = 446 + i * 16;
            p.write_to(&mut buf[offset..offset + 16]);
        }

        // Signature
        buf[510] = 0x55;
        buf[511] = 0xAA;

        buf
    }

    #[allow(dead_code)]
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, &'static str> {
        if bytes.len() < 512 {
            return Err("buffer too small");
        }
        if bytes[510] != 0x55 || bytes[511] != 0xAA {
            return Err("invalid signature");
        }
        let mut partitions = [PartRecord::default(); 4];
        #[allow(clippy::needless_range_loop)]
        for i in 0..4 {
            let offset = 446 + i * 16;
            let p = &bytes[offset..offset + 16];
            partitions[i] = PartRecord {
                boot_indicator: p[0],
                start_head: p[1],
                start_sector: p[2],
                start_track: p[3],
                os_type: p[4],
                end_head: p[5],
                end_sector: p[6],
                end_track: p[7],
                lb_start: LittleEndian::read_u32(&p[8..12]),
                lb_len: LittleEndian::read_u32(&p[12..16]),
            };
        }
        Ok(Self { partitions })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mbr_serialization() {
        let mut mbr = ProtectiveMBR::new();
        let mut p = PartRecord::new_protective(Some(1000));
        p.os_type = 0x0B;
        mbr.set_partition(0, p);

        let bytes = mbr.to_bytes();
        assert_eq!(bytes[510], 0x55);
        assert_eq!(bytes[511], 0xAA);

        // Check partition 1
        let offset = 446;
        assert_eq!(bytes[offset + 4], 0x0B); // os_type
        let start = LittleEndian::read_u32(&bytes[offset + 8..]);
        let len = LittleEndian::read_u32(&bytes[offset + 12..]);
        assert_eq!(start, 1);
        assert_eq!(len, 1000);
    }
}
