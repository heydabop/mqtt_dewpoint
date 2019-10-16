use std::io::prelude::*;
use std::net::TcpStream;

fn main() -> std::io::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 3 {
        eprintln!("Expected: {} <IP address> <port>", args[0]);
    }
    let ip = &args[1];
    let port = &args[2];

    let mut stream = TcpStream::connect(format!("{}:{}", ip, port))?;
    stream.set_read_timeout(None)?;
    stream.set_nodelay(true)?;

    // http://public.dhe.ibm.com/software/dw/webservices/ws-mqtt/MQTT_V3.1_Protocol_Specific.pdf

    let mut connect_msg: Vec<u8> = vec![0x10, 0, 0, 6]; // set remaining length (byte 2) after rest of message is set
    for b in "MQIsdp".chars() {
        //protocol name
        connect_msg.push(b as u8);
    }
    connect_msg.push(3); // protocol version
    connect_msg.push(0xC2); // connect flags (username, password, clean session)
    connect_msg.push(0); // keep alive
    connect_msg.push(10); // keep alive 10 seconds

    connect_msg.push(0); // client ID len
    connect_msg.push(8); // client ID len
    for b in "humidity".chars() {
        // client ID
        connect_msg.push(b as u8);
    }
    // no will topic or will message

    connect_msg.push(0); // username length
    connect_msg.push(8); // username length
    for b in "humidity".chars() {
        // username
        connect_msg.push(b as u8);
    }

    connect_msg.push(0); // password length
    connect_msg.push(10); // passowrd length
    for b in "__________".chars() {
        // password
        connect_msg.push(b as u8);
    }

    if connect_msg.len() > 127 {
        panic!("We don't support sending large messages yet");
    }
    connect_msg[1] = (connect_msg.len() - 2) as u8; // set len now that we know it

    stream.write_all(&connect_msg[..])?;
    stream.flush()?;

    let mut buf = vec![0; 4];
    stream.read(&mut buf[..])?;
    println!("{:?}", buf);

    if buf != [32, 2, 0, 0] {
        eprintln!("Error response code in CONNACK");
    }

    stream.write_all(&[0xE0, 0])?; // disconnect
    stream.flush()?;

    Ok(())
}
