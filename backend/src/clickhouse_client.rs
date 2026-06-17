use crate::models::*;
use clickhouse_rs::{Client, Pool, types::Block};
use std::sync::Arc;
use uuid::Uuid;
use chrono::{DateTime, Utc};

#[derive(Clone)]
pub struct ClickHouseClient {
    pool: Arc<Pool>,
}

impl ClickHouseClient {
    pub async fn new(url: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let pool = Pool::new(url);
        let client = Self { pool: Arc::new(pool) };
        client.ping().await?;
        Ok(client)
    }

    async fn ping(&self) -> Result<(), Box<dyn std::error::Error>> {
        let mut client = self.pool.get_handle().await?;
        client.ping().await?;
        Ok(())
    }

    pub async fn insert_sensor_data(&self, data: &[SensorData]) -> Result<(), Box<dyn std::error::Error>> {
        let mut block = Block::new();

        let mut ship_ids: Vec<String> = Vec::new();
        let mut timestamps: Vec<DateTime<Utc>> = Vec::new();
        let mut compartment_ids: Vec<u8> = Vec::new();
        let mut water_levels: Vec<f64> = Vec::new();
        let mut max_water_levels: Vec<f64> = Vec::new();
        let mut is_flooded: Vec<bool> = Vec::new();
        let mut drafts: Vec<f64> = Vec::new();
        let mut heel_angles: Vec<f64> = Vec::new();
        let mut trim_angles: Vec<f64> = Vec::new();
        let mut damage_locations: Vec<String> = Vec::new();
        let mut damage_severities: Vec<f64> = Vec::new();
        let mut metacentric_heights: Vec<f64> = Vec::new();
        let mut righting_arms: Vec<f64> = Vec::new();

        for d in data {
            ship_ids.push(d.ship_id.clone());
            timestamps.push(d.timestamp);
            compartment_ids.push(d.compartment_id);
            water_levels.push(d.water_level);
            max_water_levels.push(d.max_water_level);
            is_flooded.push(d.is_flooded);
            drafts.push(d.draft);
            heel_angles.push(d.heel_angle);
            trim_angles.push(d.trim_angle);
            damage_locations.push(d.damage_location.clone());
            damage_severities.push(d.damage_severity);
            metacentric_heights.push(d.metacentric_height);
            righting_arms.push(d.righting_arm);
        }

        block = block.add_column("ship_id", ship_ids)?;
        block = block.add_column("timestamp", timestamps)?;
        block = block.add_column("compartment_id", compartment_ids)?;
        block = block.add_column("water_level", water_levels)?;
        block = block.add_column("max_water_level", max_water_levels)?;
        block = block.add_column("is_flooded", is_flooded)?;
        block = block.add_column("draft", drafts)?;
        block = block.add_column("heel_angle", heel_angles)?;
        block = block.add_column("trim_angle", trim_angles)?;
        block = block.add_column("damage_location", damage_locations)?;
        block = block.add_column("damage_severity", damage_severities)?;
        block = block.add_column("metacentric_height", metacentric_heights)?;
        block = block.add_column("righting_arm", righting_arms)?;

        let mut client = self.pool.get_handle().await?;
        client.insert("ship_simulation.sensor_data", block).await?;
        Ok(())
    }

    pub async fn insert_simulation_result(&self, result: &StabilityResult) -> Result<(), Box<dyn std::error::Error>> {
        let mut block = Block::new();
        block = block.add_column("simulation_id", vec![result.simulation_id])?;
        block = block.add_column("ship_id", vec![result.ship_id.clone()])?;
        block = block.add_column("timestamp", vec![result.timestamp])?;
        block = block.add_column("flooded_compartments", vec![result.flooded_compartments.clone()])?;
        block = block.add_column("final_draft", vec![result.final_draft])?;
        block = block.add_column("final_heel_angle", vec![result.final_heel_angle])?;
        block = block.add_column("final_trim_angle", vec![result.final_trim_angle])?;
        block = block.add_column("metacentric_height", vec![result.metacentric_height])?;
        block = block.add_column("righting_arm_max", vec![result.righting_arm_max])?;
        block = block.add_column("range_of_stability", vec![result.range_of_stability])?;
        block = block.add_column("is_safe", vec![result.is_safe])?;
        block = block.add_column("sinking_time_seconds", vec![result.sinking_time_seconds])?;
        block = block.add_column("reserve_buoyancy", vec![result.reserve_buoyancy])?;

        let mut client = self.pool.get_handle().await?;
        client.insert("ship_simulation.simulation_results", block).await?;

        self.insert_stability_curve(result.simulation_id, &result.stability_curve).await?;
        Ok(())
    }

