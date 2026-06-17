use crate::clickhouse_client::ClickHouseClient;
use crate::genetic_algorithm::GeneticOptimizer;
use crate::models::*;
use crate::ship_statics::{ShipHydrostatics, check_alarm_conditions};
use crate::websocket::{WsServer, Connect, Disconnect, WsMessage};
use actix::{Actor, Addr, Handler, Message, Recipient, StreamHandler};
use actix_web::{web, Error, HttpResponse, Responder};
use actix_web_actors::ws;
use chrono::Utc;
use serde_json::json;
use std::time::{Duration, Instant};

pub async fn get_ship_config(
    path: web::Path<String>,
    clickhouse: web::Data<ClickHouseClient>,
) -> impl Responder {
    let ship_id = path.into_inner();
    match clickhouse.get_ship_config(&ship_id).await {
        Ok(Some(config)) => HttpResponse::Ok().json(config),
        Ok(None) => HttpResponse::NotFound().json(json!({
            "error": format!("Ship config not found for id: {}", ship_id)
        })),
        Err(e) => HttpResponse::InternalServerError().json(json!({
            "error": e.to_string()
        })),
    }
}

pub async fn get_sensor_data(
    path: web::Path<String>,
    clickhouse: web::Data<ClickHouseClient>,
    query: web::Query<QueryLimit>,
) -> impl Responder {
    let ship_id = path.into_inner();
    let limit = query.limit.unwrap_or(100);
    match clickhouse.get_recent_sensor_data(&ship_id, limit).await {
        Ok(data) => HttpResponse::Ok().json(data),
        Err(e) => HttpResponse::InternalServerError().json(json!({
            "error": e.to_string()
        })),
    }
}

pub async fn get_alarms(
    path: web::Path<String>,
    clickhouse: web::Data<ClickHouseClient>,
    query: web::Query<QueryLimit>,
) -> impl Responder {
    let ship_id = path.into_inner();
    let limit = query.limit.unwrap_or(50);
    match clickhouse.get_recent_alarms(&ship_id, limit).await {
        Ok(data) => HttpResponse::Ok().json(data),
        Err(e) => HttpResponse::InternalServerError().json(json!({
            "error": e.to_string()
        })),
    }
}

pub async fn ingest_sensor_data(
    clickhouse: web::Data<ClickHouseClient>,
    ws_server: web::Data<Addr<WsServer>>,
    data: web::Json<Vec<SensorData>>,
) -> impl Responder {
    let sensor_data = data.into_inner();
    if let Err(e) = clickhouse.insert_sensor_data(&sensor_data).await {
        return HttpResponse::InternalServerError().json(json!({
            "error": e.to_string()
        }));
    }

    ws_server.get_ref().broadcast_sensor_data(&sensor_data);

    HttpResponse::Ok().json(json!({
        "status": "success",
        "count": sensor_data.len()
    }))
}

pub async fn simulate_damage(
    clickhouse: web::Data<ClickHouseClient>,
    ws_server: web::Data<Addr<WsServer>>,
    scenario: web::Json<FloodingScenario>,
) -> impl Responder {
    let scenario = scenario.into_inner();

    let config = match clickhouse.get_ship_config(&scenario.ship_id).await {
        Ok(Some(c)) => c,
        Ok(None) => return HttpResponse::NotFound().json(json!({
            "error": format!("Ship config not found for id: {}", scenario.ship_id)
        })),
        Err(e) => return HttpResponse::InternalServerError().json(json!({
            "error": e.to_string()
        })),
    };

    let hydrostatics = ShipHydrostatics::new(config.clone());
    let result = hydrostatics.simulate_damage(&scenario);

    if let Err(e) = clickhouse.insert_simulation_result(&result).await {
        return HttpResponse::InternalServerError().json(json!({
            "error": e.to_string()
        }));
    }

    if let Some(alarm) = check_alarm_conditions(&result, &config) {
        if let Err(e) = clickhouse.insert_alarm_event(&alarm).await {
            log::error!("Failed to insert alarm: {}", e);
        }
        ws_server.get_ref().broadcast_alarm(&alarm);
    }

    ws_server.get_ref().broadcast_simulation_result(&result);

    HttpResponse::Ok().json(result)
}

pub async fn batch_simulate(
    clickhouse: web::Data<ClickHouseClient>,
    ws_server: web::Data<Addr<WsServer>>,
    scenarios: web::Json<Vec<FloodingScenario>>,
) -> impl Responder {
    let scenarios = scenarios.into_inner();
    let mut results = Vec::new();

    for scenario in scenarios {
        let config = match clickhouse.get_ship_config(&scenario.ship_id).await {
            Ok(Some(c)) => c,
            Ok(None) => continue,
            Err(_) => continue,
        };

        let hydrostatics = ShipHydrostatics::new(config.clone());
        let result = hydrostatics.simulate_damage(&scenario);

        if let Ok(_) = clickhouse.insert_simulation_result(&result).await {
            if let Some(alarm) = check_alarm_conditions(&result, &config) {
                let _ = clickhouse.insert_alarm_event(&alarm).await;
                ws_server.get_ref().broadcast_alarm(&alarm);
            }
            ws_server.get_ref().broadcast_simulation_result(&result);
        }

        results.push(result);
    }

    HttpResponse::Ok().json(json!({
        "status": "success",
        "count": results.len(),
        "results": results
    }))
}

