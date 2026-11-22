use super::*;

#[test]
fn parse_accepts_sixteen_lowercase_hex_characters() {
    let id = ReadId::parse("0123456789abcdef").expect("valid id");
    assert_eq!(id.as_str(), "0123456789abcdef");
}

#[test]
fn parse_refuses_anything_else() {
    for bad in [
        "",
        "0123456789abcde",
        "0123456789abcdef0",
        "0123456789ABCDEF",
        "0123456789abcdeg",
        "invalid",
        " 0123456789abcde",
        "０123456789abcde",
    ] {
        assert_eq!(ReadId::parse(bad), None, "{bad:?}");
    }
}

#[test]
fn a_new_id_parses_back_to_itself() {
    let id = ReadId::new();
    assert_eq!(ReadId::parse(id.as_str()), Some(id));
}

#[test]
fn from_str_and_try_from_follow_parse() {
    assert_eq!(
        "0123456789abcdef".parse::<ReadId>(),
        Ok(ReadId::parse("0123456789abcdef").unwrap())
    );
    assert_eq!("XYZ".parse::<ReadId>(), Err(InvalidReadId));
    assert_eq!(ReadId::try_from("XYZ"), Err(InvalidReadId));
    assert!(ReadId::try_from("fedcba9876543210").is_ok());
}
