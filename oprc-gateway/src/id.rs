use crate::error::ParseIdError;
use base32::{decode, Alphabet};

#[inline]
/// Parses the ID string in the format "<partition_id in base32>:<object_id in base32>"
pub fn parse_id(id_str: &str) -> Result<(u16, u64), ParseIdError> {
    let parts: Vec<&str> = id_str.split(':').collect();

    if parts.len() != 2 {
        return Err(ParseIdError::InvalidFormat);
    }

    // Decode partition_id and object_id from base32
    let partition_id_bytes = decode(Alphabet::Rfc4648 { padding: false }, parts[0])
        .ok_or(ParseIdError::PartitionIdDecodeError)?;
    let object_id_bytes = decode(Alphabet::Rfc4648 { padding: false }, parts[1])
        .ok_or(ParseIdError::ObjectIdDecodeError)?;

    // Validate the length of decoded partition_id and object_id
    if partition_id_bytes.len() != 2 {
        return Err(ParseIdError::InvalidPartitionIdLength);
    }
    if object_id_bytes.len() != 8 {
        return Err(ParseIdError::InvalidObjectIdLength);
    }

    // Convert the decoded byte arrays into u16 and u64 respectively
    let partition_id = u16::from_be_bytes(partition_id_bytes[..2].try_into().unwrap());
    let object_id = u64::from_be_bytes(object_id_bytes[..8].try_into().unwrap());

    Ok((partition_id, object_id))
}

#[inline]
fn to_string(partition_id: u16, object_id: u64) -> String {
    // Convert the partition_id and object_id into byte arrays
    let partition_id_bytes = partition_id.to_be_bytes(); // u16 -> [u8; 2]
    let object_id_bytes = object_id.to_be_bytes(); // u64 -> [u8; 8]

    // Encode the byte arrays into base32 using RFC4648 alphabet without padding
    let partition_id_str =
        base32::encode(Alphabet::Rfc4648 { padding: false }, &partition_id_bytes);
    let object_id_str = base32::encode(Alphabet::Rfc4648 { padding: false }, &object_id_bytes);

    // Return the combined string "<partition_id>:<object_id>"
    format!("{}:{}", partition_id_str, object_id_str)
}

#[test]
fn it_works() {
    let (p, o) = (123, 45646);
    let result = to_string(p, o);
    print!("{}", result);
    let (p2, o2) = parse_id(&result).unwrap();
    assert_eq!(p, p2);
    assert_eq!(o, o2);
}
