use cc_uax::reader::Reader;

#[test]
fn fstring_ansi() {
    let mut data = 6i32.to_le_bytes().to_vec();

    data.extend_from_slice(b"Hello\0");

    let mut r = Reader::new(&data);

    assert_eq!(r.read_fstring().unwrap(), "Hello");
}

#[test]
fn fstring_empty() {
    let data = 0i32.to_le_bytes();

    let mut r = Reader::new(&data);

    assert_eq!(r.read_fstring().unwrap(), "");
}

#[test]
fn fstring_utf16() {
    let mut data = (-3i32).to_le_bytes().to_vec();

    data.extend_from_slice(&[0x48, 0x00, 0x69, 0x00, 0x00, 0x00]);

    let mut r = Reader::new(&data);

    assert_eq!(r.read_fstring().unwrap(), "Hi");
}

#[test]
fn read_integers_le() {
    let data = [0x01, 0x00, 0x00, 0x00, 0xff, 0xff, 0xff, 0xff];

    let mut r = Reader::new(&data);

    assert_eq!(r.read_i32().unwrap(), 1);

    assert_eq!(r.read_i32().unwrap(), -1);
}

#[test]
fn read_raw_name() {
    let data = [0x05, 0x00, 0x00, 0x00, 0x02, 0x00, 0x00, 0x00];

    let mut r = Reader::new(&data);

    let n = r.read_raw_name().unwrap();

    assert_eq!(n.index, 5);

    assert_eq!(n.number, 2);
}

#[test]
fn read_io_hash_rejects_short_input() {
    let data = [0u8; 19];

    let mut r = Reader::new(&data);

    let err = r.read_io_hash().err().unwrap().to_string();

    assert!(err.contains("read 20 bytes out of range"));
}
