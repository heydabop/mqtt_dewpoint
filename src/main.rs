use mqtt::client::Client;
use serde::Deserialize;
use std::error::Error;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;

extern crate ctrlc;

const A: f64 = 17.625;
const B: f64 = 243.04;

fn main() -> Result<(), Box<dyn Error>> {
    // config init
    let args: Vec<String> = std::env::args().collect();
    let filename = if args.len() >= 2 {
        &args[1]
    } else {
        "config.toml"
    };

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

    let mut client = Client::new(client_id, username, password, 60);
    client.connect(broker_addr)?;

    client
        .subscribe(
            "zigbee2mqtt/tempSensor",
            calculate_dewpoint("homeassistant/sensor/0x00158d0002c9119d/dewpoint/state"),
        )
        .unwrap();

    client
        .subscribe(
            "zigbee2mqtt/0x00158d00069afcf8",
            calculate_dewpoint("homeassistant/sensor/0x00158d00069afcf8/dewpoint/state"),
        )
        .unwrap();

    client.publish("homeassistant/sensor/dewpoint/config", r#"{"name":"dewpoint","device_class":"temperature","state_topic":"homeassistant/sensor/dewpoint/state","unit_of_measurement":"\u{b0}F"}"#);
    client.publish("homeassistant/sensor/upstairsDewpoint/config", r#"{"name":"upstairsDewpoint","device_class":"temperature","state_topic":"homeassistant/sensor/upstairsDewpoint/state","unit_of_measurement":"\u{b0}F"}"#);

    let main_thread = thread::current();
    let closing = Arc::new(AtomicBool::new(false));
    let closing_clone = closing.clone();

    ctrlc::set_handler(move || {
        closing_clone.store(true, Ordering::SeqCst);
        main_thread.unpark();
    })
    .expect("Error setting Ctrl-C handler");

    while !closing.load(Ordering::SeqCst) {
        thread::park();
    }

    client.disconnect();

    Ok(())
}

fn calculate_dewpoint(topic: &'static str) -> Box<dyn Fn(Vec<u8>) -> Option<Vec<u8>> + Send> {
    Box::new(move |payload| {
        let r: SensorRecord = serde_json::from_str(
            &String::from_utf8(payload).expect("Error generating string from payload"),
        )
        .expect("Error parsing JSON from string");

        println!(
            "Temp: {:.2}\u{b0}C / {:.2}\u{b0}F - Hum: {}%",
            r.temperature,
            r.temperature.mul_add(1.8, 32_f64),
            r.humidity
        );

        let rh = r.humidity / 100.0;
        let t = r.temperature;
        let c = (A * t) / (B + t);
        let ln_rh = rh.ln();

        let dewpoint = (B * (ln_rh + c)) / (A - ln_rh - c);
        let dewpoint_f = dewpoint.mul_add(1.8, 32_f64);

        println!(
            "Dewpoint: {:.2}\u{b0}C / {:.2}\u{b0}F",
            dewpoint, dewpoint_f
        );

        Some(mqtt::message::make_publish(
            topic,
            &format!("{:.1}", dewpoint_f),
        ))
    })
}

#[derive(Deserialize)]
struct SensorRecord {
    temperature: f64,
    humidity: f64,
}
