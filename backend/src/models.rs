use serde::{Serialize, Deserialize};
use uuid::Uuid;
use chrono::{DateTime, Utc};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShipConfig {
    pub ship_id: String,
    pub ship_name: String,
    pub length_overall: f64,
    pub beam: f64,
    pub depth: f64,
    pub design_draft: f64,
    pub displacement: f64,
    pub compartment_count: u8,
    pub compartment_names: Vec<String>,
    pub compartment_lengths: Vec<f64>,
    pub compartment_volumes: Vec<f64>,
    pub watertight_bulkheads: Vec<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SensorData {
    pub ship_id: String,
    pub timestamp: DateTime<Utc>,
    pub compartment_id: u8,
    pub water_level: f64,
    pub max_water_level: f64,
    pub is_flooded: bool,
    pub draft: f64,
    pub heel_angle: f64,
    pub trim_angle: f64,
    pub damage_location: String,
    pub damage_severity: f64,
    pub metacentric_height: f64,
    pub righting_arm: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompartmentState {
    pub compartment_id: u8,
    pub water_level: f64,
    pub volume_flooded: f64,
    pub is_flooded: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FloodingScenario {
    pub ship_id: String,
    pub flooded_compartments: Vec<u8>,
    pub damage_severity: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StabilityResult {
    pub simulation_id: Uuid,
    pub ship_id: String,
    pub timestamp: DateTime<Utc>,
    pub flooded_compartments: Vec<u8>,
    pub final_draft: f64,
    pub final_heel_angle: f64,
    pub final_trim_angle: f64,
    pub metacentric_height: f64,
    pub righting_arm_max: f64,
    pub range_of_stability: f64,
    pub is_safe: bool,
    pub sinking_time_seconds: f64,
    pub reserve_buoyancy: f64,
    pub stability_curve: Vec<StabilityPoint>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StabilityPoint {
    pub heel_angle: f64,
    pub righting_arm: f64,
    pub righting_moment: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AlarmType {
    StabilityLoss,
    FloodingSpread,
    DraftExceeded,
    HeelExcessive,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AlarmLevel {
    Info,
    Warning,
    Critical,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlarmEvent {
    pub alarm_id: Uuid,
    pub ship_id: String,
    pub timestamp: DateTime<Utc>,
    pub alarm_type: AlarmType,
    pub alarm_level: AlarmLevel,
    pub description: String,
    pub flooded_compartments: Vec<u8>,
    pub metacentric_height: f64,
    pub heel_angle: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OptimizationRequest {
    pub ship_id: String,
    pub min_compartments: u8,
    pub max_compartments: u8,
    pub population_size: usize,
    pub generations: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OptimizationResult {
    pub optimization_id: Uuid,
    pub ship_id: String,
    pub timestamp: DateTime<Utc>,
    pub compartment_count: u8,
    pub fitness_score: f64,
    pub max_flooded_compartments: u8,
    pub survival_probability: f64,
    pub configuration: Vec<f64>,
    pub best_configurations: Vec<OptimizedConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OptimizedConfig {
    pub compartment_count: u8,
    pub bulkhead_positions: Vec<f64>,
    pub fitness: f64,
    pub survival_probability: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebSocketMessage {
    pub message_type: String,
    pub data: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DamageParams {
    pub gravity: f64,
    pub sea_water_density: f64,
    pub permeability: f64,
    pub min_metacentric_height: f64,
    pub max_safe_heel_angle: f64,
    pub min_reserve_buoyancy: f64,
    pub draft_depth_ratio_threshold: f64,
    pub flooding_spread_count: usize,
    pub block_coefficient_base: f64,
    pub block_coefficient_draft_factor: f64,
    pub waterplane_coefficient_base: f64,
    pub waterplane_coefficient_draft_factor: f64,
    pub hull_form_factor: f64,
    pub damage_orifice_area_coefficient: f64,
    pub sinking_time_threshold_seconds: f64,
    pub max_safe_draft_depth_ratio: f64,
}

impl Default for DamageParams {
    fn default() -> Self {
        Self {
            gravity: 9.81,
            sea_water_density: 1025.0,
            permeability: 0.7,
            min_metacentric_height: 0.15,
            max_safe_heel_angle: 15.0,
            min_reserve_buoyancy: 10.0,
            draft_depth_ratio_threshold: 0.9,
            flooding_spread_count: 3,
            block_coefficient_base: 0.68,
            block_coefficient_draft_factor: 0.08,
            waterplane_coefficient_base: 0.75,
            waterplane_coefficient_draft_factor: 0.05,
            hull_form_factor: 0.7,
            damage_orifice_area_coefficient: 0.5,
            sinking_time_threshold_seconds: 3600.0,
            max_safe_draft_depth_ratio: 0.95,
        }
    }
}
