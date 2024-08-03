use core::str;
use std::{
    cell::{Cell, RefCell},
    mem::{size_of, MaybeUninit},
};

use windows_sys::Win32::{
    Foundation::{CloseHandle, BOOL, HANDLE, STILL_ACTIVE, WAIT_FAILED, WAIT_TIMEOUT},
    System::{
        ProcessStatus::{K32EnumProcesses, K32GetModuleBaseNameW},
        Threading::{
            GetExitCodeProcess, IsWow64Process, OpenProcess, WaitForSingleObject,
            PROCESS_QUERY_INFORMATION, PROCESS_VM_OPERATION, PROCESS_VM_READ, PROCESS_VM_WRITE,
        },
    },
};

use crate::common::get_string_utf16;

/// A Process instance running on the system
pub struct Process {
    pub(crate) handle: HANDLE,
    name: RefCell<Option<String>>,
    is_64_bit: Cell<Option<bool>>,
}

impl Drop for Process {
    /// Process handles are unmanaged resources the Windows kernel
    /// keeps around in order to allow retuning information about the
    /// process even after it exits. The handle must be freed manually
    /// (or, if not, it will be freed by Windows when the process who
    /// requested it closes). As keeping it around is basically a
    /// (albeit small) leak of resources, we make sure to free it when
    /// dropping Process.
    fn drop(&mut self) {
        unsafe {
            CloseHandle(self.handle);
        }
    }
}

impl Process {
    /// Enumerates processes open in the current system.
    /// For performance reasons, the returned iterator is limited to a maximum size of 1024.
    ///
    /// Documentation: https://learn.microsoft.com/en-us/windows/win32/api/psapi/nf-psapi-enumprocesses
    pub fn get_processes() -> impl DoubleEndedIterator<Item = Process> {
        const ACCESS_TYPE: u32 =
            PROCESS_QUERY_INFORMATION | PROCESS_VM_READ | PROCESS_VM_WRITE | PROCESS_VM_OPERATION;

        const MAX_PROCESSES: usize = 1024;

        unsafe {
            // pid_process will be an array receiving the list of process identifiers
            let mut pid_process =
                MaybeUninit::<[MaybeUninit<u32>; MAX_PROCESSES]>::uninit().assume_init();

            // The number of bytes returned in the array defined in pid_process
            let mut lpcneeded = MaybeUninit::<u32>::uninit();

            let success = K32EnumProcesses(
                pid_process.as_mut_ptr() as *mut u32,
                pid_process.len() as _,
                &mut lpcneeded as *mut _ as *mut u32,
            );

            let no_of_processes = if success != 0 {
                lpcneeded.assume_init().wrapping_div(size_of::<u32>() as _)
            } else {
                0
            };

            (0..no_of_processes as usize).filter_map(move |i| {
                let pid = core::mem::transmute(pid_process[i]);
                let handle = OpenProcess(ACCESS_TYPE, 0, pid);

                match handle {
                    0 => None,
                    _ => Some(Process {
                        handle,
                        name: RefCell::new(None),
                        is_64_bit: Cell::new(None),
                    }),
                }
            })
        }
    }

    /// Returns the name of the process
    pub fn get_name(&self) -> Option<String> {
        let mut name = self.name.borrow_mut();
        if name.is_some() {
            return name.clone();
        }

        let name_bytes = self.name_internal()?;
        let new_name = get_string_utf16(&name_bytes);
        if let Some(n_name) = &new_name {
            let _ = name.insert(n_name.clone());
        }
        new_name
    }

    /// Internal function used to store the name of a process in a fixed-size array.
    /// This is used internally to avoid allocations. If you wish to recover
    /// the name of a process in a more "standard" way, use .get_name()
    fn name_internal(&self) -> Option<[u16; 255]> {
        unsafe {
            let mut base_name = MaybeUninit::<[MaybeUninit<u16>; 255]>::uninit().assume_init();
            let len =
                K32GetModuleBaseNameW(self.handle, 0, &mut base_name as *mut _ as *mut u16, 255);
            match len {
                0 => None,
                _ => {
                    // We know the size
                    base_name[len as usize..].iter_mut().for_each(|val| {
                        val.write(0);
                    });

                    Some(core::mem::transmute(base_name))
                }
            }
        }
    }

    /// Returns an iterator with the Processes matching the name provided
    pub fn get_processes_by_name(name: &str) -> impl Iterator<Item = Process> + '_ {
        let mut name_as_array =
            unsafe { MaybeUninit::<[MaybeUninit<u16>; 255]>::uninit().assume_init() };
        name.encode_utf16().enumerate().for_each(|(i, val)| {
            name_as_array[i].write(val);
        });
        name_as_array[name.chars().count()..]
            .iter_mut()
            .for_each(|val| {
                val.write(0);
            });

        let name = unsafe { core::mem::transmute::<_, [u16; 255]>(name_as_array) };

        Self::get_processes()
            .filter(move |proc| proc.name_internal().is_some_and(|val| val.eq(&name)))
    }

    /// Checks if a process is running under `Wow64`
    pub fn is_64_bit(&self) -> Option<bool> {
        let is_64_bit = self.is_64_bit.get();
        if let Some(_) = is_64_bit {
            return is_64_bit;
        }

        let mut proc_wow64 = MaybeUninit::<BOOL>::uninit();
        unsafe {
            let success = IsWow64Process(self.handle, &mut proc_wow64 as *mut _ as *mut BOOL);
            let is_64_bit = match success {
                0 => false,
                _ => proc_wow64.assume_init() == 0,
            };

            self.is_64_bit.set(Some(is_64_bit));
            Some(is_64_bit)
        }
    }

    /// Checks if the process is currently running
    pub fn is_open(&self) -> Option<bool> {
        unsafe {
            let mut lpexitcode = MaybeUninit::<i32>::uninit();
            let success = GetExitCodeProcess(self.handle, lpexitcode.as_mut_ptr() as *mut _);

            match success {
                0 => None,
                _ => match lpexitcode.assume_init() {
                    STILL_ACTIVE => match WaitForSingleObject(self.handle, 0) {
                        WAIT_FAILED => None,
                        WAIT_TIMEOUT => Some(true),
                        _ => Some(false),
                    },
                    _ => Some(false),
                },
            }
        }
    }
}
