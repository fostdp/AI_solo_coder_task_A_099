mod models;
mod ship_statics;
mod genetic_algorithm;
mod clickhouse_client;
mod websocket;
mod handlers;

use actix_cors::Cors;
use actix_web::{web, App, HttpServer};
use actix::prelude::*;
use std::env;
use log::info;

use crate::clickhouse_client::ClickHouseClient;
use crate::websocket::WsServer;
use crate::handlers::*;

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let clickhouse_url = env::var("CLICKHOUSE_URL")
        .unwrap_or_else(|_| "tcp://localhost:9000?compression=lz4".to_string());
    let server_host = env::var("SERVER_HOST").unwrap_or_else(|_| "0.0.0.0".to_string());
    let server_port = env::var("SERVER_PORT").unwrap_or_else(|_| "8080".to_string());

    info!("Connecting to ClickHouse at: {}", clickhouse_url);

    let clickhouse_client = match ClickHouseClient::new(&clickhouse_url).await {
        Ok(client) => {
            info!("Successfully connected to ClickHouse");
            client
        }
        Err(e) => {
            log::warn!("Failed to connect to ClickHouse: {}. Continuing without database persistence.", e);
            return Err(std::io::Error::new(std::io::ErrorKind::ConnectionRefused, e.to_string()));
        }
    };

    let ws_server = WsServer::new().start();

    info!("Starting server on {}:{}", server_host, server_port);

    HttpServer::new(move || {
        let cors = Cors::permissive();

        App::new()
            .wrap(cors)
            .app_data(web::Data::new(clickhouse_client.clone()))
            .app_data(web::Data::new(ws_server.clone()))
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
