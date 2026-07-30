//! Zero-Copy NoProto Parser for P2P Communication
//! 
//! Implements zero-copy deserialization of AIOS task packets directly
//! from network buffers without memory allocation.

use core::mem;

/// AIOS Task Packet - Strict C alignment for zero-copy parsing
/// 
/// This structure is designed to be read directly from network buffers
/// using slice-overlay, avoiding any memory allocation.
#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct AiosTaskPacket {
    /// Packet magic number for validation (0x41494F53 = "AIOS")
    pub magic: u32,
    /// Lamport logical clock value
    pub clock: u64,
    /// Source node ID
    pub source_id: u8,
    /// Destination node ID (0xFF for broadcast)
    pub dest_id: u8,
    /// Task type (inference, training, sync, etc.)
    pub task_type: TaskType,
    /// Priority level (0-255)
    pub priority: u8,
    /// Tensor data length in bytes
    pub tensor_len: u32,
    /// Parameter data length in bytes
    pub param_len: u32,
    /// Flags (bitfield for various options)
    pub flags: PacketFlags,
    /// Reserved for future use
    pub reserved: [u8; 8],
}

/// Task types for AIOS operations
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskType {
    /// Unknown task
    Unknown = 0,
    /// Inference request
    Inference = 1,
    /// Training request
    Training = 2,
    /// Synchronization
    Sync = 3,
    /// Model update
    ModelUpdate = 4,
    /// Heartbeat
    Heartbeat = 5,
    /// Error report
    Error = 6,
    /// Shutdown
    Shutdown = 7,
}

/// Packet flags bitfield
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PacketFlags {
    /// Require persistence to TicKV
    pub persist: bool,
    /// Requires acknowledgment
    pub require_ack: bool,
    /// Compressed tensor data
    pub compressed: bool,
    /// Encrypted payload
    pub encrypted: bool,
    /// Reserved bits
    pub _reserved: u8,
}

impl PacketFlags {
    /// Create default flags (all false)
    #[must_use]
    pub const fn new() -> Self {
        Self {
            persist: false,
            require_ack: true,
            compressed: false,
            encrypted: false,
            _reserved: 0,
        }
    }

    /// Convert to u8 for serialization
    #[must_use]
    pub const fn as_u8(self) -> u8 {
        let mut flags = 0u8;
        if self.persist {
            flags |= 0x01;
        }
        if self.require_ack {
            flags |= 0x02;
        }
        if self.compressed {
            flags |= 0x04;
        }
        if self.encrypted {
            flags |= 0x08;
        }
        flags | (self._reserved << 4)
    }

    /// Parse from u8
    #[must_use]
    pub const fn from_u8(value: u8) -> Self {
        Self {
            persist: (value & 0x01) != 0,
            require_ack: (value & 0x02) != 0,
            compressed: (value & 0x04) != 0,
            encrypted: (value & 0x08) != 0,
            _reserved: (value >> 4) & 0x0F,
        }
    }
}

impl Default for PacketFlags {
    fn default() -> Self {
        Self::new()
    }
}

/// Magic number for AIOS packets
pub const AIOS_MAGIC: u32 = 0x41494F53; // "AIOS"

/// Total size of the packet header (without payload)
pub const PACKET_HEADER_SIZE: usize = mem::size_of::<AiosTaskPacket>();

impl AiosTaskPacket {
    /// Create a new packet header
    #[must_use]
    pub const fn new(
        clock: u64,
        source_id: u8,
        dest_id: u8,
        task_type: TaskType,
        priority: u8,
        tensor_len: u32,
        param_len: u32,
        flags: PacketFlags,
    ) -> Self {
        Self {
            magic: AIOS_MAGIC,
            clock,
            source_id,
            dest_id,
            task_type,
            priority,
            tensor_len,
            param_len,
            flags,
            reserved: [0; 8],
        }
    }

    /// Validate the packet magic number
    #[must_use]
    pub const fn is_valid(&self) -> bool {
        self.magic == AIOS_MAGIC
    }

