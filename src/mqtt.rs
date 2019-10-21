use simple_error::bail;
use std::collections::HashMap;
use std::error::Error;
use std::fmt;
use std::io::prelude::*;
use std::net::TcpStream;
use std::sync::{mpsc, Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time;

// http://public.dhe.ibm.com/software/dw/webservices/ws-mqtt/MQTT_V3.1_Protocol_Specific.pdf

#[derive(PartialEq)]
pub enum Message {
    Pingresp,
    Connack,
    Publish {
        id: Vec<u8>,
        topic: String,
        qos: u8,
        payload: Vec<u8>,
    },
    Suback(Vec<u8>),
}

impl fmt::Debug for Message {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Pingresp => write!(f, "PINGRESP"),
            Self::Connack => write!(f, "CONNACK"),
            Self::Publish { topic, .. } => write!(f, "PUBLISH {}", topic),
            Self::Suback(msg) => write!(f, "SUBACK {}", msg[3]),
        }
    }
}

type PublishHandler = fn(Vec<u8>) -> ();

#[allow(dead_code)]
struct ConnectedClient {
    stream: TcpStream,
    tx: mpsc::Sender<Vec<u8>>,
    o_stream_thread: JoinHandle<()>,
    i_stream_thread: JoinHandle<()>,
    ping_thread: JoinHandle<()>,
}

pub struct Client {
    client_id: Vec<u8>,
    username: Vec<u8>,
    password: Vec<u8>,
    connected: Option<ConnectedClient>,
    keep_alive_secs: u8,
    pending_subscribe_ids: Arc<Mutex<Vec<u8>>>,
    next_message_id: u8,
    publish_functions: Arc<Mutex<HashMap<String, PublishHandler>>>,
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
            connected: None,
            keep_alive_secs,
            pending_subscribe_ids: Arc::new(Mutex::new(Vec::new())),
            next_message_id: 1,
            publish_functions: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    #[allow(clippy::cast_possible_truncation)]
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

    #[allow(clippy::cast_possible_truncation)]
    fn make_subscribe(&mut self, topic: &str) -> Vec<u8> {
        let topic_len = topic.len();
        if topic_len > 127 {
            panic!("Topic length too long");
        }
        let len = topic_len + 5; // 2 bytes for variable header, 2 bytes for topic len, topic, 1 byte for QoS

        let mut subscribe_msg = Vec::<u8>::with_capacity(len + 2); // 2 bytes for fixed header
        subscribe_msg.extend_from_slice(&[
            0x82, // 8 - SUBSCRIBE, 2 - QoS 1
            len as u8,
            0,                    // message ID
            self.next_message_id, // message ID
            0,                    // topic length
            topic_len as u8,      // topic length
        ]);

        subscribe_msg.extend_from_slice(&topic.bytes().collect::<Vec<u8>>());
        subscribe_msg.push(1); // QoS 1

        self.next_message_id += 1;

        subscribe_msg
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

        let mut o_stream = stream.try_clone()?;

        let mut i_stream = stream.try_clone()?;

        let (tx, o_rx): (mpsc::Sender<Vec<u8>>, mpsc::Receiver<Vec<u8>>) = mpsc::channel();
        let o_tx = tx.clone();
        let i_tx = tx.clone();

        let o_stream_thread = thread::spawn(move || {
            while let Ok(msg) = o_rx.recv() {
                o_stream.write_all(&msg[..]).unwrap();
                o_stream.flush().unwrap();
            }
        });

        let pending_subscribe_ids = Arc::clone(&self.pending_subscribe_ids);
        let publish_functions = Arc::clone(&self.publish_functions);

        let i_stream_thread = thread::spawn(move || loop {
            let mut buf = [0; 127];
            if i_stream.read(&mut buf[..2]).unwrap() == 0 {
                break;
            }
            let len = (buf[1] + 2) as usize;
            if len > 0 {
                i_stream.read_exact(&mut buf[2..len]).unwrap();
            }
            match parse_message(&buf[..len]) {
                Ok(message) => match message {
                    Message::Pingresp => println!("Pinged"),
                    Message::Suback(msg) => {
                        let mut pending_subscribe_ids = pending_subscribe_ids
                            .lock()
                            .expect("Error locking on subscribe IDs");
                        handle_suback(&msg, &mut pending_subscribe_ids);
                    }
                    Message::Publish {
                        id,
                        topic,
                        qos,
                        payload,
                    } => {
                        let publish_functions = publish_functions
                            .lock()
                            .expect("Error locking on publish functions");
                        let handler = publish_functions.get(&topic);
                        match handle_publish(&id, &topic, qos, payload, handler) {
                            Ok(puback) => {
                                i_tx.send(puback).unwrap();
                            }
                            Err(err) => {
                                eprintln!("Error handling puback: {}", err);
                            }
                        }
                    }
                    _ => eprintln!("Unexpected message type: {:?}", message),
                },
                Err(e) => eprintln!("Error parsing message: {}", e),
            };
        });

        let ping_tx = o_tx.clone();
        let keep_alive_secs = self.keep_alive_secs;
        let ping_thread = thread::spawn(move || {
            let interval = time::Duration::from_secs(u64::from(keep_alive_secs) / 2);
            loop {
                println!("Pinging...");
                ping_tx.send(PINGREQ.to_vec()).unwrap();
                thread::sleep(interval);
            }
        });

        self.connected = Some(ConnectedClient {
            stream,
            tx,
            o_stream_thread,
            i_stream_thread,
            ping_thread,
        });

        Ok(())
    }

