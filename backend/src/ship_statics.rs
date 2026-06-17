use crate::models::*;
use std::f64::consts::PI;

const GRAVITY: f64 = 9.81;
const SEA_WATER_DENSITY: f64 = 1025.0;
const MIN_METACENTRIC_HEIGHT: f64 = 0.15;
const MAX_SAFE_HEEL_ANGLE: f64 = 15.0;

pub struct ShipHydrostatics {
    config: ShipConfig,
}

impl ShipHydrostatics {
    pub fn new(config: ShipConfig) -> Self {
        Self { config }
    }

    pub fn calculate_waterplane_area(&self, draft: f64) -> f64 {
        let l = self.config.length_overall;
        let b = self.config.beam;
        let cb = 0.75 + 0.05 * (draft / self.config.depth);
        l * b * cb
    }

    pub fn calculate_displacement(&self, draft: f64) -> f64 {
        let l = self.config.length_overall;
        let b = self.config.beam;
        let cb = 0.68 + 0.08 * (draft / self.config.depth);
        SEA_WATER_DENSITY * l * b * draft * cb
    }

    pub fn calculate_buoyancy_force(&self, draft: f64) -> f64 {
        self.calculate_displacement(draft) * GRAVITY
    }

    pub fn calculate_longitudinal_center_of_buoyancy(&self, draft: f64) -> f64 {
        self.config.length_overall * (0.5 + 0.02 * (draft / self.config.depth))
    }

    pub fn calculate_vertical_center_of_buoyancy(&self, draft: f64) -> f64 {
        draft * 0.55
    }

    pub fn calculate_metacentric_radius_bm(&self, draft: f64) -> f64 {
        let l = self.config.length_overall;
        let b = self.config.beam;
        let displacement = self.calculate_displacement(draft) / SEA_WATER_DENSITY;
        let moment_of_inertia = l * b.powi(3) / 12.0;
        moment_of_inertia / displacement
    }

    pub fn calculate_metacentric_height_gm(
        &self,
        draft: f64,
        kg: f64,
        flooded_volumes: &[f64],
    ) -> f64 {
        let kb = self.calculate_vertical_center_of_buoyancy(draft);
        let bm = self.calculate_metacentric_radius_bm(draft);
        let km = kb + bm;

        let total_flooded_volume: f64 = flooded_volumes.iter().sum();
        let displacement_volume = self.calculate_displacement(draft) / SEA_WATER_DENSITY;
        let free_surface_correction = if total_flooded_volume > 0.0 {
            let avg_compartment_beam = self.config.beam * 0.85;
            let fse = (avg_compartment_beam.powi(3) * flooded_volumes.len() as f64)
                / (12.0 * displacement_volume);
            fse.min(0.1)
        } else {
            0.0
        };

        km - kg - free_surface_correction
    }

    pub fn calculate_righting_arm(
        &self,
        heel_angle: f64,
        gm: f64,
        draft: f64,
    ) -> f64 {
        let heel_rad = heel_angle.to_radians();
        let gz = gm * heel_rad.sin();

        let max_heel = 40.0_f64.to_radians();
        let reduction_factor = if heel_angle > 30.0 {
            (1.0 - (heel_angle - 30.0) / 15.0).max(0.3)
        } else {
            1.0
        };

        gz * reduction_factor
    }

    pub fn calculate_righting_moment(&self, gz: f64, displacement: f64) -> f64 {
        displacement * GRAVITY * gz
    }

    pub fn generate_stability_curve(
        &self,
        draft: f64,
        kg: f64,
        flooded_volumes: &[f64],
    ) -> Vec<StabilityPoint> {
        let gm = self.calculate_metacentric_height_gm(draft, kg, flooded_volumes);
        let displacement = self.calculate_displacement(draft);

        (0..=90)
            .step_by(1)
            .map(|angle| {
                let heel_angle = angle as f64;
                let gz = self.calculate_righting_arm(heel_angle, gm, draft);
                let moment = self.calculate_righting_moment(gz, displacement);
                StabilityPoint {
                    heel_angle,
                    righting_arm: gz,
                    righting_moment: moment,
                }
            })
            .collect()
    }

    pub fn calculate_range_of_stability(&self, curve: &[StabilityPoint]) -> f64 {
        curve
            .iter()
            .take_while(|p| p.righting_arm > 0.0)
            .last()
            .map(|p| p.heel_angle)
            .unwrap_or(0.0)
    }

