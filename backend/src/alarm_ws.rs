use crate::clickhouse_client::ClickHouseClient;
use crate::metrics;
use crate::models::*;
use actix::*;
use parking_lot::Mutex;
use serde_json;
use tokio::sync::mpsc;
use std::sync::Arc;

#[derive(Clone)]
pub struct WsMessage(pub WebSocketMessage);

impl Message for WsMessage {
    type Result = ();
}

pub struct WsServer {
    pub sessions: Arc<Mutex<Vec<(usize, Recipient<WsMessage>, Option<String>)>>>,
    pub next_id: usize,
}

impl WsServer {
    pub fn new() -> Self {
        Self {
            sessions: Arc::new(Mutex::new(Vec::new())),
            next_id: 0,
        }
    }
}

impl Actor for WsServer {
    type Context = Context<Self>;
}

#[derive(Message)]
#[rtype(result = "usize")]
pub struct Connect {
    pub addr: Recipient<WsMessage>,
    pub ship_id: Option<String>,
}

#[derive(Message)]
#[rtype(result = "()")]
pub struct Disconnect {
    pub id: usize,
}

#[derive(Message)]
#[rtype(result = "()")]
pub struct BroadcastAlarm(pub AlarmEvent);

#[derive(Message)]
#[rtype(result = "()")]
pub struct BroadcastSensorData(pub Vec<SensorData>);

#[derive(Message)]
#[rtype(result = "()")]
pub struct BroadcastSimResult(pub StabilityResult);

impl Handler<Connect> for WsServer {
    type Result = usize;

    fn handle(&mut self, msg: Connect, _: &mut Context<Self>) -> Self::Result {
        let id = self.next_id;
        self.next_id += 1;
        self.sessions.lock().push((id, msg.addr, msg.ship_id));
        tracing::info!("New WebSocket connection: {}", id);
        metrics::WS_CONNECTIONS.inc();
        id
    }
}

impl Handler<Disconnect> for WsServer {
    type Result = ();

    fn handle(&mut self, msg: Disconnect, _: &mut Context<Self>) {
        self.sessions.lock().retain(|(id, _, _)| *id != msg.id);
        tracing::info!("WebSocket disconnected: {}", msg.id);
        metrics::WS_CONNECTIONS.dec();
    }
}

impl Handler<BroadcastAlarm> for WsServer {
    type Result = ();

    fn handle(&mut self, msg: BroadcastAlarm, _: &mut Context<Self>) {
        let alarm = msg.0;
        let ws_msg = WebSocketMessage {
            message_type: "alarm".to_string(),
            data: serde_json::to_value(&alarm).unwrap_or(serde_json::Value::Null),
        };
        let sessions = self.sessions.lock();
        for (_, recipient, ship_id) in sessions.iter() {
            if ship_id.is_none() || ship_id.as_ref() == Some(&alarm.ship_id) {
                let _ = recipient.do_send(WsMessage(ws_msg.clone()));
            }
        }
    }
}

impl Handler<BroadcastSensorData> for WsServer {
    type Result = ();

    fn handle(&mut self, msg: BroadcastSensorData, _: &mut Context<Self>) {
        if msg.0.is_empty() {
            return;
        }
        let ship_id = msg.0[0].ship_id.clone();
        let ws_msg = WebSocketMessage {
            message_type: "sensor_data".to_string(),
            data: serde_json::to_value(&msg.0).unwrap_or(serde_json::Value::Null),
        };
        let sessions = self.sessions.lock();
        for (_, recipient, sub_ship_id) in sessions.iter() {
            if sub_ship_id.is_none() || sub_ship_id.as_ref() == Some(&ship_id) {
                let _ = recipient.do_send(WsMessage(ws_msg.clone()));
            }
        }
    }
}

impl Handler<BroadcastSimResult> for WsServer {
    type Result = ();

    fn handle(&mut self, msg: BroadcastSimResult, _: &mut Context<Self>) {
        let result = msg.0;
        let ws_msg = WebSocketMessage {
            message_type: "simulation_result".to_string(),
            data: serde_json::to_value(&result).unwrap_or(serde_json::Value::Null),
        };
        let sessions = self.sessions.lock();
        for (_, recipient, _) in sessions.iter() {
            let _ = recipient.do_send(WsMessage(ws_msg.clone()));
        }
    }
}

