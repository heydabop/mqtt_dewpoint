use std::error::Error;
use std::io::prelude::*;
use std::sync::mpsc;
use std::thread;
use std::time;

mod mqtt;

fn main() -> Result<(), Box<dyn Error>> {
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

    let mut client = mqtt::Client::new(client_id, username, password, 60);
    client.connect(broker_addr)?;

    let mut o_stream = match client.stream {
        Some(s) => s.try_clone()?,
        None => panic!("Stream invalid after connect"),
    };

    let mut i_stream = o_stream.try_clone()?;

    let (o_tx, o_rx): (mpsc::Sender<Vec<u8>>, mpsc::Receiver<Vec<u8>>) = mpsc::channel();

    let o_stream_thread = thread::spawn(move || {
        while let Ok(msg) = o_rx.recv() {
            o_stream.write_all(&msg[..]).unwrap();
            o_stream.flush().unwrap();
        }
    });

    let i_stream_thread = thread::spawn(move || loop {
        let mut buf = [0; 127];
        if i_stream.read(&mut buf[..2]).unwrap() == 0 {
            break;
        }
        let len = (buf[1] + 2) as usize;
        if len > 0 {
            i_stream.read_exact(&mut buf[2..len]).unwrap();
        }
        match mqtt::parse_message(&buf[..len]) {
            Ok(message) => {
                if let mqtt::Message::Pingresp = message {
                    println!("Pinged.")
                } else {
                    eprintln!("Unexpected message type: {:?}", message)
                }
            }
            Err(e) => eprintln!("Error parsing message: {}", e),
        };
    });

    let ping_tx = mpsc::Sender::clone(&o_tx);
    let ping_thread = thread::spawn(move || {
        let interval = time::Duration::from_secs(30);
        loop {
            println!("Pinging...");
            ping_tx.send(mqtt::PINGREQ.to_vec()).unwrap();
            thread::sleep(interval);
        }
    });

    i_stream_thread.join().unwrap();
    o_stream_thread.join().unwrap();
    ping_thread.join().unwrap();

    Ok(())
}
