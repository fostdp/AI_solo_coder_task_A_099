use crate::alarm_ws::{Connect, Disconnect, WsMessage, WsServer};
use crate::clickhouse_client::ClickHouseClient;
use crate::compartment_optimizer::OptimizeCommand;
use crate::dtu_receiver::SensorCommand;
use crate::flooding_simulator::SimCommand;
use crate::models::*;
use actix::{Actor, Addr, Handler, StreamHandler};
use actix_web::{web, Error, HttpResponse, Responder};
use actix_web_actors::ws;
use serde_json::json;
use std::time::{Duration, Instant};
use tokio::sync::{mpsc, oneshot};

pub struct AppState {
    pub sensor_tx: mpsc::Sender<SensorCommand>,
    pub sim_tx: mpsc::Sender<SimCommand>,
    pub optimize_tx: mpsc::Sender<OptimizeCommand>,
    pub clickhouse: ClickHouseClient,
    pub ws_server: Addr<WsServer>,
    pub default_config: ShipConfig,
}

pub async fn health_check() -> impl Responder {
    HttpResponse::Ok().json(json!({
        "status": "ok",
        "service": "古代水密隔舱船舶抗沉性仿真系统",
        "version": "2.0.0"
    }))
}

pub async fn metrics_handler() -> impl Responder {
    HttpResponse::Ok()
        .content_type("text/plain; version=0.0.4; charset=utf-8")
        .body(crate::metrics::render())
}

pub async fn default_ship_config(state: web::Data<AppState>) -> impl Responder {
    HttpResponse::Ok().json(&state.default_config)
}