pub fn check_alarm_conditions(
    result: &StabilityResult,
    config: &ShipConfig,
    params: &DamageParams,
) -> Option<AlarmEvent> {
    if result.metacentric_height < params.min_metacentric_height {
        return Some(AlarmEvent {
            alarm_id: uuid::Uuid::new_v4(),
            ship_id: config.ship_id.clone(),
            timestamp: chrono::Utc::now(),
            alarm_type: AlarmType::StabilityLoss,
            alarm_level: AlarmLevel::Critical,
            description: format!(
                "稳性丧失警告: GM={:.3}m 低于最小值 {:.2}m",
                result.metacentric_height, params.min_metacentric_height
            ),
            flooded_compartments: result.flooded_compartments.clone(),
            metacentric_height: result.metacentric_height,
            heel_angle: result.final_heel_angle,
        });
    }

    if result.final_heel_angle.abs() > params.max_safe_heel_angle {
        return Some(AlarmEvent {
            alarm_id: uuid::Uuid::new_v4(),
            ship_id: config.ship_id.clone(),
            timestamp: chrono::Utc::now(),
            alarm_type: AlarmType::HeelExcessive,
            alarm_level: AlarmLevel::Warning,
            description: format!(
                "横倾角过大警告: 横倾角={:.1}° 超过安全值 {:.1}°",
                result.final_heel_angle, params.max_safe_heel_angle
            ),
            flooded_compartments: result.flooded_compartments.clone(),
            metacentric_height: result.metacentric_height,
            heel_angle: result.final_heel_angle,
        });
    }

    if result.final_draft > config.depth * params.draft_depth_ratio_threshold {
        return Some(AlarmEvent {
            alarm_id: uuid::Uuid::new_v4(),
            ship_id: config.ship_id.clone(),
            timestamp: chrono::Utc::now(),
            alarm_type: AlarmType::DraftExceeded,
            alarm_level: AlarmLevel::Warning,
            description: format!(
                "吃水超限警告: 当前吃水={:.2}m 接近船深 {:.2}m",
                result.final_draft, config.depth
            ),
            flooded_compartments: result.flooded_compartments.clone(),
            metacentric_height: result.metacentric_height,
            heel_angle: result.final_heel_angle,
        });
    }

    if result.flooded_compartments.len() >= params.flooding_spread_count {
        return Some(AlarmEvent {
            alarm_id: uuid::Uuid::new_v4(),
            ship_id: config.ship_id.clone(),
            timestamp: chrono::Utc::now(),
            alarm_type: AlarmType::FloodingSpread,
            alarm_level: AlarmLevel::Critical,
            description: format!(
                "进水蔓延警告: {} 个舱室进水",
                result.flooded_compartments.len()
            ),
            flooded_compartments: result.flooded_compartments.clone(),
            metacentric_height: result.metacentric_height,
            heel_angle: result.final_heel_angle,
        });
    }

    None
}

pub enum AlarmCommand {
    EvaluateResult {
        result: StabilityResult,
        config: ShipConfig,
    },
    BroadcastSensorData {
        data: Vec<SensorData>,
    },
}

pub struct AlarmWs {
    rx: mpsc::Receiver<AlarmCommand>,
    ws_server: Addr<WsServer>,
    clickhouse: ClickHouseClient,
    damage_params: DamageParams,
}

impl AlarmWs {
    pub fn new(
        rx: mpsc::Receiver<AlarmCommand>,
        ws_server: Addr<WsServer>,
        clickhouse: ClickHouseClient,
        damage_params: DamageParams,
    ) -> Self {
        Self {
            rx,
            ws_server,
            clickhouse,
            damage_params,
        }
    }

    pub async fn run(mut self) {
        tracing::info!("AlarmWs task started");
        while let Some(cmd) = self.rx.recv().await {
            match cmd {
                AlarmCommand::EvaluateResult { result, config } => {
                    if let Some(alarm) =
                        check_alarm_conditions(&result, &config, &self.damage_params)
                    {
                        if let Err(e) = self.clickhouse.insert_alarm_event(&alarm).await {
                            tracing::error!("Failed to insert alarm: {}", e);
                        }
                        metrics::ALARMS_TOTAL.inc();
                        self.ws_server.do_send(BroadcastAlarm(alarm));
                    }
                    self.ws_server.do_send(BroadcastSimResult(result));
                }
                AlarmCommand::BroadcastSensorData { data } => {
                    self.ws_server.do_send(BroadcastSensorData(data));
                }
            }
        }
        tracing::info!("AlarmWs task stopped");
    }
}
