use core::mem::size_of;
use core::mem::MaybeUninit;

use windows_sys::Win32::System::ProcessStatus::{
    K32GetModuleBaseNameW, K32GetModuleFileNameExW, K32GetModuleInformation, MODULEINFO,
};

use crate::{Address, Process};

pub struct ProcessModule<'a> {
    pub(crate) parent_process: &'a Process,
    pub(crate) module_handle: isize,
}

impl ProcessModule<'_> {
    pub fn name_internal(&self) -> Option<[u16; 255]> {
        unsafe {
            let mut base_name = MaybeUninit::<[MaybeUninit<u16>; 255]>::uninit().assume_init();
            let len = K32GetModuleBaseNameW(
                self.parent_process.handle,
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

    pub fn file_name_internal(&self) -> Option<[u16; 1024]> {
        unsafe {
            let mut base_name = MaybeUninit::<[MaybeUninit<u16>; 1024]>::uninit().assume_init();
            let len = K32GetModuleFileNameExW(
                self.parent_process.handle,
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

    pub fn get_module_info(&self) -> Option<ModuleInfo> {
        let mut module_info = MaybeUninit::<MODULEINFO>::uninit();
        unsafe {
            match K32GetModuleInformation(
                self.parent_process.handle,
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
            }
        }
    }
}

#[derive(Copy, Clone, Debug, Hash)]
pub struct ModuleInfo {
    base_address: Address,
    module_memory_size: u32,
    entry_point_address: Address,
}

impl ModuleInfo {
    pub fn base_address(&self) -> Address {
        self.base_address
    }

    pub fn module_memory_size(&self) -> u32 {
        self.module_memory_size
    }

    pub fn entry_point_address(&self) -> Address {
        self.entry_point_address
    }
}
