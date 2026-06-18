mod alarm_ws;
mod clickhouse_client;
mod compartment_optimizer;
mod dtu_receiver;
mod flooding_simulator;
mod handlers;
mod models;

use actix::prelude::*;
use actix_cors::Cors;
use actix_web::{web, App, HttpServer};
use log::info;
use std::env;
use tokio::sync::mpsc;

use crate::alarm_ws::{AlarmCommand, AlarmWs, WsServer};
use crate::clickhouse_client::ClickHouseClient;
use crate::compartment_optimizer::{CompartmentOptimizer, OptimizeCommand};
use crate::dtu_receiver::{DtuReceiver, SensorCommand};
use crate::flooding_simulator::{FloodingSimulator, SimCommand};
use crate::handlers::*;
use crate::models::*;

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let clickhouse_url = env::var("CLICKHOUSE_URL")
        .unwrap_or_else(|_| "tcp://localhost:9000?compression=lz4".to_string());
    let server_host = env::var("SERVER_HOST").unwrap_or_else(|_| "0.0.0.0".to_string());
    let server_port = env::var("SERVER_PORT").unwrap_or_else(|_| "8080".to_string());

    let config_path = env::var("SHIP_CONFIG_PATH")
        .unwrap_or_else(|_| "config/ship_config.json".to_string());
    let damage_params_path = env::var("DAMAGE_PARAMS_PATH")
        .unwrap_or_else(|_| "config/damage_params.json".to_string());

    let default_config: ShipConfig = std::fs::read_to_string(&config_path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_else(|| {
            log::warn!(
                "Failed to load ship config from {}, using fallback",
                config_path
            );
            fallback_ship_config()
        });

    let damage_params: DamageParams = std::fs::read_to_string(&damage_params_path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default();

    info!("Connecting to ClickHouse at: {}", clickhouse_url);

    let clickhouse_client = match ClickHouseClient::new(&clickhouse_url).await {
        Ok(client) => {
            info!("Successfully connected to ClickHouse");
            client
        }
        Err(e) => {
            log::warn!(
                "Failed to connect to ClickHouse: {}. Continuing without database persistence.",
                e
            );
            return Err(std::io::Error::new(
                std::io::ErrorKind::ConnectionRefused,
                e.to_string(),
            ));
        }
    };

    let (sensor_tx, sensor_rx) = mpsc::channel::<SensorCommand>(100);
    let (sim_tx, sim_rx) = mpsc::channel::<SimCommand>(100);
    let (optimize_tx, optimize_rx) = mpsc::channel::<OptimizeCommand>(100);
    let (alarm_tx, alarm_rx) = mpsc::channel::<AlarmCommand>(100);

    let ws_server = WsServer::new().start();

    let dtu_receiver = DtuReceiver::new(sensor_rx, alarm_tx.clone(), clickhouse_client.clone());
    tokio::spawn(dtu_receiver.run());

    let flooding_simulator = FloodingSimulator::new(
        sim_rx,
        alarm_tx.clone(),
        clickhouse_client.clone(),
        damage_params.clone(),
    );
    tokio::spawn(flooding_simulator.run());

    let compartment_optimizer = CompartmentOptimizer::new(
        optimize_rx,
        clickhouse_client.clone(),
        damage_params.clone(),
    );
    tokio::spawn(compartment_optimizer.run());

    let alarm_ws = AlarmWs::new(
        alarm_rx,
        ws_server.clone(),
        clickhouse_client.clone(),
        damage_params.clone(),
    );
    tokio::spawn(alarm_ws.run());

    info!("All service tasks spawned: dtu_receiver, flooding_simulator, compartment_optimizer, alarm_ws");

    let app_state = web::Data::new(AppState {
        sensor_tx,
        sim_tx,
        optimize_tx,
        clickhouse: clickhouse_client,
        ws_server,
        default_config,
    });

    info!("Starting server on {}:{}", server_host, server_port);

    HttpServer::new(move || {
        let cors = Cors::permissive();

        App::new()
            .wrap(cors)
            .app_data(app_state.clone())
            .route("/health", web::get().to(health_check))
            .route("/api/config/default", web::get().to(default_ship_config))
            .route("/api/config/{ship_id}", web::get().to(get_ship_config))
            .route("/api/sensor/{ship_id}", web::get().to(get_sensor_data))
            .route("/api/sensor", web::post().to(ingest_sensor_data))
            .route("/api/alarm/{ship_id}", web::get().to(get_alarms))
            .route("/api/simulate", web::post().to(simulate_damage))
            .route("/api/simulate/batch", web::post().to(batch_simulate))
            .route("/api/optimize", web::post().to(optimize_compartments))
            .route("/ws", web::get().to(ws_index))
    })
    .bind(format!("{}:{}", server_host, server_port))?
    .run()
    .await?;

    info!("Server stopped");
    Ok(())
}

fn fallback_ship_config() -> ShipConfig {
    ShipConfig {
        ship_id: "quanzhou_song_001".to_string(),
        ship_name: "泉州宋代海船".to_string(),
        length_overall: 34.0,
        beam: 11.0,
        depth: 4.5,
        design_draft: 2.8,
        displacement: 400.0,
        compartment_count: 13,
        compartment_names: vec![
            "艏尖舱".to_string(),
            "前货舱1".to_string(),
            "前货舱2".to_string(),
            "中货舱1".to_string(),
            "中货舱2".to_string(),
            "中货舱3".to_string(),
            "中货舱4".to_string(),
            "后货舱1".to_string(),
            "后货舱2".to_string(),
            "机舱".to_string(),
            "艉尖舱".to_string(),
            "淡水舱1".to_string(),
            "淡水舱2".to_string(),
        ],
        compartment_lengths: vec![
            2.5, 2.8, 2.8, 3.0, 3.0, 3.0, 3.0, 2.8, 2.8, 4.0, 2.3, 1.5, 1.5,
        ],
        compartment_volumes: vec![
            15.0, 85.0, 85.0, 95.0, 95.0, 95.0, 95.0, 85.0, 85.0, 100.0, 12.0, 20.0, 20.0,
        ],
        watertight_bulkheads: vec![
            2.5, 5.3, 8.1, 11.1, 14.1, 17.1, 20.1, 22.9, 25.7, 29.7, 32.0, 33.5,
        ],
    }
}
