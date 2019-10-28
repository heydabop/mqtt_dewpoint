pub mod client;
pub mod message;

// http://public.dhe.ibm.com/software/dw/webservices/ws-mqtt/MQTT_V3.1_Protocol_Specific.pdf

type PublishHandler = fn(Vec<u8>) -> Option<Vec<u8>>;

#[allow(clippy::cast_possible_truncation)]
fn encode_length(mut length: usize) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(4);
    while length > 0 {
        let mut byte = length % 128;
        length /= 128;
        if length > 0 {
            byte |= 0x80;
        }
        bytes.push(byte as u8);
    }
    bytes
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn length() {
        assert_eq!(encode_length(16), vec![16]);
        assert_eq!(encode_length(97), vec![97]);
        assert_eq!(encode_length(127), vec![127]);
        assert_eq!(encode_length(128), vec![0x80, 1]);
        assert_eq!(encode_length(321), vec![0xC1, 2]);
        assert_eq!(encode_length(16383), vec![0xFF, 0x7F]);
        assert_eq!(encode_length(16384), vec![0x80, 0x80, 1]);
    }
}