    pub async fn insert_stability_curve(&self, simulation_id: Uuid, curve: &[StabilityPoint]) -> Result<(), Box<dyn std::error::Error>> {
        let mut block = Block::new();

        let mut sim_ids: Vec<Uuid> = Vec::new();
        let mut heel_angles: Vec<f64> = Vec::new();
        let mut righting_arms: Vec<f64> = Vec::new();
        let mut righting_moments: Vec<f64> = Vec::new();

        for point in curve {
            sim_ids.push(simulation_id);
            heel_angles.push(point.heel_angle);
            righting_arms.push(point.righting_arm);
            righting_moments.push(point.righting_moment);
        }

        block = block.add_column("simulation_id", sim_ids)?;
        block = block.add_column("heel_angle", heel_angles)?;
        block = block.add_column("righting_arm", righting_arms)?;
        block = block.add_column("righting_moment", righting_moments)?;

        let mut client = self.pool.get_handle().await?;
        client.insert("ship_simulation.stability_curves", block).await?;
        Ok(())
    }

    pub async fn insert_alarm_event(&self, alarm: &AlarmEvent) -> Result<(), Box<dyn std::error::Error>> {
        let mut block = Block::new();
        block = block.add_column("alarm_id", vec![alarm.alarm_id])?;
        block = block.add_column("ship_id", vec![alarm.ship_id.clone()])?;
        block = block.add_column("timestamp", vec![alarm.timestamp])?;
        block = block.add_column("alarm_type", vec![alarm_type_to_string(&alarm.alarm_type)])?;
        block = block.add_column("alarm_level", vec![alarm_level_to_string(&alarm.alarm_level)])?;
        block = block.add_column("description", vec![alarm.description.clone()])?;
        block = block.add_column("flooded_compartments", vec![alarm.flooded_compartments.clone()])?;
        block = block.add_column("metacentric_height", vec![alarm.metacentric_height])?;
        block = block.add_column("heel_angle", vec![alarm.heel_angle])?;

        let mut client = self.pool.get_handle().await?;
        client.insert("ship_simulation.alarm_events", block).await?;
        Ok(())
    }

    pub async fn insert_optimization_result(&self, result: &OptimizationResult) -> Result<(), Box<dyn std::error::Error>> {
        let mut block = Block::new();
        block = block.add_column("optimization_id", vec![result.optimization_id])?;
        block = block.add_column("ship_id", vec![result.ship_id.clone()])?;
        block = block.add_column("timestamp", vec![result.timestamp])?;
        block = block.add_column("compartment_count", vec![result.compartment_count])?;
        block = block.add_column("fitness_score", vec![result.fitness_score])?;
        block = block.add_column("max_flooded_compartments", vec![result.max_flooded_compartments])?;
        block = block.add_column("survival_probability", vec![result.survival_probability])?;
        block = block.add_column("configuration", vec![result.configuration.clone()])?;

        let mut client = self.pool.get_handle().await?;
        client.insert("ship_simulation.optimization_results", block).await?;
        Ok(())
    }

    pub async fn get_ship_config(&self, ship_id: &str) -> Result<Option<ShipConfig>, Box<dyn std::error::Error>> {
        let query = format!(
            "SELECT ship_id, ship_name, length_overall, beam, depth, design_draft, displacement, \
             compartment_count, compartment_names, compartment_lengths, compartment_volumes, watertight_bulkheads \
             FROM ship_simulation.ship_config WHERE ship_id = '{}' LIMIT 1",
            ship_id
        );

        let mut client = self.pool.get_handle().await?;
        let rows = client.query(&query).fetch_all().await?;

        if rows.is_empty() {
            return Ok(None);
        }

        let row = &rows[0];
        Ok(Some(ShipConfig {
            ship_id: row.get("ship_id")?,
            ship_name: row.get("ship_name")?,
            length_overall: row.get("length_overall")?,
            beam: row.get("beam")?,
            depth: row.get("depth")?,
            design_draft: row.get("design_draft")?,
            displacement: row.get("displacement")?,
            compartment_count: row.get("compartment_count")?,
            compartment_names: row.get("compartment_names")?,
            compartment_lengths: row.get("compartment_lengths")?,
            compartment_volumes: row.get("compartment_volumes")?,
            watertight_bulkheads: row.get("watertight_bulkheads")?,
        }))
    }

