use simple_error::bail;
use std::error::Error;
use std::fmt;

// http://public.dhe.ibm.com/software/dw/webservices/ws-mqtt/MQTT_V3.1_Protocol_Specific.pdf

pub enum Message {
    Pingresp,
    Connack,
    Publish(Vec<u8>),
}

impl fmt::Debug for Message {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Message::Pingresp => write!(f, "PINGRESP"),
            Message::Connack => write!(f, "CONNACK"),
            Message::Publish(_) => write!(f, "PUBLISH"),
        }
    }
}

pub const CONNACK: [u8; 4] = [0x20, 2, 0, 0];
pub const PINGREQ: [u8; 2] = [0xC0, 0];
pub const PINGRESP: [u8; 2] = [0xD0, 0];

pub fn parse_message(v: &[u8]) -> Result<Message, Box<dyn Error>> {
    if v.len() < 2 {
        bail!("Message too short to be valid");
    }
    let content = &v[..(v[1] + 2) as usize]; // trim buffer down to specified length

    match content[0] >> 4 {
        2 => {
            if content != CONNACK {
                bail!(
                    "Error in CONNACK, expected [32, 2, 0, 0], got {:?}",
                    &content
                );
            }
            Ok(Message::Connack)
        }
        13 => {
            if content != PINGRESP {
                bail!("Error in PINGRESP, expected [12, 0], got {:?}", &content);
            }
            Ok(Message::Pingresp)
        }
        _ => bail!(
            "Unrecognized message type {} in message {:?}",
            &content[0] >> 4,
            &content
        ),
    }
}

pub fn make_connect(client_id: &str, username: &str, password: &str) -> Vec<u8> {
    let len = 20 + client_id.len() + username.len() + password.len();

    if len > 127 {
        panic!("We don't support sending large messages yet");
    }

    let mut connect_msg = Vec::<u8>::with_capacity(len);
    connect_msg.push(0x10); // CONNECT
    connect_msg.push((len - 2) as u8); // message length (-2 for first 2 fixed bytes)
    connect_msg.push(0); // protocol name len
    connect_msg.push(6); // protocol name len
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
