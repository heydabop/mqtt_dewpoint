// http://public.dhe.ibm.com/software/dw/webservices/ws-mqtt/MQTT_V3.1_Protocol_Specific.pdf

pub const PINGREQ: [u8; 2] = [0xC0, 0];
pub const PINGRESP: [u8; 2] = [0xD0, 0];

pub fn make_connect(client_id: &str, username: &str, password: &str) -> Vec<u8> {
    let mut connect_msg: Vec<u8> = vec![0x10, 0, 0, 6]; // set remaining length (byte 2) after rest of message is set
    for b in "MQIsdp".chars() {
        //protocol name
        connect_msg.push(b as u8);
    }
    connect_msg.push(3); // protocol version
    connect_msg.push(0xC2); // connect flags (username, password, clean session)
    connect_msg.push(0); // keep alive
    connect_msg.push(15); // keep alive 15 seconds

    connect_msg.push(0); // client ID len
    connect_msg.push(client_id.len() as u8); // client ID len
    for b in client_id.chars() {
        // client ID
        connect_msg.push(b as u8);
    }
    // no will topic or will message

    connect_msg.push(0); // username length
    connect_msg.push(username.len() as u8); // username length
    for b in username.chars() {
        // username
        connect_msg.push(b as u8);
    }

    connect_msg.push(0); // password length
    connect_msg.push(password.len() as u8); // passowrd length
    for b in password.chars() {
        // password
        connect_msg.push(b as u8);
    }

    if connect_msg.len() > 127 {
        panic!("We don't support sending large messages yet");
    }
    connect_msg[1] = (connect_msg.len() - 2) as u8; // set len now that we know it

    connect_msg
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn short_connect() {
        assert_eq!(
            make_connect("iden", "username", "password"),
            vec![
                16, 38, 0, 6, 77, 81, 73, 115, 100, 112, 3, 194, 0, 15, 0, 4, 105, 100, 101, 110,
                0, 8, 117, 115, 101, 114, 110, 97, 109, 101, 0, 8, 112, 97, 115, 115, 119, 111,
                114, 100
            ]
        );
    }
}
