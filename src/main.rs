use std::error::Error;
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

    client.subscribe("zigbee2mqtt/tempSensor").unwrap();

    loop {
        thread::sleep(time::Duration::from_secs(300));
    }

    #[allow(unreachable_code)]
    Ok(())
}
