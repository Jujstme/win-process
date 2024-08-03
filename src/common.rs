use core::str;

pub(crate) fn get_string_utf8(buf: &[u8]) -> Option<String> {
    let null_terminator = buf.iter().position(|val| val.eq(&0)).unwrap_or(buf.len());
    let nuf = &buf[..null_terminator];
    str::from_utf8(nuf).map(|val| val.to_string()).ok()
}

pub(crate) fn get_string_utf16(buf: &[u16]) -> Option<String> {
    let null_terminator = buf.iter().position(|val| val.eq(&0)).unwrap_or(buf.len());
    let nuf = &buf[..null_terminator];
    String::from_utf16(nuf).ok()
}
