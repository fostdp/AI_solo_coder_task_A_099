use crate::alarm_ws::AlarmCommand;
use crate::clickhouse_client::ClickHouseClient;
use crate::models::*;
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
        log::info!("DtuReceiver task started");
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
        log::info!("DtuReceiver task stopped");
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
