use crate::models::*;
use actix::*;
use parking_lot::Mutex;
use std::sync::Arc;
use serde_json;

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

    pub fn broadcast_alarm(&self, alarm: &AlarmEvent) {
        let msg = WebSocketMessage {
            message_type: "alarm".to_string(),
            data: serde_json::to_value(alarm).unwrap_or(serde_json::Value::Null),
        };

        let sessions = self.sessions.lock();
        for (_, recipient, ship_id) in sessions.iter() {
            if ship_id.is_none() || ship_id.as_ref() == Some(&alarm.ship_id) {
                let _ = recipient.do_send(WsMessage(msg.clone()));
            }
        }
    }

    pub fn broadcast_sensor_data(&self, data: &[SensorData]) {
        if data.is_empty() {
            return;
        }

        let ship_id = data[0].ship_id.clone();
        let msg = WebSocketMessage {
            message_type: "sensor_data".to_string(),
            data: serde_json::to_value(data).unwrap_or(serde_json::Value::Null),
        };

        let sessions = self.sessions.lock();
        for (_, recipient, sub_ship_id) in sessions.iter() {
            if sub_ship_id.is_none() || sub_ship_id.as_ref() == Some(&ship_id) {
                let _ = recipient.do_send(WsMessage(msg.clone()));
            }
        }
    }

    pub fn broadcast_simulation_result(&self, result: &StabilityResult) {
        let msg = WebSocketMessage {
            message_type: "simulation_result".to_string(),
            data: serde_json::to_value(result).unwrap_or(serde_json::Value::Null),
        };

        let sessions = self.sessions.lock();
        for (_, recipient, ship_id) in sessions.iter() {
            if ship_id.is_none() || ship_id.as_ref() == Some(&result.ship_id) {
                let _ = recipient.do_send(WsMessage(msg.clone()));
            }
        }
    }
}

impl Actor for WsServer {
    type Context = Context<Self>;
}

#[derive(Message)]
#[rtype(result = "()")]
pub struct Connect {
    pub addr: Recipient<WsMessage>,
    pub ship_id: Option<String>,
}

#[derive(Message)]
#[rtype(result = "()")]
pub struct Disconnect {
    pub id: usize,
}

impl Handler<Connect> for WsServer {
    type Result = usize;

    fn handle(&mut self, msg: Connect, _: &mut Context<Self>) -> Self::Result {
        let id = self.next_id;
        self.next_id += 1;
        self.sessions.lock().push((id, msg.addr, msg.ship_id));
        log::info!("New WebSocket connection: {}", id);
        id
    }
}

impl Handler<Disconnect> for WsServer {
    type Result = ();

    fn handle(&mut self, msg: Disconnect, _: &mut Context<Self>) {
        self.sessions.lock().retain(|(id, _, _)| *id != msg.id);
        log::info!("WebSocket disconnected: {}", msg.id);
    }
}
