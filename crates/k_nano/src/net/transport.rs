//! Hybrid Transport Pipeline for P2P Communication
//! 
/// Provides transport layer selection between Raw L2 (Ethernet) and UDP/IP (smoltcp)
/// for sending NoProto packets between AIOS nodes.

use core::sync::atomic::{AtomicBool, AtomicU8, AtomicU64, Ordering};

/// Transport mode selection
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransportMode {
    /// Raw L2 Ethernet frames (same subnet)
    RawL2 = 0,
    /// UDP/IP over smoltcp (routed networks)
    UdpIp = 1,
}

/// Transport configuration
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct TransportConfig {
    /// Selected transport mode
    pub mode: TransportMode,
    /// Source MAC address (for Raw L2)
    pub src_mac: [u8; 6],
    /// Destination MAC address (for Raw L2)
    pub dst_mac: [u8; 6],
    /// Source IP address (for UDP/IP)
    pub src_ip: [u8; 4],
    /// Destination IP address (for UDP/IP)
    pub dst_ip: [u8; 4],
    /// UDP port (for UDP/IP)
    pub udp_port: u16,
    /// Ethernet type (for Raw L2, typically 0x0800 for IPv4 or custom for AIOS)
    pub ethertype: u16,
}

impl Default for TransportConfig {
    fn default() -> Self {
        Self {
            mode: TransportMode::RawL2,
            src_mac: [0; 6],
            dst_mac: [0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF], // Broadcast
            src_ip: [0; 4],
            dst_ip: [255, 255, 255, 255], // Broadcast
            udp_port: 0xA105, // AIOS default port
            ethertype: 0xA105, // Custom AIOS ethertype
        }
    }
}

/// Ethernet frame header
#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct EthernetHeader {
    /// Destination MAC address
    pub dst_mac: [u8; 6],
    /// Source MAC address
    pub src_mac: [u8; 6],
    /// EtherType
    pub ethertype: u16,
}

impl EthernetHeader {
    /// Create a new Ethernet header
    #[must_use]
    pub const fn new(dst_mac: [u8; 6], src_mac: [u8; 6], ethertype: u16) -> Self {
        Self {
            dst_mac,
            src_mac,
            ethertype,
        }
    }

    /// Get the size of the Ethernet header
    #[must_use]
    pub const fn size() -> usize {
        14 // 6 + 6 + 2
    }
}

/// UDP header
#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct UdpHeader {
    /// Source port
    pub src_port: u16,
    /// Destination port
    pub dst_port: u16,
    /// Length (header + data)
    pub length: u16,
    /// Checksum
    pub checksum: u16,
}

impl UdpHeader {
    /// Create a new UDP header
    #[must_use]
    pub const fn new(src_port: u16, dst_port: u16, length: u16) -> Self {
        Self {
            src_port,
            dst_port,
            length,
            checksum: 0, // Checksum calculation would be done separately
        }
    }

    /// Get the size of the UDP header
    #[must_use]
    pub const fn size() -> usize {
        8 // 2 + 2 + 2 + 2
    }
}

/// IPv4 header
#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct Ipv4Header {
    /// Version (4 bits) and IHL (4 bits)
    pub version_ihl: u8,
    /// Type of Service
    pub tos: u8,
    /// Total length
    pub total_length: u16,
    /// Identification
    pub identification: u16,
    /// Flags (3 bits) and fragment offset (13 bits)
    pub flags_fragment: u16,
    /// Time to Live
    pub ttl: u8,
    /// Protocol
    pub protocol: u8,
    /// Header checksum
    pub checksum: u16,
    /// Source IP
    pub src_ip: [u8; 4],
    /// Destination IP
    pub dst_ip: [u8; 4],
}

impl Ipv4Header {
    /// Create a new IPv4 header
    #[must_use]
    pub const fn new(
        src_ip: [u8; 4],
        dst_ip: [u8; 4],
        total_length: u16,
        protocol: u8,
    ) -> Self {
        Self {
            version_ihl: 0x45, // Version 4, IHL 5 (20 bytes)
            tos: 0,
            total_length,
            identification: 0,
            flags_fragment: 0x4000, // Don't fragment
            ttl: 64,
            protocol,
            checksum: 0,
            src_ip,
            dst_ip,
        }
    }

