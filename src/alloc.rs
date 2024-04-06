use crate::{process_module::ProcessModule, Address, Process};

impl Process {
    /// Recovers the name of the target process
    pub fn get_name(&self) -> Option<String> {
        let name_bytes = self.name_internal()?;
        String::from_utf16(&name_bytes[..name_bytes.iter().position(|val| val.eq(&0))?]).ok()
    }

    pub fn read_string<const N: usize>(&self, address: Address) -> Option<String> {
        let buf = self.read_value::<[u8; N]>(address)?;
        let null_terminator = buf.iter().position(|val| val.eq(&0));

        let buf = match null_terminator {
            Some(x) => &buf[..x],
            None => &buf,
        };

        match String::from_utf8(buf.to_vec()) {
            Ok(val) => Some(val),
            _ => {
                let buf = self.read_value::<[u16; N]>(address)?;
                let null_terminator = buf.iter().position(|val| val.eq(&0));

                let buf = match null_terminator {
                    Some(x) => &buf[..x],
                    None => &buf,
                };

                match String::from_utf16(buf) {
                    Ok(val) => Some(val),
                    _ => None,
                }
            }
        }
    }
}

impl ProcessModule<'_> {
    /// Recovers the name of the module
    pub fn get_name(&self) -> Option<String> {
        let name_bytes = self.name_internal()?;
        String::from_utf16(&name_bytes[..name_bytes.iter().position(|val| val.eq(&0))?]).ok()
    }

    pub fn get_file_name(&self) -> Option<String> {
        let name_bytes = self.file_name_internal()?;
        String::from_utf16(&name_bytes[..name_bytes.iter().position(|val| val.eq(&0))?]).ok()
    }
}
