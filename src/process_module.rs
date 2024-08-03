use core::cell::RefCell;
use core::mem::size_of;
use core::mem::MaybeUninit;
use std::cell::Cell;

use windows_sys::Win32::Foundation::HINSTANCE;
use windows_sys::Win32::System::ProcessStatus::K32EnumProcessModulesEx;
use windows_sys::Win32::System::ProcessStatus::{
    K32GetModuleBaseNameW, K32GetModuleFileNameExW, K32GetModuleInformation, MODULEINFO,
};

use crate::common::get_string_utf16;
use crate::process::Process;
use crate::process_memory::Address;

impl Process {
    /// Enumerates the modules loaded by the target process
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
            module_handle: unsafe { lphmodule[i].assume_init() },
            name: RefCell::new(None),
            file_name: RefCell::new(None),
            module_info: Cell::new(None),
        })
    }

    /// Returns the main module
    pub fn main_module(&self) -> Option<ProcessModule> {
        self.modules().next()
    }
}

pub struct ProcessModule {
    module_handle: isize,
    name: RefCell<Option<String>>,
    file_name: RefCell<Option<String>>,
    module_info: Cell<Option<ModuleInfo>>,
}

impl ProcessModule {
    /// Recovers the name of the current module. This function caches the recovered value in order to avoid needless system calls.
    pub fn get_name(&self, process: &Process) -> Option<String> {
        let mut cached_name = self.name.borrow_mut();
        if cached_name.is_some() {
            return cached_name.clone();
        }

        let name = get_string_utf16(&self.name_internal(process)?);
        if let Some(name) = &name {
            let _ = cached_name.insert(name.clone());
        }

        name
    }

    fn name_internal(&self, process: &Process) -> Option<[u16; 255]> {
        unsafe {
            let mut base_name = MaybeUninit::<[MaybeUninit<u16>; 255]>::uninit().assume_init();
            let len = K32GetModuleBaseNameW(
                process.handle,
                self.module_handle,
                &mut base_name as *mut _ as *mut u16,
                255,
            );
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

    pub fn get_file_name(&self, process: &Process) -> Option<String> {
        let mut file_name = self.file_name.borrow_mut();
        if file_name.is_some() {
            return file_name.clone();
        }

        let name = get_string_utf16(&self.file_name_internal(process)?);
        if let Some(name) = &name {
            let _ = file_name.insert(name.clone());
        }

        name
    }

    fn file_name_internal(&self, process: &Process) -> Option<[u16; 1024]> {
        unsafe {
            let mut base_name = MaybeUninit::<[MaybeUninit<u16>; 1024]>::uninit().assume_init();
            let len = K32GetModuleFileNameExW(
                process.handle,
                self.module_handle,
                &mut base_name as *mut _ as *mut u16,
                255,
            );
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

    pub fn get_base_address(&self, process: &Process) -> Option<Address> {
        Some(self.get_module_info(process)?.base_address)
    }

    pub fn get_entry_point_address(&self, process: &Process) -> Option<Address> {
        Some(self.get_module_info(process)?.entry_point_address)
    }

    pub fn get_module_size(&self, process: &Process) -> Option<u32> {
        Some(self.get_module_info(process)?.module_memory_size)
    }

    fn get_module_info(&self, process: &Process) -> Option<ModuleInfo> {
        let module_info_cached = self.module_info.get();
        if module_info_cached.is_some() {
            return module_info_cached;
        }

        let mut module_info = MaybeUninit::<MODULEINFO>::uninit();
        unsafe {
            let info = match K32GetModuleInformation(
                process.handle,
                self.module_handle,
                module_info.as_mut_ptr() as *mut _,
                size_of::<MODULEINFO>() as _,
            ) {
                0 => None,
                _ => {
                    let module = module_info.assume_init();
                    Some(ModuleInfo {
                        base_address: module.lpBaseOfDll as _,
                        module_memory_size: module.SizeOfImage,
                        entry_point_address: module.EntryPoint as _,
                    })
                }
            }?;

            self.module_info.set(Some(info));
            Some(info)
        }
    }
}

#[repr(C)]
#[derive(Copy, Clone, Debug, Hash)]
pub struct ModuleInfo {
    base_address: Address,
    module_memory_size: u32,
    entry_point_address: Address,
}
