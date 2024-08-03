use std::mem::{self, size_of, MaybeUninit};

use windows_sys::Win32::System::{
    Diagnostics::Debug::WriteProcessMemory,
    Memory::{VirtualAllocEx, VirtualFreeEx, MEM_COMMIT, MEM_RELEASE, PAGE_EXECUTE_READWRITE},
};

use crate::{process::Process, process_memory::Address};

impl Process {
    /// Write any value to the target process' virtual memory space
    pub fn write_value<T: Copy>(&self, address: Address, value: T) -> bool {
        let size = size_of::<T>();
        let mut bytes_written = 0;
        let write = unsafe {
            WriteProcessMemory(
                self.handle,
                address as _,
                &value as *const T as *const _,
                size as _,
                &mut bytes_written,
            )
        };
        write != 0
    }

    /// Write a memory buffer to the target process' virtual memory space
    pub fn write_buf<T: Copy>(&self, address: Address, buf: &[T]) -> bool {
        let mut bytes_written = MaybeUninit::<usize>::uninit();

        unsafe {
            WriteProcessMemory(
                self.handle,
                address as _,
                buf.as_ptr() as _,
                mem::size_of_val(buf),
                bytes_written.as_mut_ptr(),
            ) != 0
        }
    }

    /// Requests allocation of a memory page of at least the size provided.
    /// If successful, it returns the base address of the allocated memory page .
    pub fn allocate_memory(&self, size: usize) -> Option<Address> {
        match unsafe {
            VirtualAllocEx(
                self.handle,
                0 as _,
                size,
                MEM_COMMIT,
                PAGE_EXECUTE_READWRITE,
            )
        } as Address
        {
            0 => None,
            x => Some(x),
        }
    }

    /// Frees a memory page allocation. Returns `true` on success, `false` otherwise.
    pub fn free_memory(&self, address: Address) -> bool {
        unsafe { VirtualFreeEx(self.handle, address as _, 0, MEM_RELEASE) != 0 }
    }
}
