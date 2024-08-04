use windows_sys::Win32::System::Diagnostics::Debug::ReadProcessMemory;

use crate::{
    common::{get_string_utf16, get_string_utf8},
    process::Process,
};
use core::slice;
use std::mem::{self, MaybeUninit};

pub type Address = isize;

impl Process {
    /// Reads a certain data stream into the specified buffer
    pub fn read_buf<T: Copy>(&self, address: Address, buf: &mut [T]) -> bool {
        let mut bytes_read = MaybeUninit::<usize>::uninit();

        unsafe {
            ReadProcessMemory(
                self.handle,
                address as _,
                buf.as_mut_ptr() as *mut _,
                mem::size_of_val(buf),
                bytes_read.as_mut_ptr() as *mut usize,
            ) != 0
        }
    }

    /// Reads a range of bytes from the process at the address given into the
    /// buffer provided. The buffer does not need to be initialized. After the
    /// buffer successfully got filled, the initialized buffer is returned.
    pub fn read_into_uninit_buf<'buf, T: Copy>(
        &self,
        address: Address,
        buf: &'buf mut [MaybeUninit<T>],
    ) -> Option<&'buf mut [T]> {
        let mut bytes_read = MaybeUninit::<usize>::uninit();

        let success;
        unsafe {
            success = ReadProcessMemory(
                self.handle,
                address as _,
                buf.as_mut_ptr() as *mut _,
                mem::size_of_val(buf),
                bytes_read.as_mut_ptr(),
            ) != 0;

            if success {
                Some(slice::from_raw_parts_mut(
                    buf.as_mut_ptr().cast(),
                    buf.len(),
                ))
            } else {
                None
            }
        }
    }

    /// Reads any value from the target process' virtual memory space
    pub fn read_value<T: Copy>(&self, address: Address) -> Option<T> {
        let mut buf = MaybeUninit::<T>::uninit();
        match unsafe { self.read_buf::<T>(address, slice::from_raw_parts_mut(buf.as_mut_ptr(), 1)) }
        {
            true => Some(unsafe { buf.assume_init() }),
            false => None,
        }
    }

    /// Reads a value of a size of a pointer in the target process virtual memory space
    pub fn read_pointer(&self, address: Address) -> Option<Address> {
        match self.is_64_bit() {
            Some(true) => self.read_value::<u64>(address).map(|val| val as Address),
            Some(false) => self.read_value::<u32>(address).map(|val| val as Address),
            _ => None,
        }
    }

    /// Resolves a pointer path, returning the memory address at the end of the path
    pub fn deref_offsets(&self, address: Address, offsets: &[u32]) -> Option<Address> {
        let mut address = self.read_pointer(address)?;

        if let Some((&last, path)) = offsets.split_last() {
            for &val in path {
                address = self.read_pointer(address + val as isize)?;
            }

            address = address + last as isize;
        }

        Some(address)
    }

    /// Reads a string from the target process' memory space
    pub fn read_string<const N: usize>(
        &self,
        address: Address,
        string_type: StringType,
    ) -> Option<String> {
        match string_type {
            StringType::UTF8 => self.read_string_utf8::<N>(address),
            StringType::UTF16 => self.read_string_utf16::<N>(address),
            StringType::Auto => self.read_string_auto::<N>(address),
        }
    }

    pub fn read_string_utf8<const N: usize>(&self, address: Address) -> Option<String> {
        let mut buf = unsafe { MaybeUninit::<[MaybeUninit<u8>; N]>::uninit().assume_init() };
        let buf = self.read_into_uninit_buf(address, &mut buf)?;
        get_string_utf8(buf)
    }

    pub fn read_string_utf16<const N: usize>(&self, address: Address) -> Option<String> {
        let mut buf = unsafe { MaybeUninit::<[MaybeUninit<u16>; N]>::uninit().assume_init() };
        let buf = self.read_into_uninit_buf(address, &mut buf)?;
        get_string_utf16(buf)
    }

    pub fn read_string_auto<const N: usize>(&self, address: Address) -> Option<String> {
        let mut buf = unsafe { MaybeUninit::<[MaybeUninit<u16>; N]>::uninit().assume_init() };
        let buf16 = self.read_into_uninit_buf(address, &mut buf)?;
        let buf8 = unsafe { slice::from_raw_parts(buf16.as_ptr() as *const u8, buf16.len()) };

        let is_utf_16 = if let [_, second, _, fourth, ..] = buf8 {
            matches!(second, &0) && matches!(fourth, &0)
        } else {
            false
        };
        
        match is_utf_16 {
            true => get_string_utf16(buf16),
            false => get_string_utf8(buf8),
        }
    }
}

#[derive(Copy, Clone, Debug, Hash)]
pub enum StringType {
    UTF8,
    UTF16,
    Auto,
}
