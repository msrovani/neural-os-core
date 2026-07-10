//! Inode — metadados de arquivos e diretorios NeuralFS.
//! 128 bytes cada, armazenados na inode tree como key(object_id, Inode, 0).


#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct Inode {
    pub mode: u16,        // S_IFREG(0100000) | S_IFDIR(0040000) | permissoes
    pub owner: u16,       // uid (reservado)
    pub size: u64,        // tamanho em bytes
    pub ctime: u64,       // criacao (ticks)
    pub mtime: u64,       // modificacao (ticks)
    pub block_count: u64, // blocos alocados
    pub link_count: u32,  // hard links
    pub flags: u32,       // flags (compactado, imutavel, etc)
    pub reserved: [u64; 4], // 32 bytes reservados
}

impl Inode {
    pub const SIZE: usize = 128;
    pub const S_IFREG: u16 = 0x8000;
    pub const S_IFDIR: u16 = 0x4000;

    pub fn new_file() -> Self {
        Inode {
            mode: Inode::S_IFREG | 0o644,
            owner: 0,
            size: 0,
            ctime: 0,
            mtime: 0,
            block_count: 0,
            link_count: 1,
            flags: 0,
            reserved: [0; 4],
        }
    }

    pub fn new_dir() -> Self {
        Inode {
            mode: Inode::S_IFDIR | 0o755,
            owner: 0,
            size: 0,
            ctime: 0,
            mtime: 0,
            block_count: 0,
            link_count: 2,
            flags: 0,
            reserved: [0; 4],
        }
    }

    pub fn to_bytes(&self) -> [u8; 128] {
        let mut b = [0u8; 128];
        b[0..2].copy_from_slice(&self.mode.to_le_bytes());
        b[2..4].copy_from_slice(&self.owner.to_le_bytes());
        b[4..12].copy_from_slice(&self.size.to_le_bytes());
        b[12..20].copy_from_slice(&self.ctime.to_le_bytes());
        b[20..28].copy_from_slice(&self.mtime.to_le_bytes());
        b[28..36].copy_from_slice(&self.block_count.to_le_bytes());
        b[36..40].copy_from_slice(&self.link_count.to_le_bytes());
        b[40..44].copy_from_slice(&self.flags.to_le_bytes());
        b
    }

    pub fn from_bytes(b: &[u8]) -> Self {
        Inode {
            mode: u16::from_le_bytes([b[0], b[1]]),
            owner: u16::from_le_bytes([b[2], b[3]]),
            size: u64::from_le_bytes([b[4], b[5], b[6], b[7], b[8], b[9], b[10], b[11]]),
            ctime: u64::from_le_bytes([b[12], b[13], b[14], b[15], b[16], b[17], b[18], b[19]]),
            mtime: u64::from_le_bytes([b[20], b[21], b[22], b[23], b[24], b[25], b[26], b[27]]),
            block_count: u64::from_le_bytes([b[28], b[29], b[30], b[31], b[32], b[33], b[34], b[35]]),
            link_count: u32::from_le_bytes([b[36], b[37], b[38], b[39]]),
            flags: u32::from_le_bytes([b[40], b[41], b[42], b[43]]),
            reserved: [0; 4],
        }
    }

    pub fn is_dir(&self) -> bool { self.mode & Inode::S_IFDIR != 0 }
    pub fn is_file(&self) -> bool { self.mode & Inode::S_IFREG != 0 }
}
