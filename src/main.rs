use core::{
    mem::{size_of, MaybeUninit},
    slice,
};
use process_module::ProcessModule;
use windows_sys::Win32::{
    Foundation::{CloseHandle, BOOL, HANDLE, HINSTANCE, STILL_ACTIVE, WAIT_FAILED, WAIT_TIMEOUT},
    System::{
        Diagnostics::Debug::{ReadProcessMemory, WriteProcessMemory},
        Memory::{VirtualAllocEx, VirtualFreeEx, MEM_COMMIT, MEM_RELEASE, PAGE_EXECUTE_READWRITE},
        ProcessStatus::{K32EnumProcessModulesEx, K32EnumProcesses, K32GetModuleBaseNameW},
        Threading::{
            GetExitCodeProcess, IsWow64Process, OpenProcess, WaitForSingleObject,
            PROCESS_ALL_ACCESS,
        },
    },
};

#[cfg(feature = "alloc")]
pub mod alloc;

pub mod process_module;
type Address = isize;

pub struct Process {
    handle: HANDLE,
}

impl Drop for Process {
    fn drop(&mut self) {
        unsafe {
            CloseHandle(self.handle);
        }
    }
}

impl Process {
    /// Enumerates all processes
    ///
    /// Documentation: https://learn.microsoft.com/en-us/windows/win32/api/psapi/nf-psapi-enumprocesses
    pub fn get_processes() -> impl DoubleEndedIterator<Item = Process> {
        const ACCESS_TYPE: u32 = PROCESS_ALL_ACCESS;
        const MAX_PROCESSES: usize = 1024;

        unsafe {
            let mut pid_process =
                MaybeUninit::<[MaybeUninit<u32>; MAX_PROCESSES]>::uninit().assume_init();
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
                    _ => Some(Process { handle }),
                }
            })
        }
    }

    /// Internal function used to store the name of a process in a fixed-size array.
    /// This is used internally to avoid allocations. If you wish to recover
    /// the name of a process in a more "standard" way, use .get_name()
    pub fn name_internal(&self) -> Option<[u16; 255]> {
        unsafe {
            let mut base_name = MaybeUninit::<[MaybeUninit<u16>; 255]>::uninit().assume_init();
            let len =
                K32GetModuleBaseNameW(self.handle, 0, &mut base_name as *mut _ as *mut u16, 255);
            match len {
                0 => None,
                _ => {
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

    /// Checks if a process is running under Wow64
    pub fn is_64_bit(&self) -> Option<bool> {
        let mut proc_wow64 = MaybeUninit::<BOOL>::uninit();
        unsafe {
            let success = IsWow64Process(self.handle, &mut proc_wow64 as *mut _ as *mut BOOL);
            match success {
                0 => None,
                _ => Some(proc_wow64.assume_init() == 0),
            }
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

    /// Reads a certain data stream into the specified buffer
    pub fn read_bytes(&self, address: Address, buf: &mut [u8]) -> bool {
        let mut bytes_read = MaybeUninit::<usize>::uninit();

        unsafe {
            ReadProcessMemory(
                self.handle,
                address as _,
                buf.as_mut_ptr() as *mut _,
                buf.len() as _,
                bytes_read.as_mut_ptr() as *mut usize,
            ) != 0
        }
    }

    /// Reads any value from the target process' virtual memory space
    pub fn read_value<T: Copy>(&self, address: Address) -> Option<T> {
        let mut buf = MaybeUninit::<T>::uninit();
        match unsafe {
            self.read_bytes(
                address,
                slice::from_raw_parts_mut(buf.as_mut_ptr() as *mut u8, size_of::<T>()),
            )
        } {
            true => Some(unsafe { buf.assume_init() }),
            false => None,
        }
    }

    pub fn read_pointer(&self, address: Address) -> Option<Address> {
        match self.is_64_bit() {
            Some(true) => self.read_value::<u64>(address).map(|val| val as Address),
            Some(false) => self.read_value::<u32>(address).map(|val| val as Address),
            None => None,
        }
    }

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

    pub fn write_bytes(&self, address: Address, buf: &[u8]) -> bool {
        let mut bytes_written = MaybeUninit::<usize>::uninit();

        unsafe {
            WriteProcessMemory(
                self.handle,
                address as _,
                buf.as_ptr() as _,
                buf.len() as _,
                bytes_written.as_mut_ptr(),
            ) != 0
        }
    }

    pub fn modules(&self) -> impl DoubleEndedIterator<Item = ProcessModule> + '_ {
        let mut lphmodule =
            unsafe { MaybeUninit::<[MaybeUninit<HINSTANCE>; 1024]>::uninit().assume_init() };
        let mut lpcneeded = MaybeUninit::<u32>::uninit();

        let success = unsafe {
            K32EnumProcessModulesEx(
                self.handle,
                lphmodule.as_mut_ptr() as *mut _,
                size_of::<HINSTANCE>().saturating_mul(1024) as _,
                lpcneeded.as_mut_ptr(),
                0x03,
            )
        };

        let number_of_modules = match success {
            0 => 0,
            _ => unsafe {
                lpcneeded
                    .assume_init()
                    .saturating_div(size_of::<HINSTANCE>() as _)
            },
        };

        (0..number_of_modules as usize).map(move |i| ProcessModule {
            parent_process: self,
            module_handle: unsafe { lphmodule[i].assume_init() },
        })
    }

    pub fn main_module(&self) -> Option<ProcessModule> {
        self.modules().next()
    }

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

    pub fn free_memory(&self, address: Address) -> bool {
        unsafe { VirtualFreeEx(self.handle, address as _, 0, MEM_RELEASE) != 0 }
    }
}