pub async fn optimize_compartments(
    clickhouse: web::Data<ClickHouseClient>,
    request: web::Json<OptimizationRequest>,
) -> impl Responder {
    let request = request.into_inner();

    let config = match clickhouse.get_ship_config(&request.ship_id).await {
        Ok(Some(c)) => c,
        Ok(None) => return HttpResponse::NotFound().json(json!({
            "error": format!("Ship config not found for id: {}", request.ship_id)
        })),
        Err(e) => return HttpResponse::InternalServerError().json(json!({
            "error": e.to_string()
        })),
    };

    let optimizer = GeneticOptimizer::new(
        config,
        request.population_size.max(20),
        request.generations.max(50),
    );

    let result = optimizer.optimize_compartment_count(
        request.min_compartments.max(3),
        request.max_compartments.min(20),
    );

    if let Err(e) = clickhouse.insert_optimization_result(&result).await {
        log::error!("Failed to insert optimization result: {}", e);
    }

    HttpResponse::Ok().json(result)
}

pub async fn ws_index(
    req: actix_web::HttpRequest,
    stream: web::Payload,
    ws_server: web::Data<Addr<WsServer>>,
    query: web::Query<WsQuery>,
) -> Result<HttpResponse, Error> {
    let ship_id = query.ship_id.clone();
    let resp = ws::start(
        WsSession::new(ws_server.get_ref().clone(), ship_id),
        &req,
        stream,
    );
    resp
}

const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(5);
const CLIENT_TIMEOUT: Duration = Duration::from_secs(10);

struct WsSession {
    id: usize,
    hb: Instant,
    ship_id: Option<String>,
    ws_server: Addr<WsServer>,
}

impl WsSession {
    fn new(ws_server: Addr<WsServer>, ship_id: Option<String>) -> Self {
        Self {
            id: 0,
            hb: Instant::now(),
            ship_id,
            ws_server,
        }
    }

    fn hb(&self, ctx: &mut ws::WebsocketContext<Self>) {
        ctx.run_interval(HEARTBEAT_INTERVAL, |act, ctx| {
            if Instant::now().duration_since(act.hb) > CLIENT_TIMEOUT {
                log::warn!("WebSocket client heartbeat failed, disconnecting");
                ctx.stop();
                return;
            }
            ctx.ping(b"");
        });
    }
}

impl Actor for WsSession {
    type Context = ws::WebsocketContext<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        self.hb(ctx);
        let addr = ctx.address().recipient();
        self.id = self.ws_server.send(Connect {
            addr,
            ship_id: self.ship_id.clone(),
        }).wait().unwrap_or(0);
        log::info!("WebSocket connection started: {}", self.id);
    }

    fn stopped(&mut self, _ctx: &mut Self::Context) {
        self.ws_server.do_send(Disconnect { id: self.id });
        log::info!("WebSocket connection closed: {}", self.id);
    }
}

impl StreamHandler<Result<ws::Message, ws::ProtocolError>> for WsSession {
    fn handle(&mut self, msg: Result<ws::Message, ws::ProtocolError>, ctx: &mut Self::Context) {
        match msg {
            Ok(ws::Message::Ping(msg)) => {
                self.hb = Instant::now();
                ctx.pong(&msg);
            }
            Ok(ws::Message::Pong(_)) => {
                self.hb = Instant::now();
            }
            Ok(ws::Message::Text(text)) => {
                self.hb = Instant::now();
                if let Ok(msg) = serde_json::from_str::<WebSocketMessage>(&text) {
                    if msg.message_type == "subscribe" {
                        if let Some(ship_id) = msg.data.as_str() {
                            self.ship_id = Some(ship_id.to_string());
                            log::info!("Client {} subscribed to ship: {}", self.id, ship_id);
                        }
                    }
                }
            }
            Ok(ws::Message::Binary(bin)) => {
                ctx.binary(bin);
            }
            Ok(ws::Message::Close(reason)) => {
                ctx.close(reason);
                ctx.stop();
            }
            _ => {}
        }
    }
}

impl Handler<WsMessage> for WsSession {
    type Result = ();

    fn handle(&mut self, msg: WsMessage, ctx: &mut Self::Context) {
        if let Ok(text) = serde_json::to_string(&msg.0) {
            ctx.text(text);
        }
    }
}

pub async fn health_check() -> impl Responder {
    HttpResponse::Ok().json(json!({
        "status": "ok",
        "service": "古代水密隔舱船舶抗沉性仿真系统",
        "version": "1.0.0"
    }))
}

pub async fn default_ship_config() -> impl Responder {
    HttpResponse::Ok().json(json!({
        "ship_id": "quanzhou_song_001",
        "ship_name": "泉州宋代海船",
        "length_overall": 34.0,
        "beam": 11.0,
        "depth": 4.5,
        "design_draft": 2.8,
        "displacement": 400.0,
        "compartment_count": 13,
        "compartment_names": [
            "艏尖舱", "前货舱1", "前货舱2", "中货舱1", "中货舱2",
            "中货舱3", "中货舱4", "后货舱1", "后货舱2", "机舱",
            "艉尖舱", "淡水舱1", "淡水舱2"
        ],
        "compartment_lengths": [2.5, 2.8, 2.8, 3.0, 3.0, 3.0, 3.0, 2.8, 2.8, 4.0, 2.3, 1.5, 1.5],
        "compartment_volumes": [15.0, 85.0, 85.0, 95.0, 95.0, 95.0, 95.0, 85.0, 85.0, 100.0, 12.0, 20.0, 20.0],
        "watertight_bulkheads": [2.5, 5.3, 8.1, 11.1, 14.1, 17.1, 20.1, 22.9, 25.7, 29.7, 32.0, 33.5, 35.0]
    }))
}

#[derive(Debug, serde::Deserialize)]
pub struct QueryLimit {
    pub limit: Option<u32>,
}

#[derive(Debug, serde::Deserialize)]
pub struct WsQuery {
    pub ship_id: Option<String>,
}
