use prometheus::{
    register_int_counter, register_int_gauge, IntCounter, IntGauge, TextEncoder,
};
use lazy_static::lazy_static;

lazy_static! {
    pub static ref SENSOR_INGESTED_TOTAL: IntCounter =
        register_int_counter!("sensor_ingested_total", "Total sensor data records ingested")
            .expect("register sensor_ingested_total");
    pub static ref SIMULATIONS_TOTAL: IntCounter =
        register_int_counter!("simulations_total", "Total flooding simulations executed")
            .expect("register simulations_total");
    pub static ref ALARMS_TOTAL: IntCounter =
        register_int_counter!("alarms_total", "Total alarms generated")
            .expect("register alarms_total");
    pub static ref OPTIMIZATIONS_TOTAL: IntCounter =
        register_int_counter!("optimizations_total", "Total compartment optimizations executed")
            .expect("register optimizations_total");
    pub static ref MQTT_MESSAGES: IntCounter =
        register_int_counter!("mqtt_messages_total", "MQTT sensor messages received")
            .expect("register mqtt_messages_total");
    pub static ref MQTT_ERRORS: IntCounter =
        register_int_counter!("mqtt_errors_total", "MQTT connection errors")
            .expect("register mqtt_errors_total");
    pub static ref WS_CONNECTIONS: IntGauge =
        register_int_gauge!("ws_connections", "Active WebSocket connections")
            .expect("register ws_connections");
}

pub fn render() -> String {
    let encoder = TextEncoder::new();
    let mfs = prometheus::gather();
    let mut buf = Vec::new();
    let _ = encoder.encode(&mfs, &mut buf);
    String::from_utf8(buf).unwrap_or_default()
}
