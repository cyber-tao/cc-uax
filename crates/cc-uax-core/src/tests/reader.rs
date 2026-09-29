use crate::reader::Reader;

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

    assert!(err.contains("read of 20 byte(s) at offset 0 crosses the read limit 19"));
}

/// Runs `read` inside a window ending at `limit` over `data`, starting at `start`.
/// Returns the outcome and the position afterwards.
fn read_in_window<T>(
    data: &[u8],
    start: u64,
    limit: u64,
    read: impl FnOnce(&mut Reader) -> anyhow::Result<T>,
) -> (anyhow::Result<T>, u64) {
    let mut r = Reader::new(data);
    r.seek(start).unwrap();
    let result = r.with_limit(limit, read);
    (result, r.pos())
}

fn assert_crosses_limit<T: std::fmt::Debug>(
    (result, pos): (anyhow::Result<T>, u64),
    start: u64,
    limit: u64,
) {
    let err = result.unwrap_err().to_string();
    assert!(err.contains("crosses the read limit"), "{err}");
    assert!(err.contains(&limit.to_string()), "{err}");
    assert_eq!(pos, start, "a refused read must not move the position");
}

#[test]
fn limited_primitive_reads_succeed_up_to_the_limit() {
    let data = [0u8; 64];
    type ReadCase = (&'static str, u64, fn(&mut Reader) -> anyhow::Result<()>);
    let cases: [ReadCase; 5] = [
        ("u8", 1, |r| r.read_u8().map(drop)),
        ("i32", 4, |r| r.read_i32().map(drop)),
        ("u64", 8, |r| r.read_u64().map(drop)),
        ("f64", 8, |r| r.read_f64().map(drop)),
        ("guid", 16, |r| r.read_guid().map(drop)),
    ];
    for (name, size, read) in cases {
        let (ok, pos) = read_in_window(&data, 10, 10 + size, read);
        ok.unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(pos, 10 + size, "{name}");
        assert_crosses_limit(
            read_in_window(&data, 10, 10 + size - 1, read),
            10,
            10 + size - 1,
        );
    }
}

#[test]
fn limited_raw_name_and_bytes_respect_the_limit() {
    let data = [0u8; 64];
    let (ok, pos) = read_in_window(&data, 4, 12, |r| r.read_raw_name());
    ok.unwrap();
    assert_eq!(pos, 12);
    assert_crosses_limit(read_in_window(&data, 4, 11, |r| r.read_raw_name()), 4, 11);

    let (ok, pos) = read_in_window(&data, 4, 9, |r| r.read_bytes(5));
    assert_eq!(ok.unwrap().len(), 5);
    assert_eq!(pos, 9);
    assert_crosses_limit(read_in_window(&data, 4, 8, |r| r.read_bytes(5)), 4, 8);
}

#[test]
fn limited_fstring_reads_respect_the_limit() {
    let mut ansi = 6i32.to_le_bytes().to_vec();
    ansi.extend_from_slice(b"Hello\0");
    let mut utf16 = (-3i32).to_le_bytes().to_vec();
    utf16.extend_from_slice(&[0x48, 0x00, 0x69, 0x00, 0x00, 0x00]);

    let (ok, pos) = read_in_window(&ansi, 0, 10, |r| r.read_fstring());
    assert_eq!(ok.unwrap(), "Hello");
    assert_eq!(pos, 10);
    let (result, pos) = read_in_window(&ansi, 0, 9, |r| r.read_fstring());
    assert!(result.is_err());
    assert_eq!(pos, 4, "only the length prefix was consumed");

    let (ok, pos) = read_in_window(&utf16, 0, 10, |r| r.read_fstring());
    assert_eq!(ok.unwrap(), "Hi");
    assert_eq!(pos, 10);
    let (result, pos) = read_in_window(&utf16, 0, 9, |r| r.read_fstring());
    assert!(result.is_err());
    assert_eq!(pos, 4, "only the length prefix was consumed");

    let (ok, pos) = read_in_window(&ansi, 0, 10, |r| r.read_narrow_string_within(10, "name"));
    assert_eq!(ok.unwrap(), "Hello");
    assert_eq!(pos, 10);
    let (result, pos) = read_in_window(&ansi, 0, 9, |r| r.read_narrow_string_within(10, "name"));
    assert!(result.is_err());
    assert_eq!(pos, 4, "only the length prefix was consumed");
}

#[test]
fn limit_is_restored_after_the_closure_including_errors() {
    let data = [0u8; 16];
    let mut r = Reader::new(&data);
    assert_eq!(r.limit(), 16);

    let ok = r.with_limit(4, |r| r.read_u32());
    assert!(ok.is_ok());
    assert_eq!(r.limit(), 16);

    let failed = r.with_limit(6, |r| r.read_u64());
    assert!(failed.is_err());
    assert_eq!(r.limit(), 16);
    r.read_u64().unwrap();
}

#[test]
fn nested_limit_cannot_widen_the_outer_limit() {
    let data = [0u8; 16];
    let mut r = Reader::new(&data);
    r.with_limit(4, |r| {
        r.with_limit(12, |r| {
            assert_eq!(r.limit(), 4);
            assert!(r.read_u64().is_err());
            r.read_u32().unwrap();
        });
        assert_eq!(r.limit(), 4);
    });
    assert_eq!(r.limit(), 16);
}

#[test]
fn remaining_is_measured_to_the_limit() {
    let data = [0u8; 16];
    let mut r = Reader::new(&data);
    assert_eq!(r.remaining(), 16);
    r.with_limit(10, |r| {
        r.read_u32().unwrap();
        assert_eq!(r.remaining(), 6);
        r.seek(12).unwrap();
        assert_eq!(r.remaining(), 0);
    });
    assert_eq!(r.remaining(), 4);
}

#[test]
fn seek_past_the_limit_succeeds_but_reads_there_fail() {
    let data = [0u8; 16];
    let mut r = Reader::new(&data);
    r.with_limit(8, |r| {
        r.seek(12).unwrap();
        assert_eq!(r.pos(), 12);
        let err = r.read_u8().unwrap_err().to_string();
        assert!(err.contains("crosses the read limit 8"), "{err}");
        assert!(r.seek(17).is_err());
    });
}
