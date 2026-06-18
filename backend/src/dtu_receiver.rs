use crate::alarm_ws::AlarmCommand;
use crate::clickhouse_client::ClickHouseClient;
use crate::metrics;
use crate::models::*;
use rumqttc::{AsyncClient, MqttOptions, QoS};
use std::time::Duration;
use tokio::sync::{mpsc, oneshot};

pub enum SensorCommand {
    Ingest {
        data: Vec<SensorData>,
        reply: oneshot::Sender<Result<usize, String>>,
    },
    GetRecent {
        ship_id: String,
        limit: u32,
        reply: oneshot::Sender<Result<Vec<SensorData>, String>>,
    },
}

pub struct DtuReceiver {
    rx: mpsc::Receiver<SensorCommand>,
    alarm_tx: mpsc::Sender<AlarmCommand>,
    clickhouse: ClickHouseClient,
}

impl DtuReceiver {
    pub fn new(
        rx: mpsc::Receiver<SensorCommand>,
        alarm_tx: mpsc::Sender<AlarmCommand>,
        clickhouse: ClickHouseClient,
    ) -> Self {
        Self {
            rx,
            alarm_tx,
            clickhouse,
        }
    }

    pub async fn run(mut self) {
        tracing::info!("DtuReceiver task started");
        while let Some(cmd) = self.rx.recv().await {
            match cmd {
                SensorCommand::Ingest { data, reply } => {
                    let result = self.handle_ingest(data).await;
                    let _ = reply.send(result);
                }
                SensorCommand::GetRecent {
                    ship_id,
                    limit,
                    reply,
                } => {
                    let result = self
                        .clickhouse
                        .get_recent_sensor_data(&ship_id, limit)
                        .await
                        .map_err(|e| e.to_string());
                    let _ = reply.send(result);
                }
            }
        }
        tracing::info!("DtuReceiver task stopped");
    }

    async fn handle_ingest(&self, data: Vec<SensorData>) -> Result<usize, String> {
        for item in &data {
            self.validate_sensor_data(item)?;
        }

        self.clickhouse
            .insert_sensor_data(&data)
            .await
            .map_err(|e| e.to_string())?;

        let count = data.len();
        metrics::SENSOR_INGESTED_TOTAL.inc_by(count as u64);
        let _ = self
            .alarm_tx
            .send(AlarmCommand::BroadcastSensorData { data })
            .await;

        Ok(count)
    }

    fn validate_sensor_data(&self, data: &SensorData) -> Result<(), String> {
        if data.draft < 0.0 || data.draft > 10.0 {
            return Err(format!("Invalid draft value: {}", data.draft));
        }
        if data.heel_angle < -45.0 || data.heel_angle > 45.0 {
            return Err(format!("Invalid heel angle: {}", data.heel_angle));
        }
        if data.trim_angle < -10.0 || data.trim_angle > 10.0 {
            return Err(format!("Invalid trim angle: {}", data.trim_angle));
        }
        if data.water_level < 0.0 || data.water_level > 5.0 {
            return Err(format!("Invalid water level: {}", data.water_level));
        }
        if data.damage_severity < 0.0 || data.damage_severity > 1.0 {
            return Err(format!("Invalid damage severity: {}", data.damage_severity));
        }
        Ok(())
    }
}

pub async fn run_mqtt_subscriber(
    host: String,
    port: u16,
    topic: String,
    sensor_tx: mpsc::Sender<SensorCommand>,
) {
    let client_id = format!("backend-mqtt-sub-{}", std::process::id());
    let mut options = MqttOptions::new(client_id, host, port);
    options.set_keep_alive(Duration::from_secs(30));
    options.set_clean_session(true);

    let (client, mut connection) = AsyncClient::new(options, 10);

    match client.subscribe(&topic, QoS::AtLeastOnce).await {
        Ok(_) => tracing::info!("MQTT subscribed to topic: {}", topic),
        Err(e) => {
            tracing::error!("Failed to subscribe to MQTT topic {}: {}", topic, e);
            metrics::MQTT_ERRORS.inc();
            return;
        }
    }

    tokio::task::spawn_blocking(move || loop {
        match connection.recv() {
            Ok(Ok(rumqttc::Event::Incoming(rumqttc::Packet::Publish(p)))) => {
                metrics::MQTT_MESSAGES.inc();
                match serde_json::from_slice::<SensorData>(p.payload.as_ref()) {
                    Ok(sensor) => {
                        tracing::debug!(
                            "MQTT sensor: ship={} draft={:.2}",
                            sensor.ship_id,
                            sensor.draft
                        );
                        let (tx, rx) = oneshot::channel();
                        if sensor_tx
                            .blocking_send(SensorCommand::Ingest {
                                data: vec![sensor],
                                reply: tx,
                            })
                            .is_ok()
                        {
                            let _ = rx.blocking_recv();
                        }
                    }
                    Err(e) => {
                        tracing::warn!("Failed to parse MQTT payload: {}", e);
                    }
                }
            }
            Ok(Ok(_)) => {}
            Ok(Err(e)) => {
                metrics::MQTT_ERRORS.inc();
                tracing::error!("MQTT connection error: {}", e);
            }
            Err(_) => {
                tracing::error!("MQTT request channel closed, stopping subscriber");
                break;
            }
        }
    });
}
