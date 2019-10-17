use std::io::prelude::*;
use std::net::TcpStream;
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

    // MQTT CONNECT

    let connect_msg = mqtt::make_connect(client_id, username, password);

    stream.write_all(&connect_msg[..])?;
    stream.flush()?;

    // MQTT CONNACK

    let mut buf = vec![0; 4];
    stream.read(&mut buf[..])?;
    println!("{:?}", buf);

    if buf != [32, 2, 0, 0] {
        eprintln!("Error response code in CONNACK");
    }

    let five_sec = time::Duration::from_secs(5);
    thread::sleep(five_sec);

    // MQTT PINGREQ

    stream.write_all(&mqtt::PINGREQ[..])?;
    stream.flush()?;

    let five_sec = time::Duration::from_secs(5);
    thread::sleep(five_sec);

    // MQTT PINGRESP

    buf = vec![0; 2];
    stream.read(&mut buf[..])?;
    println!("{:?}", buf);

    if buf != mqtt::PINGRESP {
        eprintln!("Didn't receive PINGRESP after PINGREQ");
    }

    // MQTT DISCONNECT

    stream.write_all(&[0xE0, 0])?;
    stream.flush()?;

    Ok(())
}