pub async fn get_ship_config(
    path: web::Path<String>,
    state: web::Data<AppState>,
) -> impl Responder {
    let ship_id = path.into_inner();
    match state.clickhouse.get_ship_config(&ship_id).await {
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
    state: web::Data<AppState>,
    query: web::Query<QueryLimit>,
) -> impl Responder {
    let ship_id = path.into_inner();
    let limit = query.limit.unwrap_or(100);
    let (tx, rx) = oneshot::channel();
    let cmd = SensorCommand::GetRecent {
        ship_id,
        limit,
        reply: tx,
    };

    if let Err(e) = state.sensor_tx.send(cmd).await {
        return HttpResponse::InternalServerError().json(json!({
            "error": format!("Channel send failed: {}", e)
        }));
    }

    match rx.await {
        Ok(Ok(data)) => HttpResponse::Ok().json(data),
        Ok(Err(e)) => HttpResponse::InternalServerError().json(json!({ "error": e })),
        Err(_) => HttpResponse::InternalServerError().json(json!({
            "error": "Channel receiver dropped"
        })),
    }
}

pub async fn get_alarms(
    path: web::Path<String>,
    state: web::Data<AppState>,
    query: web::Query<QueryLimit>,
) -> impl Responder {
    let ship_id = path.into_inner();
    let limit = query.limit.unwrap_or(50);
    match state.clickhouse.get_recent_alarms(&ship_id, limit).await {
        Ok(data) => HttpResponse::Ok().json(data),
        Err(e) => HttpResponse::InternalServerError().json(json!({
            "error": e.to_string()
        })),
    }
}

pub async fn ingest_sensor_data(
    state: web::Data<AppState>,
    data: web::Json<Vec<SensorData>>,
) -> impl Responder {
    let (tx, rx) = oneshot::channel();
    let cmd = SensorCommand::Ingest {
        data: data.into_inner(),
        reply: tx,
    };

    if let Err(e) = state.sensor_tx.send(cmd).await {
        return HttpResponse::InternalServerError().json(json!({
            "error": format!("Channel send failed: {}", e)
        }));
    }

    match rx.await {
        Ok(Ok(count)) => HttpResponse::Ok().json(json!({
            "status": "success",
            "count": count
        })),
        Ok(Err(e)) => HttpResponse::InternalServerError().json(json!({ "error": e })),
        Err(_) => HttpResponse::InternalServerError().json(json!({
            "error": "Channel receiver dropped"
        })),
    }
}

pub async fn simulate_damage(
    state: web::Data<AppState>,
    scenario: web::Json<FloodingScenario>,
) -> impl Responder {
    let (tx, rx) = oneshot::channel();
    let cmd = SimCommand::Simulate {
        scenario: scenario.into_inner(),
        reply: tx,
    };

    if let Err(e) = state.sim_tx.send(cmd).await {
        return HttpResponse::InternalServerError().json(json!({
            "error": format!("Channel send failed: {}", e)
        }));
    }

    match rx.await {
        Ok(Ok(result)) => HttpResponse::Ok().json(result),
        Ok(Err(e)) => {
            if e.contains("not found") {
                HttpResponse::NotFound().json(json!({ "error": e }))
            } else {
                HttpResponse::InternalServerError().json(json!({ "error": e }))
            }
        }
        Err(_) => HttpResponse::InternalServerError().json(json!({
            "error": "Channel receiver dropped"
        })),
    }
}

pub async fn batch_simulate(
    state: web::Data<AppState>,
    scenarios: web::Json<Vec<FloodingScenario>>,
) -> impl Responder {
    let (tx, rx) = oneshot::channel();
    let cmd = SimCommand::BatchSimulate {
        scenarios: scenarios.into_inner(),
        reply: tx,
    };

    if let Err(e) = state.sim_tx.send(cmd).await {
        return HttpResponse::InternalServerError().json(json!({
            "error": format!("Channel send failed: {}", e)
        }));
    }

    match rx.await {
        Ok(Ok(results)) => HttpResponse::Ok().json(json!({
            "status": "success",
            "count": results.len(),
            "results": results
        })),
        Ok(Err(e)) => HttpResponse::InternalServerError().json(json!({ "error": e })),
        Err(_) => HttpResponse::InternalServerError().json(json!({
            "error": "Channel receiver dropped"
        })),
    }
}

pub async fn optimize_compartments(
    state: web::Data<AppState>,
    request: web::Json<OptimizationRequest>,
) -> impl Responder {
    let (tx, rx) = oneshot::channel();
    let cmd = OptimizeCommand::Optimize {
        request: request.into_inner(),
        reply: tx,
    };

    if let Err(e) = state.optimize_tx.send(cmd).await {
        return HttpResponse::InternalServerError().json(json!({
            "error": format!("Channel send failed: {}", e)
        }));
    }

    match rx.await {
        Ok(Ok(result)) => HttpResponse::Ok().json(result),
        Ok(Err(e)) => {
            if e.contains("not found") {
                HttpResponse::NotFound().json(json!({ "error": e }))
            } else {
                HttpResponse::InternalServerError().json(json!({ "error": e }))
            }
        }
        Err(_) => HttpResponse::InternalServerError().json(json!({
            "error": "Channel receiver dropped"
        })),
    }
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
                tracing::warn!("WebSocket client heartbeat failed, disconnecting");
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
        self.id = self
            .ws_server
            .send(Connect {
                addr,
                ship_id: self.ship_id.clone(),
            })
            .wait()
            .unwrap_or(0);
        tracing::info!("WebSocket connection started: {}", self.id);
    }

    fn stopped(&mut self, _ctx: &mut Self::Context) {
        self.ws_server.do_send(Disconnect { id: self.id });
        tracing::info!("WebSocket connection closed: {}", self.id);
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
                            tracing::info!("Client {} subscribed to ship: {}", self.id, ship_id);
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

pub async fn ws_index(
    req: actix_web::HttpRequest,
    stream: web::Payload,
    state: web::Data<AppState>,
    query: web::Query<WsQuery>,
) -> Result<HttpResponse, Error> {
    let ship_id = query.ship_id.clone();
    let resp = ws::start(
        WsSession::new(state.ws_server.clone(), ship_id),
        &req,
        stream,
    );
    resp
}

#[derive(Debug, serde::Deserialize)]
pub struct QueryLimit {
    pub limit: Option<u32>,
}

#[derive(Debug, serde::Deserialize)]
pub struct WsQuery {
    pub ship_id: Option<String>,
}