    pub fn subscribe(&mut self, topic: &str, f: PublishHandler) -> Result<(), Box<dyn Error>> {
        let sub_msg = self.make_subscribe(topic);

        println!("Subscribing...");

        self.publish_functions
            .lock()
            .expect("Error locking on publish functions")
            .insert(String::from(topic), f);

        let tx = match self.connected.as_ref() {
            Some(c) => &c.tx,
            None => bail!("Client not connected"),
        };

        self.pending_subscribe_ids
            .lock()
            .expect("Error locking on pending subscribe IDs")
            .push(sub_msg[3]);

        tx.send(sub_msg)?;

        Ok(())
    }

    pub fn disconnect(&self) {
        println!("Disconnecting...");
        self.connected
            .as_ref()
            .expect("Attempt to disconnect while not connected")
            .tx
            .send(DISCONNECT.to_vec())
            .unwrap();
    }
}

pub const CONNACK: [u8; 4] = [0x20, 2, 0, 0];
pub const DISCONNECT: [u8; 2] = [0xE0, 0];
pub const PINGREQ: [u8; 2] = [0xC0, 0];
pub const PINGRESP: [u8; 2] = [0xD0, 0];

fn parse_message(v: &[u8]) -> Result<Message, Box<dyn Error>> {
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
        3 => parse_publish(v),
        9 => Ok(Message::Suback(v.to_vec())),
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

fn parse_publish(publish: &[u8]) -> Result<Message, Box<dyn Error>> {
    let qos = match publish[0] & 6 {
        0 => 0,
        2 => 1,
        4 => bail!("Can't handle PUBLISH QoS 2"),
        _ => bail!("Unexpected QoS value {}", publish[0] & 0x0F),
    };
    let len = publish[1];
    if len >= 127 {
        bail!("Can't handle publish lengths of 127 or greater");
    }
    if len == 0 {
        bail!("Empty publish");
    }
    let topic_len = ((u16::from(publish[2]) << 8) + u16::from(publish[3])) as usize;
    let topic = String::from_utf8(publish[4..topic_len + 4].to_vec())?;

    let mut payload_offset = topic_len + 4;
    let mut id = Vec::new();
    if qos == 1 {
        payload_offset += 2; // message ID after topic
        id.extend_from_slice(&publish[topic_len + 4..topic_len + 6]);
    }
    let payload = publish[payload_offset..].to_vec();

    Ok(Message::Publish {
        id,
        topic,
        qos,
        payload,
    })
}

fn handle_suback(suback: &[u8], pending_subscribe_ids: &mut Vec<u8>) {
    println!("Suback {}", suback[3]);
    if let Some(pos) = pending_subscribe_ids.iter().position(|&x| x == suback[3]) {
        pending_subscribe_ids.remove(pos);
    } else {
        eprintln!("Received suback for unknown ID {}", suback[3]);
    }
}

fn handle_publish(
    id: &[u8],
    topic: &str,
    qos: u8,
    payload: Vec<u8>,
    f: Option<&PublishHandler>,
) -> Result<Vec<u8>, Box<dyn Error>> {
    println!("Publish topic {}", topic);

    if let Some(f) = f {
        f(payload);
    }

    if qos == 1 {
        return Ok(make_puback(&id));
    }
    Ok(vec![])
}

fn make_puback(msg_id: &[u8]) -> Vec<u8> {
    vec![0x40, 2, msg_id[0], msg_id[1]]
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
    fn short_subscribe() {
        let mut client = Client::new("iden", "username", "password", 15);
        assert_eq!(
            client.make_subscribe("test/topic"),
            vec![130, 15, 0, 1, 0, 10, 116, 101, 115, 116, 47, 116, 111, 112, 105, 99, 1]
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

    #[test]
    fn puback_gen() {
        assert_eq!(make_puback(&[0, 27]), vec![0x40, 2, 0, 27]);
        assert_eq!(make_puback(&[12, 14]), vec![0x40, 2, 12, 14]);
    }

    #[test]
    fn publish() {
        match parse_publish(&[0x32, 7, 0, 3, b'a', b'/', b'b', 0, 27]).unwrap() {
            Message::Publish {
                id,
                topic,
                qos,
                payload,
            } => assert_eq!(
                handle_publish(&id, &topic, qos, payload, None).unwrap(),
                vec![0x40, 2, 0, 27]
            ),
            _ => panic!("Received non-publish from parse"),
        };

        match parse_publish(&[0x30, 7, 0, 3, b'a', b'/', b'b', 0, 27]).unwrap() {
            Message::Publish {
                id,
                topic,
                qos,
                payload,
            } => assert_eq!(
                handle_publish(&id, &topic, qos, payload, None).unwrap(),
                Vec::<u8>::new()
            ),
            _ => panic!("Received non-publish from parse"),
        };

        assert!(parse_publish(&[0x34, 7, 0, 3, b'a', b'/', b'b', 0, 27]).is_err())
    }
}
