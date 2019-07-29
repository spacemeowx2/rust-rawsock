use std::ffi::{CStr, CString};
use crate::{Error,  BorrowedPacket, DataLink, traits, Stats};
use super::dll::{PCapHandle, WPCapDll, SUCCESS, PCapPacketHeader, PCapSendQueue};
use super::dll::helpers::PCapErrBuf;
use libc::{ c_int, c_uint};
use std::mem::uninitialized;
use time::Timespec;
use std::slice::from_raw_parts;
use super::structs::PCapStat;
use crate::utils::cstr_to_string;

const QUEUE_SIZE: usize = 65536 * 8; //min 8 packets

///wpcap specific Interface representation.
pub struct Interface<'a> {
    handle: * const PCapHandle,
    dll: & 'a WPCapDll,
    datalink: DataLink,
    queue: * mut PCapSendQueue
}


impl<'a> Interface<'a> {
    pub fn new(name: &str, dll: &'a WPCapDll) ->Result<Self, Error> {
        let name = CString::new(name)?;
        let mut errbuf =  PCapErrBuf::new();
        let handle = unsafe { dll.pcap_open_live(
            name.as_ptr(),
            65536,                  /* max packet size */
            8,                      /* promiscuous mode */
            1000,                   /* read timeout in milliseconds */
            errbuf.buffer()
        )};
        if handle.is_null() {
            return Err(Error::OpeningInterface(errbuf.as_string()))
        }
        let queue = unsafe{dll.pcap_sendqueue_alloc(QUEUE_SIZE as c_uint)};
        assert!(!queue.is_null());
        let datalink = match unsafe{dll.pcap_datalink(handle)}{
            1 => DataLink::Ethernet,
            12 => DataLink::RawIp,
            _=> DataLink::Other
        };

        Ok(Interface {
            dll,
            queue,
            handle,
            datalink
        })
    }

    fn last_error(&self) -> Error {
        let cerr = unsafe{self.dll.pcap_geterr(self.handle)};
        Error::LibraryError(cstr_to_string(cerr))
    }
}

impl<'a> Drop for Interface<'a> {
    fn drop(&mut self) {
        unsafe {
            self.dll.pcap_sendqueue_destroy(self.queue);
            self.dll.pcap_close(self.handle)
        }
    }
}

impl<'a> traits::Interface<'a> for Interface<'a> {
    fn send(&self, packet: &[u8]) -> Result<(), Error> {
        let header = PCapPacketHeader {
            len: packet.len() as c_uint,
            caplen: packet.len() as c_uint,
            ts: unsafe{uninitialized()},
            comment: unsafe{uninitialized()},
        };

        let err = unsafe{self.dll.pcap_sendqueue_queue(self.queue, &header, packet.as_ptr())};
        if err != 0 {
            self.flush();
            let err = unsafe {self.dll.pcap_sendqueue_queue(self.queue, &header, packet.as_ptr())};
            assert_eq!(err,0);
        }

        if unsafe {self.dll.pcap_sendpacket(self.handle, packet.as_ptr(), packet.len() as c_int)} == SUCCESS {
            Ok(())
        } else {
            let txt = unsafe {CStr::from_ptr(self.dll.pcap_geterr(self.handle))}.to_string_lossy().into_owned();
            Err(Error::SendingPacket(txt))
        }
    }

    fn receive<'b>(&'b mut self) -> Result<BorrowedPacket<'b>, Error>{
        let mut header: PCapPacketHeader = unsafe {uninitialized()};
        //TODO: replace pcap_next with pcap_next_ex to obtain more error information
        let data = unsafe { self.dll.pcap_next(self.handle, &mut header)};
        if data.is_null() {
            Err(Error::ReceivingPacket("Unknown error when obtaining packet".into()))
        } else {
            Ok(
                unsafe {
                    BorrowedPacket::new(Timespec::new(header.ts.tv_sec as i64, (header.ts.tv_usec * 1000) as i32), from_raw_parts(data, header.caplen as usize))
                })
        }
    }

    fn flush(&self) {
        unsafe {
            self.dll.pcap_sendqueue_transmit(self.handle, self.queue, 0);
            /*
            Those calls are reported by masscan code to be necessary
            although I can't find any reason for that. For now disabled.
            self.dll.pcap_sendqueue_destroy(self.queue);
            self.queue = self.dll.pcap_sendqueue_alloc(QUEUE_SIZE as c_uint);
            */
        }
    }

    fn data_link(&self) -> DataLink {
        self.datalink
    }

    fn stats(&self) -> Result<Stats, Error> {
        let mut stats: PCapStat = unsafe{uninitialized()};
        if SUCCESS == unsafe{self.dll.pcap_stats(self.handle, &mut stats)}{
            Ok(Stats{
                received: stats.ps_recv as u64,
                dropped: stats.ps_drop as u64 //sp_ifdrop is not yet supported.
            })
        } else {
            Err(self.last_error())
        }
    }
}