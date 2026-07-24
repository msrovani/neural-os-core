//! Cell Channel - Transparent Messaging (Silicon & Network Telepathy)
//! 
//! Implements unified messaging abstraction that works transparently
//! between local cores (L3 cache) and remote nodes (network).

use alloc::boxed::Box;
use alloc::vec;
use core::sync::atomic::{AtomicPtr, AtomicU64, Ordering};
use crate::async_rt::SpscChannel;
use crate::net::transport::HybridTransport;

/// Channel type
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChannelType {
    /// Local: shared memory via SpscChannel (~10ns latency)
    Local = 0,
    /// Remote: Raw Ethernet frames (sub-millisecond latency)
    Remote = 1,
}

/// Cell message descriptor
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct CellMessageDescriptor {
    /// Source node/core ID
    pub source_id: u64,
    /// Destination node/core ID
    pub dest_id: u64,
    /// Message type
    pub msg_type: u32,
    /// Payload length
    pub payload_len: u32,
    /// Sequence number
    pub sequence: u64,
}

impl CellMessageDescriptor {
    /// Create a new message descriptor
    #[must_use]
    pub const fn new(source_id: u64, dest_id: u64, msg_type: u32, payload_len: u32, sequence: u64) -> Self {
        Self {
            source_id,
            dest_id,
            msg_type,
            payload_len,
            sequence,
        }
    }

    /// Get total size including descriptor
    #[must_use]
    pub const fn total_size(&self) -> usize {
        core::mem::size_of::<Self>() + self.payload_len as usize
    }
}

/// Cell Channel trait for transparent messaging
/// 
/// Abstraction that works identically for local and remote communication.
/// For hermes and cortex, sending a task to a Cognitive Cell uses the
/// same interface regardless of whether the cell is on the same socket
/// or on another server in the network.
pub trait CellChannel {
    /// Send a message through the channel
    fn send(&self, descriptor: &CellMessageDescriptor, payload: &[u8]) -> Result<(), &'static str>;
    
    /// Receive a message from the channel
    fn receive(&self, descriptor: &mut CellMessageDescriptor, payload: &mut [u8]) -> Result<usize, &'static str>;
    
    /// Get the channel type
    fn channel_type(&self) -> ChannelType;
    
    /// Check if channel is ready
    fn is_ready(&self) -> bool;
}

/// Local cell channel using shared memory (SpscChannel)
/// 
/// Uses AtomicPtr for shared memory between cores with ~10ns latency.
pub struct LocalCellChannel {
    /// SPSC channel for messages
    channel: AtomicPtr<SpscChannel<u8>>,
    /// Buffer for message data
    buffer: *mut u8,
    /// Buffer size
    buffer_size: usize,
}

impl LocalCellChannel {
    /// Create a new local cell channel
    /// 
    /// # Safety
    /// The channel pointer and buffer must be valid for the lifetime of the channel.
    #[must_use]
    pub unsafe fn new(channel: *mut SpscChannel<u8>, buffer: *mut u8, buffer_size: usize) -> Self {
        Self {
            channel: AtomicPtr::new(channel),
            buffer,
            buffer_size,
        }
    }

    /// Get the underlying SPSC channel
    #[must_use]
    pub fn channel(&self) -> &SpscChannel<u8> {
        unsafe { &*self.channel.load(Ordering::Acquire) }
    }
}

impl CellChannel for LocalCellChannel {
    fn send(&self, descriptor: &CellMessageDescriptor, payload: &[u8]) -> Result<(), &'static str> {
        let total_size = descriptor.total_size();
        
        if total_size > self.buffer_size {
            return Err("Payload too large for buffer");
        }

        // Write descriptor to buffer
        unsafe {
            let dst = self.buffer as *mut CellMessageDescriptor;
            dst.write(*descriptor);
        }

        // Write payload to buffer
        unsafe {
            let payload_offset = core::mem::size_of::<CellMessageDescriptor>();
            core::ptr::copy_nonoverlapping(
                payload.as_ptr(),
                self.buffer.add(payload_offset),
                payload.len(),
            );
        }

        // Send notification through channel
        let channel = self.channel();
        if !channel.try_push(1) {
            return Err("Channel full");
        }

