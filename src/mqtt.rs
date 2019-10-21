use simple_error::bail;
use std::error::Error;
use std::fmt;
use std::io::prelude::*;
use std::net::TcpStream;
use std::time;

// http://public.dhe.ibm.com/software/dw/webservices/ws-mqtt/MQTT_V3.1_Protocol_Specific.pdf

#[derive(PartialEq)]
pub enum Message {
    Pingresp,
    Connack,
}

impl fmt::Debug for Message {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Pingresp => write!(f, "PINGRESP"),
            Self::Connack => write!(f, "CONNACK"),
        }
    }
}

pub struct Client {
    client_id: Vec<u8>,
    username: Vec<u8>,
    password: Vec<u8>,
    pub stream: Option<TcpStream>,
    keep_alive_secs: u8,
}

impl Client {
    pub fn new(client_id: &str, username: &str, password: &str, keep_alive_secs: u8) -> Self {
        let client_id_len = client_id.len();
        if client_id_len < 1 || client_id_len > 23 {
            panic!("Client ID must be between 1 and 23 characters in length");
        }

        let username_len = username.len();
        if username_len < 1 || username_len > 12 {
            panic!("Username should be between 1 and 23 characters in length");
        }

        let password_len = password.len();
        if password_len < 1 || password_len > 12 {
            panic!("Password should be between 1 and 23 characters in length");
        }

        Self {
            client_id: String::from(client_id).into_bytes(),
            username: String::from(username).into_bytes(),
            password: String::from(password).into_bytes(),
            stream: None,
            keep_alive_secs,
        }
    }

    fn make_connect(&self) -> Vec<u8> {
        let client_id_len = self.client_id.len() as u8;

        let username_len = self.username.len() as u8;

        let password_len = self.password.len() as u8;

        let len = 20 + client_id_len + username_len + password_len;

        if len > 127 {
            panic!("We don't support sending large messages yet");
        }

        let mut connect_msg = Vec::<u8>::with_capacity(len as usize);
        connect_msg.extend_from_slice(&[
            0x10,    // CONNECT
            len - 2, // message length (-2 for first 2 fixed bytes)
            0,       // protocol name len
            6,       // protocol name len
            b'M',    // protocol name
            b'Q',
            b'I',
            b's',
            b'd',
            b'p',
            3,                    // protocol version
            0xC2,                 // connect flags (username, password, clean session)
            0,                    // keep alive
            self.keep_alive_secs, // keep alive 60 seconds
            0,                    // client ID len
            client_id_len,        // client ID len
        ]);

        connect_msg.extend_from_slice(&self.client_id[..]); // client_id

        // no will topic or will message

        connect_msg.extend_from_slice(&[0, username_len]); // username length
        connect_msg.extend_from_slice(&self.username[..]); // username

        connect_msg.extend_from_slice(&[0, password_len]); // password length
        connect_msg.extend_from_slice(&self.password[..]); // password

        connect_msg
    }

    pub fn connect(&mut self, addr: &str) -> Result<(), Box<dyn Error>> {
        let msg = self.make_connect();

        // TCP init

        let mut stream = TcpStream::connect(addr)?;
        stream.set_read_timeout(Some(time::Duration::from_secs(
            u64::from(self.keep_alive_secs) * 2,
        )))?;
        stream.set_nodelay(true)?;

        // CONNECT

        println!("Connecting...");

        stream.write_all(&msg[..])?;
        stream.flush()?;

        // CONNACK

        let mut buf = [0; 4];
        stream.read_exact(&mut buf)?;
        let connack = parse_message(&buf).unwrap();
        match connack {
            Message::Connack => (),
            _ => bail!(
                "Expected {:?} from server, got {:?}",
                Message::Connack,
                connack
            ),
        };

        println!("Connected!");

        self.stream = Some(stream);

        Ok(())
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

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn short_connect() {
        let client = Client::new("iden", "username", "password", 15);
        assert_eq!(
            client.make_connect(),
            vec![
                16, 38, 0, 6, 77, 81, 73, 115, 100, 112, 3, 194, 0, 15, 0, 4, 105, 100, 101, 110,
                0, 8, 117, 115, 101, 114, 110, 97, 109, 101, 0, 8, 112, 97, 115, 115, 119, 111,
                114, 100
            ]
        );
    }

    #[test]
    fn parse_connack() {
        assert_eq!(Message::Connack, parse_message(&CONNACK).unwrap());

        let mut connack_err = CONNACK;
        connack_err[3] = 1;

        assert!(parse_message(&connack_err).is_err());
    }

    #[test]
    fn parse_pingresp() {
        assert_eq!(Message::Pingresp, parse_message(&PINGRESP).unwrap());

        let pingresp_err = [PINGRESP[0], 1, 0];

        assert!(parse_message(&pingresp_err).is_err());
    }

    #[test]
    fn parse_invalid() {
        assert!(parse_message(&[PINGRESP[0]]).is_err());
        assert!(parse_message(&[0, 1, 1]).is_err());
    }
}