    /// Get total packet size including payload
    #[must_use]
    pub const fn total_size(&self) -> usize {
        PACKET_HEADER_SIZE + (self.tensor_len as usize) + (self.param_len as usize)
    }

    /// Get offset to tensor data in the buffer
    #[must_use]
    pub const fn tensor_offset(&self) -> usize {
        PACKET_HEADER_SIZE
    }

    /// Get offset to parameter data in the buffer
    #[must_use]
    pub const fn param_offset(&self) -> usize {
        PACKET_HEADER_SIZE + (self.tensor_len as usize)
    }
}

impl Default for AiosTaskPacket {
    fn default() -> Self {
        Self::new(
            0,
            0,
            0xFF,
            TaskType::Unknown,
            128,
            0,
            0,
            PacketFlags::new(),
        )
    }
}

/// Zero-Copy NoProto Parser
/// 
/// Parses AIOS task packets directly from network buffers without
/// allocating memory. Uses slice-overlay for zero-copy deserialization.
pub struct NoProtoParser;

impl NoProtoParser {
    /// Parse a packet from a raw buffer (zero-copy)
    /// 
    /// # Safety
    /// The buffer must contain at least PACKET_HEADER_SIZE bytes.
    /// The caller must ensure the buffer is valid for the lifetime of the returned reference.
    #[must_use]
    pub unsafe fn parse<'a>(buffer: &'a [u8]) -> Option<&'a AiosTaskPacket> {
        if buffer.len() < PACKET_HEADER_SIZE {
            return None;
        }

        let packet = &*(buffer.as_ptr() as *const AiosTaskPacket);

        if packet.is_valid() {
            Some(packet)
        } else {
            None
        }
    }

    /// Get tensor data slice from buffer (zero-copy)
    /// 
    /// # Safety
    /// The buffer must contain a valid packet with sufficient tensor data.
    #[must_use]
    pub unsafe fn get_tensor_data<'a>(buffer: &'a [u8]) -> Option<&'a [u8]> {
        let packet = Self::parse(buffer)?;
        let tensor_len = packet.tensor_len as usize;
        let offset = packet.tensor_offset();

        if buffer.len() < offset + tensor_len {
            return None;
        }

        Some(&buffer[offset..offset + tensor_len])
    }

    /// Get parameter data slice from buffer (zero-copy)
    /// 
    /// # Safety
    /// The buffer must contain a valid packet with sufficient parameter data.
    #[must_use]
    pub unsafe fn get_param_data<'a>(buffer: &'a [u8]) -> Option<&'a [u8]> {
        let packet = Self::parse(buffer)?;
        let param_len = packet.param_len as usize;
        let offset = packet.param_offset();

        if buffer.len() < offset + param_len {
            return None;
        }

        Some(&buffer[offset..offset + param_len])
    }

    /// Serialize a packet header into a buffer
    /// 
    /// # Safety
    /// The buffer must have at least PACKET_HEADER_SIZE bytes.
    pub unsafe fn serialize_header(buffer: &mut [u8], packet: &AiosTaskPacket) -> bool {
        if buffer.len() < PACKET_HEADER_SIZE {
            return false;
        }

        let dst = buffer.as_mut_ptr() as *mut AiosTaskPacket;
        dst.write(*packet);
        true
    }

    /// Validate a complete packet (header + payload)
    #[must_use]
    pub fn validate_packet(buffer: &[u8]) -> bool {
        if buffer.len() < PACKET_HEADER_SIZE {
            return false;
        }

        unsafe {
            let packet = Self::parse(buffer);
            match packet {
                None => false,
                Some(p) => buffer.len() >= p.total_size(),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_packet_flags() {
        let flags = PacketFlags {
            persist: true,
            require_ack: false,
            compressed: true,
            encrypted: false,
            _reserved: 0,
        };

        let value = flags.as_u8();
        let parsed = PacketFlags::from_u8(value);

        assert_eq!(parsed.persist, flags.persist);
        assert_eq!(parsed.require_ack, flags.require_ack);
        assert_eq!(parsed.compressed, flags.compressed);
        assert_eq!(parsed.encrypted, flags.encrypted);
    }

    #[test]
    fn test_packet_creation() {
        let packet = AiosTaskPacket::new(
            42,
            1,
            2,
            TaskType::Inference,
            255,
            1024,
            512,
            PacketFlags::new(),
        );

        assert!(packet.is_valid());
        assert_eq!(packet.clock, 42);
        assert_eq!(packet.source_id, 1);
        assert_eq!(packet.dest_id, 2);
        assert_eq!(packet.tensor_len, 1024);
        assert_eq!(packet.param_len, 512);
    }

    #[test]
    fn test_packet_offsets() {
        let packet = AiosTaskPacket::new(
            0,
            0,
            0xFF,
            TaskType::Inference,
            128,
            256,
            128,
            PacketFlags::new(),
        );

        assert_eq!(packet.tensor_offset(), PACKET_HEADER_SIZE);
        assert_eq!(packet.param_offset(), PACKET_HEADER_SIZE + 256);
        assert_eq!(packet.total_size(), PACKET_HEADER_SIZE + 256 + 128);
    }

    #[test]
    fn test_zero_copy_parse() {
        let packet = AiosTaskPacket::new(
            100,
            5,
            10,
            TaskType::Training,
            200,
            0,
            0,
            PacketFlags::new(),
        );

        let mut buffer = [0u8; 1024];
        unsafe {
            NoProtoParser::serialize_header(&mut buffer, &packet);
        }

        let parsed = unsafe { NoProtoParser::parse(&buffer) };
        assert!(parsed.is_some());
        let parsed = parsed.unwrap();

        assert_eq!(parsed.clock, 100);
        assert_eq!(parsed.source_id, 5);
        assert_eq!(parsed.dest_id, 10);
        assert_eq!(parsed.task_type, TaskType::Training);
    }

    #[test]
    fn test_zero_copy_with_payload() {
        let packet = AiosTaskPacket::new(
            50,
            1,
            2,
            TaskType::Inference,
            128,
            512,
            256,
            PacketFlags::new(),
        );

        let total_size = packet.total_size();
        let mut buffer = vec![0u8; total_size];

        // Write header
        unsafe {
            NoProtoParser::serialize_header(&mut buffer, &packet);
        }

        // Write tensor data (pattern 0xAA)
        let tensor_offset = packet.tensor_offset();
        for i in 0..packet.tensor_len as usize {
            buffer[tensor_offset + i] = 0xAA;
        }

        // Write param data (pattern 0xBB)
        let param_offset = packet.param_offset();
        for i in 0..packet.param_len as usize {
            buffer[param_offset + i] = 0xBB;
        }

        // Parse and validate
        assert!(NoProtoParser::validate_packet(&buffer));

        // Get tensor data
        let tensor_data = unsafe { NoProtoParser::get_tensor_data(&buffer) };
        assert!(tensor_data.is_some());
        let tensor_data = tensor_data.unwrap();
        assert_eq!(tensor_data.len(), 512);
        assert!(tensor_data.iter().all(|&b| b == 0xAA));

        // Get param data
        let param_data = unsafe { NoProtoParser::get_param_data(&buffer) };
        assert!(param_data.is_some());
        let param_data = param_data.unwrap();
        assert_eq!(param_data.len(), 256);
        assert!(param_data.iter().all(|&b| b == 0xBB));
    }

    #[test]
    fn test_invalid_magic() {
        let mut buffer = [0u8; 1024];
        buffer[0] = 0xFF; // Invalid magic

        let parsed = unsafe { NoProtoParser::parse(&buffer) };
        assert!(parsed.is_none());
    }

    #[test]
    fn test_buffer_too_small() {
        let buffer = [0u8; 10]; // Smaller than header

        let parsed = unsafe { NoProtoParser::parse(&buffer) };
        assert!(parsed.is_none());
    }
}
