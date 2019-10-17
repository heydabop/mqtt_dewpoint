use std::io::prelude::*;
use std::net::TcpStream;
use std::sync::mpsc;
use std::thread;
use std::time;

mod mqtt;

fn main() -> std::io::Result<()> {
    // config init
    let filename: &str;

    let args: Vec<String> = std::env::args().collect();
    if args.len() >= 2 {
        filename = &args[1];
    } else {
        filename = "config.toml";
    }

    let config = std::fs::read_to_string(filename)?
        .parse::<toml::Value>()
        .unwrap();

    let broker_addr = config["broker_addr"].as_str().unwrap();

    let client_id = config["client_id"].as_str().unwrap();
    if client_id.len() > 0xFF {
        panic!("Client ID too long");
    }

    let username = config["username"].as_str().unwrap();
    if username.len() > 0xFF {
        panic!("Username too long");
    }

    let password = config["password"].as_str().unwrap();
    if password.len() > 0xFF {
        panic!("Password too long");
    }

    // TCP init

    let mut stream = TcpStream::connect(broker_addr)?;
    stream.set_read_timeout(None)?;
    stream.set_nodelay(true)?;

    let mut ostream = stream.try_clone()?;

    let (o_tx, o_rx): (mpsc::Sender<Vec<u8>>, mpsc::Receiver<Vec<u8>>) = mpsc::channel();
    let (i_tx, i_rx): (mpsc::Sender<mqtt::Message>, mpsc::Receiver<mqtt::Message>) =
        mpsc::channel();

    let ostream_thread = thread::spawn(move || loop {
        match o_rx.recv() {
            Ok(msg) => {
                ostream.write_all(&msg[..]).unwrap();
                ostream.flush().unwrap();
            }
            Err(_) => break,
        };
    });

    let istream_thread = thread::spawn(move || loop {
        let mut buf = [0; 127];
        if stream.read(&mut buf[..2]).unwrap() == 0 {
            break;
        }
        let len = (buf[1] + 2) as usize;
        if len > 0 {
            stream.read(&mut buf[2..len]).unwrap();
        }
        match mqtt::parse_message(&buf[..len]) {
            Ok(message) => i_tx.send(message).unwrap(),
            Err(e) => eprintln!("Error parsing message: {}", e),
        };
    });

    // MQTT CONNECT

    println!("Connecting...");

    o_tx.send(mqtt::make_connect(client_id, username, password))
        .unwrap();

    // MQTT CONNACK

    let connack = i_rx.recv().unwrap();
    match connack {
        mqtt::Message::Connack => (),
        _ => panic!("Expected {:?}, got {:?}", mqtt::Message::Connack, connack),
    }

    println!("Connected!");

    let five_sec = time::Duration::from_secs(5);
    thread::sleep(five_sec);

    // MQTT PINGREQ

    println!("Pinging...");

    o_tx.send(mqtt::PINGREQ.to_vec()).unwrap();

    let five_sec = time::Duration::from_secs(5);
    thread::sleep(five_sec);

    // MQTT PINGRESP

    let pingresp = i_rx.recv().unwrap();
    match pingresp {
        mqtt::Message::Pingresp => (),
        _ => panic!("Expected {:?}, got {:?}", mqtt::Message::Pingresp, pingresp),
    }

    println!("Pinged.");

    drop(i_rx);

    // MQTT DISCONNECT

    println!("Disconnecting");

    o_tx.send(vec![0xE0, 0]).unwrap();

    drop(o_tx);

    istream_thread.join().unwrap();
    ostream_thread.join().unwrap();

    Ok(())
}