    /// Get the size of the IPv4 header
    #[must_use]
    pub const fn size() -> usize {
        20
    }
}

/// Transport error types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransportError {
    /// Buffer too small
    BufferTooSmall,
    /// Invalid configuration
    InvalidConfig,
    /// Send failed
    SendFailed,
    /// Receive failed
    ReceiveFailed,
    /// Invalid packet
    InvalidPacket,
}

/// Hybrid transport for P2P communication
pub struct HybridTransport {
    /// Transport configuration
    config: TransportConfig,
    /// Transport initialized flag
    initialized: AtomicBool,
    /// Packet counter
    packet_count: AtomicU64,
}

impl HybridTransport {
    /// Create a new hybrid transport
    #[must_use]
    pub const fn new(config: TransportConfig) -> Self {
        Self {
            config,
            initialized: AtomicBool::new(false),
            packet_count: AtomicU64::new(0),
        }
    }

    /// Initialize the transport
    pub fn init(&self) -> Result<(), TransportError> {
        // Validate configuration
        match self.config.mode {
            TransportMode::RawL2 => {
                // Validate MAC addresses
                if self.config.src_mac.iter().all(|&b| b == 0) {
                    return Err(TransportError::InvalidConfig);
                }
            }
            TransportMode::UdpIp => {
                // Validate IP addresses
                if self.config.src_ip.iter().all(|&b| b == 0) {
                    return Err(TransportError::InvalidConfig);
                }
            }
        }

        self.initialized.store(true, Ordering::Release);
        Ok(())
    }

    /// Check if transport is initialized
    #[must_use]
    pub fn is_initialized(&self) -> bool {
        self.initialized.load(Ordering::Acquire)
    }

    /// Send a NoProto packet using the configured transport mode
    /// 
    /// # Arguments
    /// * `packet_data` - The NoProto packet data to send
    /// * `buffer` - Output buffer for the framed packet
    /// 
    /// Returns the number of bytes written to the buffer
    pub fn send_packet(&self, packet_data: &[u8], buffer: &mut [u8]) -> Result<usize, TransportError> {
        if !self.is_initialized() {
            return Err(TransportError::InvalidConfig);
        }

        match self.config.mode {
            TransportMode::RawL2 => self.send_raw_l2(packet_data, buffer),
            TransportMode::UdpIp => self.send_udp_ip(packet_data, buffer),
        }
    }

    /// Send using Raw L2 Ethernet
    fn send_raw_l2(&self, packet_data: &[u8], buffer: &mut [u8]) -> Result<usize, TransportError> {
        let header_size = EthernetHeader::size();
        let total_size = header_size + packet_data.len();

        if buffer.len() < total_size {
            return Err(TransportError::BufferTooSmall);
        }

        // Build Ethernet header
        let eth_header = EthernetHeader::new(
            self.config.dst_mac,
            self.config.src_mac,
            self.config.ethertype,
        );

        // Write header to buffer (little-endian for ethertype)
        unsafe {
            let dst = buffer.as_mut_ptr() as *mut EthernetHeader;
            dst.write(eth_header);
        }

        // Convert ethertype to network byte order
        buffer[12] = (self.config.ethertype >> 8) as u8;
        buffer[13] = (self.config.ethertype & 0xFF) as u8;

        // Copy packet data
        buffer[header_size..total_size].copy_from_slice(packet_data);

        // Increment packet counter
        self.packet_count.fetch_add(1, Ordering::Release);

        Ok(total_size)
    }

