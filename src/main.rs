use serde::Deserialize;
use std::error::Error;
use std::process;
use std::thread;
use std::time;

mod mqtt;

extern crate ctrlc;

const A: f64 = 17.625;
const B: f64 = 243.04;

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

    client
        .subscribe("zigbee2mqtt/tempSensor", calculate_dewpoint)
        .unwrap();

    client.publish("homeassistant/sensor/dewpoint/config", r#"{"name":"dewpoint","device_class":"temperature","state_topic":"homeassistant/sensor/dewpoint/state","unit_of_measurement": "°C", "value_template": "{{ value_json.dewpoint}}"}"#);

    ctrlc::set_handler(move || {
        client.disconnect();
        process::exit(0);
    })
    .expect("Error setting Ctrl-C handler");

    loop {
        thread::sleep(time::Duration::from_secs(300));
    }

    #[allow(unreachable_code)]
    Ok(())
}

fn calculate_dewpoint(payload: Vec<u8>) -> Option<Vec<u8>> {
    let r: SensorRecord = serde_json::from_str(
        &String::from_utf8(payload).expect("Error generating string from payload"),
    )
    .expect("Error parsing JSON from string");

    println!(
        "Temp: {:.2}\u{b0}C / {:.2}\u{b0}F - Hum: {}%",
        r.temperature,
        r.temperature * 1.8 + 32_f64,
        r.humidity
    );

    let rh = r.humidity / 100.0;
    let t = r.temperature;
    let c = (A * t) / (B + t);
    let ln_rh = rh.ln();

    let dewpoint = (B * (ln_rh + c)) / (A - ln_rh - c);

    println!(
        "Dewpoint: {:.2}\u{b0}C / {:.2}\u{b0}F",
        dewpoint,
        dewpoint * 1.8 + 32_f64
    );

    Some(mqtt::make_publish(
        "homeassistant/sensor/dewpoint/state",
        &format!(r#"{{"dewpoint": {:.2}}}"#, dewpoint),
    ))
}

#[derive(Deserialize)]
struct SensorRecord {
    temperature: f64,
    humidity: f64,
}
