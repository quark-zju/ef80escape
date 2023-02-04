#![no_main]

use libfuzzer_sys::fuzz_target;
use ef80escape::bytes_to_str;
use ef80escape::str_to_bytes;

fuzz_target!(|data: &[u8]| {
    let s = bytes_to_str(data);
    let d = str_to_bytes(&s);
    assert_eq!(data, d.as_ref(), "str: {:?}", s.as_bytes());
});