    pub async fn get_recent_sensor_data(&self, ship_id: &str, limit: u32) -> Result<Vec<SensorData>, Box<dyn std::error::Error>> {
        let query = format!(
            "SELECT ship_id, timestamp, compartment_id, water_level, max_water_level, is_flooded, \
             draft, heel_angle, trim_angle, damage_location, damage_severity, metacentric_height, righting_arm \
             FROM ship_simulation.sensor_data WHERE ship_id = '{}' \
             ORDER BY timestamp DESC LIMIT {}",
            ship_id, limit
        );

        let mut client = self.pool.get_handle().await?;
        let rows = client.query(&query).fetch_all().await?;

        let mut result = Vec::new();
        for row in rows.iter() {
            result.push(SensorData {
                ship_id: row.get("ship_id")?,
                timestamp: row.get("timestamp")?,
                compartment_id: row.get("compartment_id")?,
                water_level: row.get("water_level")?,
                max_water_level: row.get("max_water_level")?,
                is_flooded: row.get("is_flooded")?,
                draft: row.get("draft")?,
                heel_angle: row.get("heel_angle")?,
                trim_angle: row.get("trim_angle")?,
                damage_location: row.get("damage_location")?,
                damage_severity: row.get("damage_severity")?,
                metacentric_height: row.get("metacentric_height")?,
                righting_arm: row.get("righting_arm")?,
            });
        }

        Ok(result)
    }

    pub async fn get_recent_alarms(&self, ship_id: &str, limit: u32) -> Result<Vec<AlarmEvent>, Box<dyn std::error::Error>> {
        let query = format!(
            "SELECT alarm_id, ship_id, timestamp, alarm_type, alarm_level, description, \
             flooded_compartments, metacentric_height, heel_angle \
             FROM ship_simulation.alarm_events WHERE ship_id = '{}' \
             ORDER BY timestamp DESC LIMIT {}",
            ship_id, limit
        );

        let mut client = self.pool.get_handle().await?;
        let rows = client.query(&query).fetch_all().await?;

        let mut result = Vec::new();
        for row in rows.iter() {
            let alarm_type_str: String = row.get("alarm_type")?;
            let alarm_level_str: String = row.get("alarm_level")?;

            result.push(AlarmEvent {
                alarm_id: row.get("alarm_id")?,
                ship_id: row.get("ship_id")?,
                timestamp: row.get("timestamp")?,
                alarm_type: string_to_alarm_type(&alarm_type_str),
                alarm_level: string_to_alarm_level(&alarm_level_str),
                description: row.get("description")?,
                flooded_compartments: row.get("flooded_compartments")?,
                metacentric_height: row.get("metacentric_height")?,
                heel_angle: row.get("heel_angle")?,
            });
        }

        Ok(result)
    }
}

fn alarm_type_to_string(alarm_type: &AlarmType) -> String {
    match alarm_type {
        AlarmType::StabilityLoss => "StabilityLoss",
        AlarmType::FloodingSpread => "FloodingSpread",
        AlarmType::DraftExceeded => "DraftExceeded",
        AlarmType::HeelExcessive => "HeelExcessive",
    }.to_string()
}

fn alarm_level_to_string(alarm_level: &AlarmLevel) -> String {
    match alarm_level {
        AlarmLevel::Info => "Info",
        AlarmLevel::Warning => "Warning",
        AlarmLevel::Critical => "Critical",
    }.to_string()
}

fn string_to_alarm_type(s: &str) -> AlarmType {
    match s {
        "StabilityLoss" => AlarmType::StabilityLoss,
        "FloodingSpread" => AlarmType::FloodingSpread,
        "DraftExceeded" => AlarmType::DraftExceeded,
        "HeelExcessive" => AlarmType::HeelExcessive,
        _ => AlarmType::StabilityLoss,
    }
}

fn string_to_alarm_level(s: &str) -> AlarmLevel {
    match s {
        "Info" => AlarmLevel::Info,
        "Warning" => AlarmLevel::Warning,
        "Critical" => AlarmLevel::Critical,
        _ => AlarmLevel::Warning,
    }
}