    /// Send using UDP/IP
    fn send_udp_ip(&self, packet_data: &[u8], buffer: &mut [u8]) -> Result<usize, TransportError> {
        let eth_size = EthernetHeader::size();
        let ip_size = Ipv4Header::size();
        let udp_size = UdpHeader::size();
        let header_size = eth_size + ip_size + udp_size;
        let total_size = header_size + packet_data.len();

        if buffer.len() < total_size {
            return Err(TransportError::BufferTooSmall);
        }

        // Build UDP header
        let udp_length = (udp_size + packet_data.len()) as u16;
        let udp_header = UdpHeader::new(self.config.udp_port, self.config.udp_port, udp_length);

        // Build IPv4 header
        let ip_total_length = (ip_size + udp_size + packet_data.len()) as u16;
        let ip_header = Ipv4Header::new(
            self.config.src_ip,
            self.config.dst_ip,
            ip_total_length,
            17, // UDP protocol
        );

        // Build Ethernet header (for IPv4)
        let eth_header = EthernetHeader::new(
            [0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF], // Broadcast MAC for now
            self.config.src_mac,
            0x0800, // IPv4 ethertype
        );

        // Write headers to buffer
        let mut offset = 0;

        unsafe {
            let dst = buffer.as_mut_ptr().add(offset) as *mut EthernetHeader;
            dst.write(eth_header);
        }
        offset += eth_size;

        unsafe {
            let dst = buffer.as_mut_ptr().add(offset) as *mut Ipv4Header;
            dst.write(ip_header);
        }
        offset += ip_size;

        unsafe {
            let dst = buffer.as_mut_ptr().add(offset) as *mut UdpHeader;
            dst.write(udp_header);
        }
        offset += udp_size;

        // Copy packet data
        buffer[offset..total_size].copy_from_slice(packet_data);

        // Increment packet counter
        self.packet_count.fetch_add(1, Ordering::Release);

        Ok(total_size)
    }

    /// Receive and parse a packet from the buffer
    /// 
    /// # Arguments
    /// * `buffer` - Input buffer containing the received frame
    /// * `output` - Output buffer for the NoProto packet data
    /// 
    /// Returns the number of bytes of NoProto data extracted
    pub fn receive_packet(&self, buffer: &[u8], output: &mut [u8]) -> Result<usize, TransportError> {
        if !self.is_initialized() {
            return Err(TransportError::InvalidConfig);
        }

        match self.config.mode {
            TransportMode::RawL2 => self.receive_raw_l2(buffer, output),
            TransportMode::UdpIp => self.receive_udp_ip(buffer, output),
        }
    }

    /// Receive using Raw L2 Ethernet
    fn receive_raw_l2(&self, buffer: &[u8], output: &mut [u8]) -> Result<usize, TransportError> {
        let header_size = EthernetHeader::size();

        if buffer.len() < header_size {
            return Err(TransportError::InvalidPacket);
        }

        // Check ethertype
        let ethertype = u16::from_be_bytes([buffer[12], buffer[13]]);
        if ethertype != self.config.ethertype {
            return Err(TransportError::InvalidPacket);
        }

        // Extract packet data
        let packet_data = &buffer[header_size..];
        if output.len() < packet_data.len() {
            return Err(TransportError::BufferTooSmall);
        }

        output[..packet_data.len()].copy_from_slice(packet_data);

        Ok(packet_data.len())
    }

    /// Receive using UDP/IP
    fn receive_udp_ip(&self, buffer: &[u8], output: &mut [u8]) -> Result<usize, TransportError> {
        let eth_size = EthernetHeader::size();
        let ip_size = Ipv4Header::size();
        let udp_size = UdpHeader::size();
        let header_size = eth_size + ip_size + udp_size;

        if buffer.len() < header_size {
            return Err(TransportError::InvalidPacket);
        }

        // Check IPv4 ethertype
        let ethertype = u16::from_be_bytes([buffer[12], buffer[13]]);
        if ethertype != 0x0800 {
            return Err(TransportError::InvalidPacket);
        }

        // Extract packet data (skip all headers)
        let packet_data = &buffer[header_size..];
        if output.len() < packet_data.len() {
            return Err(TransportError::BufferTooSmall);
        }

        output[..packet_data.len()].copy_from_slice(packet_data);

        Ok(packet_data.len())
    }

    /// Get the transport configuration
    #[must_use]
    pub const fn config(&self) -> &TransportConfig {
        &self.config
    }

    /// Set the transport mode
    pub fn set_mode(&mut self, mode: TransportMode) {
        self.config.mode = mode;
    }

    /// Get the packet count
    #[must_use]
    pub fn packet_count(&self) -> u64 {
        self.packet_count.load(Ordering::Acquire)
    }

