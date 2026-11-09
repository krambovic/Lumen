//! Native ICMP echo (`IcmpSendEcho` / `Icmp6SendEcho2`).
//!
//! `ping.exe` output is localized ("время=35мс") and cannot be parsed
//! outside English Windows, so Lumen talks to iphlpapi directly.

use std::net::{Ipv4Addr, Ipv6Addr};

pub const ICMP_PAYLOAD: &[u8] = b"lumen-ping-probe";
const REPLY_SIZE: usize = 256 + ICMP_PAYLOAD.len();

#[cfg(windows)]
mod ffi {
    use std::ffi::c_void;

    #[repr(C)]
    pub struct SockaddrIn6 {
        pub family: u16,
        pub port: u16,
        pub flowinfo: u32,
        pub addr: [u8; 16],
        pub scope_id: u32,
    }

    pub const AF_INET6: u16 = 23;
    pub const INVALID_HANDLE: isize = -1;

    #[link(name = "iphlpapi")]
    unsafe extern "system" {
        pub fn IcmpCreateFile() -> isize;
        pub fn Icmp6CreateFile() -> isize;
        pub fn IcmpCloseHandle(handle: isize) -> i32;
        pub fn IcmpSendEcho(
            handle: isize,
            destination: u32,
            request_data: *const c_void,
            request_size: u16,
            request_options: *const c_void,
            reply_buffer: *mut c_void,
            reply_size: u32,
            timeout: u32,
        ) -> u32;
        pub fn Icmp6SendEcho2(
            handle: isize,
            event: isize,
            apc_routine: *const c_void,
            apc_context: *const c_void,
            source: *const SockaddrIn6,
            destination: *const SockaddrIn6,
            request_data: *const c_void,
            request_size: u16,
            request_options: *const c_void,
            reply_buffer: *mut c_void,
            reply_size: u32,
            timeout: u32,
        ) -> u32;
    }

    pub struct Handle(pub isize);

    impl Drop for Handle {
        fn drop(&mut self) {
            // SAFETY: the handle came from Icmp(6)CreateFile and is closed once.
            unsafe {
                IcmpCloseHandle(self.0);
            }
        }
    }
}

/// 8-byte aligned reply buffer.
fn reply_buffer() -> Vec<u64> {
    vec![0u64; REPLY_SIZE.div_ceil(8)]
}

fn read_u32(buffer: &[u64], offset: usize) -> u32 {
    // SAFETY: the u64 buffer is reinterpreted as bytes within its bounds.
    let bytes = unsafe { std::slice::from_raw_parts(buffer.as_ptr() as *const u8, buffer.len() * 8) };
    u32::from_le_bytes([bytes[offset], bytes[offset + 1], bytes[offset + 2], bytes[offset + 3]])
}

/// One IPv4 echo; round trip in ms (at least 1) or `None`.
#[cfg(windows)]
pub fn echo_v4(address: Ipv4Addr, timeout_ms: u32) -> Option<i64> {
    use std::ffi::c_void;
    // SAFETY: plain iphlpapi calls with buffers that outlive them.
    unsafe {
        let raw = ffi::IcmpCreateFile();
        if raw == 0 || raw == ffi::INVALID_HANDLE {
            return None;
        }
        let handle = ffi::Handle(raw);
        let mut reply = reply_buffer();
        let count = ffi::IcmpSendEcho(
            handle.0,
            u32::from_le_bytes(address.octets()),
            ICMP_PAYLOAD.as_ptr() as *const c_void,
            ICMP_PAYLOAD.len() as u16,
            std::ptr::null(),
            reply.as_mut_ptr() as *mut c_void,
            REPLY_SIZE as u32,
            timeout_ms,
        );
        if count == 0 {
            return None;
        }
        // ICMP_ECHO_REPLY: Address, Status, RoundTripTime, ...
        let status = read_u32(&reply, 4);
        let rtt = read_u32(&reply, 8);
        (status == 0).then(|| (rtt as i64).max(1))
    }
}

/// One IPv6 echo; round trip in ms (at least 1) or `None`.
#[cfg(windows)]
pub fn echo_v6(address: Ipv6Addr, scope_id: u32, timeout_ms: u32) -> Option<i64> {
    use std::ffi::c_void;
    let source = ffi::SockaddrIn6 { family: ffi::AF_INET6, port: 0, flowinfo: 0, addr: [0; 16], scope_id: 0 };
    let destination =
        ffi::SockaddrIn6 { family: ffi::AF_INET6, port: 0, flowinfo: 0, addr: address.octets(), scope_id };
    // SAFETY: plain iphlpapi calls with buffers that outlive them.
    unsafe {
        let raw = ffi::Icmp6CreateFile();
        if raw == 0 || raw == ffi::INVALID_HANDLE {
            return None;
        }
        let handle = ffi::Handle(raw);
        let mut reply = reply_buffer();
        let count = ffi::Icmp6SendEcho2(
            handle.0,
            0,
            std::ptr::null(),
            std::ptr::null(),
            &source,
            &destination,
            ICMP_PAYLOAD.as_ptr() as *const c_void,
            ICMP_PAYLOAD.len() as u16,
            std::ptr::null(),
            reply.as_mut_ptr() as *mut c_void,
            REPLY_SIZE as u32,
            timeout_ms,
        );
        if count == 0 {
            return None;
        }
        // The IPv6 address and its padding occupy 28 bytes before the
        // aligned Status and RoundTripTime DWORDs.
        let status = read_u32(&reply, 28);
        let rtt = read_u32(&reply, 32);
        (status == 0).then(|| (rtt as i64).max(1))
    }
}

#[cfg(not(windows))]
pub fn echo_v4(_address: Ipv4Addr, _timeout_ms: u32) -> Option<i64> {
    None
}

#[cfg(not(windows))]
pub fn echo_v6(_address: Ipv6Addr, _scope_id: u32, _timeout_ms: u32) -> Option<i64> {
    None
}

#[cfg(test)]
mod tests {
    #[test]
    fn loopback_echo() {
        if cfg!(windows) {
            assert!(super::echo_v4(std::net::Ipv4Addr::LOCALHOST, 1000).is_some());
        }
    }
}