    pub fn calculate_max_righting_arm(&self, curve: &[StabilityPoint]) -> f64 {
        curve
            .iter()
            .map(|p| p.righting_arm)
            .fold(f64::NEG_INFINITY, f64::max)
    }

    pub fn calculate_equilibrium_draft(
        &self,
        flooded_compartments: &[u8],
        damage_severity: f64,
    ) -> f64 {
        let initial_draft = self.config.design_draft;
        let flooded_volume: f64 = flooded_compartments
            .iter()
            .map(|&id| {
                let idx = id as usize;
                if idx < self.config.compartment_volumes.len() {
                    self.config.compartment_volumes[idx] * damage_severity
                } else {
                    0.0
                }
            })
            .sum();

        let waterplane_area = self.calculate_waterplane_area(initial_draft);
        let additional_draft = flooded_volume / waterplane_area;

        (initial_draft + additional_draft).min(self.config.depth * 0.95)
    }

    pub fn calculate_heel_moment(
        &self,
        flooded_compartments: &[u8],
        damage_severity: f64,
        draft: f64,
    ) -> f64 {
        let beam = self.config.beam;
        let mut heel_moment = 0.0;

        for &compartment_id in flooded_compartments {
            let idx = compartment_id as usize;
            if idx >= self.config.compartment_volumes.len() {
                continue;
            }

            let volume = self.config.compartment_volumes[idx] * damage_severity;
            let lateral_offset = if idx % 2 == 0 {
                beam * 0.15
            } else {
                -beam * 0.15
            };

            heel_moment += volume * SEA_WATER_DENSITY * GRAVITY * lateral_offset;
        }

        heel_moment
    }

    pub fn calculate_equilibrium_heel(
        &self,
        heel_moment: f64,
        curve: &[StabilityPoint],
        displacement: f64,
    ) -> f64 {
        for point in curve {
            let restoring_moment = point.righting_moment;
            if restoring_moment.abs() >= heel_moment.abs() {
                return point.heel_angle * heel_moment.signum();
            }
        }
        90.0 * heel_moment.signum()
    }

    pub fn calculate_trim_angle(
        &self,
        flooded_compartments: &[u8],
        damage_severity: f64,
        draft: f64,
    ) -> f64 {
        let l = self.config.length_overall;
        let mut trim_moment = 0.0;

        for &compartment_id in flooded_compartments {
            let idx = compartment_id as usize;
            if idx >= self.config.compartment_volumes.len() {
                continue;
            }

            let volume = self.config.compartment_volumes[idx] * damage_severity;
            let longitudinal_pos = if idx == 0 {
                l * 0.1
            } else if idx == self.config.compartment_count as usize - 1 {
                l * 0.9
            } else {
                l * (0.2 + idx as f64 * 0.6 / self.config.compartment_count as f64)
            };

            let lcb = self.calculate_longitudinal_center_of_buoyancy(draft);
            let moment_arm = longitudinal_pos - lcb;
            trim_moment += volume * SEA_WATER_DENSITY * GRAVITY * moment_arm;
        }

        let mtc = self.calculate_moment_to_change_trim(draft);
        if mtc.abs() > 1e-6 {
            trim_moment / mtc
        } else {
            0.0
        }
    }

    pub fn calculate_moment_to_change_trim(&self, draft: f64) -> f64 {
        let l = self.config.length_overall;
        let displacement = self.calculate_displacement(draft) / SEA_WATER_DENSITY;
        let bml = l.powi(3) * self.config.beam / (12.0 * displacement);
        (displacement * SEA_WATER_DENSITY * GRAVITY * bml) / (100.0 * l)
    }

    pub fn calculate_reserve_buoyancy(&self, draft: f64) -> f64 {
        let total_volume =
            self.config.length_overall * self.config.beam * self.config.depth * 0.7;
        let displaced_volume = self.calculate_displacement(draft) / SEA_WATER_DENSITY;
        ((total_volume - displaced_volume) / total_volume * 100.0).max(0.0)
    }

    pub fn calculate_sinking_time(
        &self,
        flooded_compartments: &[u8],
        damage_severity: f64,
        draft: f64,
    ) -> f64 {
        let total_vol: f64 = flooded_compartments
            .iter()
            .map(|&id| {
                let idx = id as usize;
                if idx < self.config.compartment_volumes.len() {
                    self.config.compartment_volumes[idx]
                } else {
                    0.0
                }
            })
            .sum();

        let damage_area = damage_severity * 0.5;
        let head_pressure = (draft - 0.5).max(0.5);
        let flow_rate = damage_area * (2.0 * GRAVITY * head_pressure).sqrt();

        if flow_rate > 0.0 {
            total_vol / flow_rate
        } else {
            f64::INFINITY
        }
    }