    /// Reset the packet counter
    pub fn reset_packet_count(&self) {
        self.packet_count.store(0, Ordering::Release);
    }
}

impl Default for HybridTransport {
    fn default() -> Self {
        Self::new(TransportConfig::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ethernet_header_size() {
        assert_eq!(EthernetHeader::size(), 14);
    }

    #[test]
    fn test_udp_header_size() {
        assert_eq!(UdpHeader::size(), 8);
    }

    #[test]
    fn test_ipv4_header_size() {
        assert_eq!(Ipv4Header::size(), 20);
    }

    #[test]
    fn test_transport_config_default() {
        let config = TransportConfig::default();
        assert_eq!(config.mode, TransportMode::RawL2);
        assert_eq!(config.udp_port, 0xA105);
    }

    #[test]
    fn test_hybrid_transport_init() {
        let mut config = TransportConfig::default();
        config.src_mac = [0x00, 0x11, 0x22, 0x33, 0x44, 0x55];

        let transport = HybridTransport::new(config);
        assert!(!transport.is_initialized());

        let result = transport.init();
        assert!(result.is_ok());
        assert!(transport.is_initialized());
    }

    #[test]
    fn test_send_raw_l2() {
        let mut config = TransportConfig::default();
        config.src_mac = [0x00, 0x11, 0x22, 0x33, 0x44, 0x55];
        config.dst_mac = [0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF];

        let transport = HybridTransport::new(config);
        transport.init().unwrap();

        let packet_data = b"AIOS_PACKET_DATA";
        let mut buffer = [0u8; 1024];

        let result = transport.send_packet(packet_data, &mut buffer);
        assert!(result.is_ok());

        let size = result.unwrap();
        assert_eq!(size, EthernetHeader::size() + packet_data.len());
    }

    #[test]
    fn test_receive_raw_l2() {
        let mut config = TransportConfig::default();
        config.src_mac = [0x00, 0x11, 0x22, 0x33, 0x44, 0x55];
        config.dst_mac = [0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF];

        let transport = HybridTransport::new(config);
        transport.init().unwrap();

        let packet_data = b"AIOS_PACKET_DATA";
        let mut send_buffer = [0u8; 1024];
        transport.send_packet(packet_data, &mut send_buffer).unwrap();

        let mut receive_buffer = [0u8; 1024];
        let result = transport.receive_packet(&send_buffer, &mut receive_buffer);
        assert!(result.is_ok());

        let size = result.unwrap();
        assert_eq!(size, packet_data.len());
        assert_eq!(&receive_buffer[..size], packet_data);
    }

    #[test]
    fn test_send_udp_ip() {
        let mut config = TransportConfig::default();
        config.mode = TransportMode::UdpIp;
        config.src_ip = [192, 168, 1, 100];
        config.dst_ip = [192, 168, 1, 101];
        config.src_mac = [0x00, 0x11, 0x22, 0x33, 0x44, 0x55];

        let transport = HybridTransport::new(config);
        transport.init().unwrap();

        let packet_data = b"AIOS_UDP_PACKET";
        let mut buffer = [0u8; 1024];

        let result = transport.send_packet(packet_data, &mut buffer);
        assert!(result.is_ok());

        let size = result.unwrap();
        let header_size = EthernetHeader::size() + Ipv4Header::size() + UdpHeader::size();
        assert_eq!(size, header_size + packet_data.len());
    }

    #[test]
    fn test_packet_count() {
        let mut config = TransportConfig::default();
        config.src_mac = [0x00, 0x11, 0x22, 0x33, 0x44, 0x55];

        let transport = HybridTransport::new(config);
        transport.init().unwrap();

        assert_eq!(transport.packet_count(), 0);

        let packet_data = b"TEST";
        let mut buffer = [0u8; 1024];
        transport.send_packet(packet_data, &mut buffer).unwrap();

        assert_eq!(transport.packet_count(), 1);

        transport.send_packet(packet_data, &mut buffer).unwrap();
        assert_eq!(transport.packet_count(), 2);

        transport.reset_packet_count();
        assert_eq!(transport.packet_count(), 0);
    }
}