        Ok(())
    }

    fn receive(&self, descriptor: &mut CellMessageDescriptor, payload: &mut [u8]) -> Result<usize, &'static str> {
        let channel = self.channel();
        
        // Wait for notification
        match channel.try_pop() {
            Some(_) => {
                // Read descriptor from buffer
                unsafe {
                    let src = self.buffer as *const CellMessageDescriptor;
                    *descriptor = src.read();
                }

                // Read payload from buffer
                let payload_offset = core::mem::size_of::<CellMessageDescriptor>();
                let payload_len = descriptor.payload_len as usize;
                
                if payload.len() < payload_len {
                    return Err("Payload buffer too small");
                }

                unsafe {
                    core::ptr::copy_nonoverlapping(
                        self.buffer.add(payload_offset),
                        payload.as_mut_ptr(),
                        payload_len,
                    );
                }

                Ok(payload_len)
            }
            None => Err("No message available"),
        }
    }

    fn channel_type(&self) -> ChannelType {
        ChannelType::Local
    }

    fn is_ready(&self) -> bool {
        let channel = self.channel();
        !channel.is_empty()
    }
}

/// Remote cell channel using Raw Ethernet
/// 
/// Encapsulates CellMessageDescriptor in raw Ethernet frames without
/// passing through conventional TCP/IP stack (sub-millisecond latency).
pub struct RemoteCellChannel {
    /// Transport for network communication
    transport: *mut HybridTransport,
    /// Destination MAC address
    dest_mac: [u8; 6],
    /// Source node ID
    source_id: u64,
    /// Destination node ID
    dest_id: u64,
    /// Sequence number
    sequence: AtomicU64,
}

impl RemoteCellChannel {
    /// Create a new remote cell channel
    /// 
    /// # Safety
    /// The transport pointer must be valid for the lifetime of the channel.
    #[must_use]
    pub unsafe fn new(
        transport: *mut HybridTransport,
        dest_mac: [u8; 6],
        source_id: u64,
        dest_id: u64,
    ) -> Self {
        Self {
            transport,
            dest_mac,
            source_id,
            dest_id,
            sequence: AtomicU64::new(0),
        }
    }

    /// Get the underlying transport
    #[must_use]
    pub fn transport(&self) -> &HybridTransport {
        unsafe { &*self.transport }
    }
}

impl CellChannel for RemoteCellChannel {
    fn send(&self, descriptor: &CellMessageDescriptor, payload: &[u8]) -> Result<(), &'static str> {
        // Build complete message (descriptor + payload)
        let total_size = descriptor.total_size();
        let mut message = vec![0u8; total_size];

        // Write descriptor
        unsafe {
            let dst = message.as_mut_ptr() as *mut CellMessageDescriptor;
            dst.write(*descriptor);
        }

        // Write payload
        let payload_offset = core::mem::size_of::<CellMessageDescriptor>();
        message[payload_offset..].copy_from_slice(payload);

        // Send via transport
        let transport = self.transport();
        let mut buffer = [0u8; 2048];
        
        transport
            .send_packet(&message, &mut buffer)
            .map_err(|_| "Send failed")?;

        Ok(())
    }

    fn receive(&self, descriptor: &mut CellMessageDescriptor, payload: &mut [u8]) -> Result<usize, &'static str> {
        let transport = self.transport();
        let buffer = [0u8; 2048];

        // Receive from transport
        let size = transport
            .receive_packet(&buffer, payload)
            .map_err(|_| "Receive failed")?;

        // Parse descriptor from received data
        if size < core::mem::size_of::<CellMessageDescriptor>() {
            return Err("Message too short");
        }

        unsafe {
            let src = buffer.as_ptr() as *const CellMessageDescriptor;
            *descriptor = src.read();
        }

        // Extract payload
        let payload_offset = core::mem::size_of::<CellMessageDescriptor>();
        let payload_len = descriptor.payload_len as usize;

        if payload.len() < payload_len {
            return Err("Payload buffer too small");
        }

        payload[..payload_len].copy_from_slice(&buffer[payload_offset..payload_offset + payload_len]);

        Ok(payload_len)
    }

    fn channel_type(&self) -> ChannelType {
        ChannelType::Remote
    }

    fn is_ready(&self) -> bool {
        // For remote channels, we assume ready if transport is initialized
        let transport = self.transport();
        transport.is_initialized()
    }
}