    pub fn assess_safety(&self, gm: f64, heel_angle: f64, reserve_buoyancy: f64) -> bool {
        gm > MIN_METACENTRIC_HEIGHT
            && heel_angle.abs() < MAX_SAFE_HEEL_ANGLE
            && reserve_buoyancy > 10.0
    }

    pub fn simulate_damage(
        &self,
        scenario: &FloodingScenario,
    ) -> StabilityResult {
        let flooded_compartments = scenario.flooded_compartments.clone();
        let damage_severity = scenario.damage_severity;

        let draft = self.calculate_equilibrium_draft(&flooded_compartments, damage_severity);
        let kg = self.config.depth * 0.5;

        let flooded_volumes: Vec<f64> = flooded_compartments
            .iter()
            .map(|&id| {
                let idx = id as usize;
                if idx < self.config.compartment_volumes.len() {
                    self.config.compartment_volumes[idx] * damage_severity
                } else {
                    0.0
                }
            })
            .collect();

        let gm = self.calculate_metacentric_height_gm(draft, kg, &flooded_volumes);
        let stability_curve = self.generate_stability_curve(draft, kg, &flooded_volumes);
        let displacement = self.calculate_displacement(draft);

        let heel_moment = self.calculate_heel_moment(&flooded_compartments, damage_severity, draft);
        let heel_angle = self.calculate_equilibrium_heel(heel_moment, &stability_curve, displacement);
        let trim_angle = self.calculate_trim_angle(&flooded_compartments, damage_severity, draft);

        let range_of_stability = self.calculate_range_of_stability(&stability_curve);
        let righting_arm_max = self.calculate_max_righting_arm(&stability_curve);
        let reserve_buoyancy = self.calculate_reserve_buoyancy(draft);
        let sinking_time = self.calculate_sinking_time(&flooded_compartments, damage_severity, draft);
        let is_safe = self.assess_safety(gm, heel_angle, reserve_buoyancy);

        StabilityResult {
            simulation_id: uuid::Uuid::new_v4(),
            timestamp: chrono::Utc::now(),
            flooded_compartments,
            final_draft: draft,
            final_heel_angle: heel_angle,
            final_trim_angle: trim_angle,
            metacentric_height: gm,
            righting_arm_max,
            range_of_stability,
            is_safe,
            sinking_time_seconds: sinking_time,
            reserve_buoyancy,
            stability_curve,
        }
    }
}

pub fn check_alarm_conditions(result: &StabilityResult, config: &ShipConfig) -> Option<AlarmEvent> {
    if result.metacentric_height < MIN_METACENTRIC_HEIGHT {
        return Some(AlarmEvent {
            alarm_id: uuid::Uuid::new_v4(),
            ship_id: config.ship_id.clone(),
            timestamp: chrono::Utc::now(),
            alarm_type: AlarmType::StabilityLoss,
            alarm_level: AlarmLevel::Critical,
            description: format!(
                "稳性丧失警告: GM={:.3}m 低于最小值 {:.2}m",
                result.metacentric_height, MIN_METACENTRIC_HEIGHT
            ),
            flooded_compartments: result.flooded_compartments.clone(),
            metacentric_height: result.metacentric_height,
            heel_angle: result.final_heel_angle,
        });
    }

    if result.final_heel_angle.abs() > MAX_SAFE_HEEL_ANGLE {
        return Some(AlarmEvent {
            alarm_id: uuid::Uuid::new_v4(),
            ship_id: config.ship_id.clone(),
            timestamp: chrono::Utc::now(),
            alarm_type: AlarmType::HeelExcessive,
            alarm_level: AlarmLevel::Warning,
            description: format!(
                "横倾角过大警告: 横倾角={:.1}° 超过安全值 {:.1}°",
                result.final_heel_angle, MAX_SAFE_HEEL_ANGLE
            ),
            flooded_compartments: result.flooded_compartments.clone(),
            metacentric_height: result.metacentric_height,
            heel_angle: result.final_heel_angle,
        });
    }

    if result.final_draft > config.depth * 0.9 {
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

    if result.flooded_compartments.len() >= 3 {
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