/// Cell channel factory
/// 
/// Creates appropriate channel type based on destination.
pub struct CellChannelFactory;

impl CellChannelFactory {
    /// Create a local channel
    /// 
    /// # Safety
    /// Channel and buffer pointers must be valid.
    pub unsafe fn create_local(
        channel: *mut SpscChannel<u8>,
        buffer: *mut u8,
        buffer_size: usize,
    ) -> LocalCellChannel {
        LocalCellChannel::new(channel, buffer, buffer_size)
    }

    /// Create a remote channel
    /// 
    /// # Safety
    /// Transport pointer must be valid.
    pub unsafe fn create_remote(
        transport: *mut HybridTransport,
        dest_mac: [u8; 6],
        source_id: u64,
        dest_id: u64,
    ) -> RemoteCellChannel {
        RemoteCellChannel::new(transport, dest_mac, source_id, dest_id)
    }

    /// Auto-detect and create appropriate channel
    /// 
    /// If dest_id is on the same physical socket, create local channel.
    /// Otherwise, create remote channel.
    pub fn create_auto(
        source_id: u64,
        dest_id: u64,
        local_channel: Option<LocalCellChannel>,
        remote_channel: Option<RemoteCellChannel>,
    ) -> Box<dyn CellChannel> {
        // Simple heuristic: if IDs are close (same socket), use local
        // In real implementation, would query topology
        let is_local = (source_id as i64 - dest_id as i64).abs() < 64;

        if is_local {
            if let Some(ch) = local_channel {
                Box::new(ch)
            } else {
                // Fallback to remote if local not available
                Box::new(remote_channel.unwrap())
            }
        } else {
            if let Some(ch) = remote_channel {
                Box::new(ch)
            } else {
                // Fallback to local if remote not available
                Box::new(local_channel.unwrap())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cell_message_descriptor() {
        let desc = CellMessageDescriptor::new(1, 2, 3, 100, 42);
        assert_eq!(desc.source_id, 1);
        assert_eq!(desc.dest_id, 2);
        assert_eq!(desc.msg_type, 3);
        assert_eq!(desc.payload_len, 100);
        assert_eq!(desc.total_size(), core::mem::size_of::<CellMessageDescriptor>() + 100);
    }

    #[test]
    fn test_local_cell_channel_send_receive() {
        let mut channel = SpscChannel::new();
        let mut buffer = [0u8; 1024];
        
        let local = unsafe { LocalCellChannel::new(&mut channel, buffer.as_mut_ptr(), buffer.len()) };
        
        let desc = CellMessageDescriptor::new(1, 2, 3, 10, 42);
        let payload = b"hello_world";
        
        let result = local.send(&desc, payload);
        assert!(result.is_ok());
        
        let mut recv_desc = CellMessageDescriptor::new(0, 0, 0, 0, 0);
        let mut recv_payload = [0u8; 20];
        
        let result = local.receive(&mut recv_desc, &mut recv_payload);
        assert!(result.is_ok());
        
        assert_eq!(recv_desc.source_id, 1);
        assert_eq!(recv_desc.dest_id, 2);
        assert_eq!(recv_payload[..10], *payload);
    }

    #[test]
    fn test_local_cell_channel_type() {
        let mut channel = SpscChannel::new();
        let mut buffer = [0u8; 1024];
        
        let local = unsafe { LocalCellChannel::new(&mut channel, buffer.as_mut_ptr(), buffer.len()) };
        
        assert_eq!(local.channel_type(), ChannelType::Local);
    }

    #[test]
    fn test_remote_cell_channel_type() {
        let mut transport = HybridTransport::new(crate::net::transport::TransportConfig::default());
        transport.init().unwrap();
        
        let remote = unsafe { RemoteCellChannel::new(&mut transport, [0xFF; 6], 1, 2) };
        
        assert_eq!(remote.channel_type(), ChannelType::Remote);
    }

    #[test]
    fn test_channel_type_enum() {
        assert_eq!(ChannelType::Local as u8, 0);
        assert_eq!(ChannelType::Remote as u8, 1);
    }
}
