CREATE DATABASE IF NOT EXISTS ship_simulation;

USE ship_simulation;

CREATE TABLE IF NOT EXISTS sensor_data (
    ship_id String,
    timestamp DateTime64(3, 'Asia/Shanghai'),
    compartment_id UInt8,
    water_level Float64,
    max_water_level Float64,
    is_flooded Bool,
    draft Float64,
    heel_angle Float64,
    trim_angle Float64,
    damage_location String,
    damage_severity Float64,
    metacentric_height Float64,
    righting_arm Float64
) ENGINE = MergeTree()
PARTITION BY toYYYYMM(timestamp)
ORDER BY (ship_id, timestamp, compartment_id)
TTL timestamp + INTERVAL 1 YEAR;

CREATE TABLE IF NOT EXISTS simulation_results (
    simulation_id UUID,
    ship_id String,
    timestamp DateTime64(3, 'Asia/Shanghai'),
    flooded_compartments Array(UInt8),
    final_draft Float64,
    final_heel_angle Float64,
    final_trim_angle Float64,
    metacentric_height Float64,
    righting_arm_max Float64,
    range_of_stability Float64,
    is_safe Bool,
    sinking_time_seconds Float64,
    reserve_buoyancy Float64
) ENGINE = MergeTree()
ORDER BY (simulation_id, timestamp);

CREATE TABLE IF NOT EXISTS stability_curves (
    simulation_id UUID,
    heel_angle Float64,
    righting_arm Float64,
    righting_moment Float64
) ENGINE = MergeTree()
ORDER BY (simulation_id, heel_angle);

CREATE TABLE IF NOT EXISTS alarm_events (
    alarm_id UUID,
    ship_id String,
    timestamp DateTime64(3, 'Asia/Shanghai'),
    alarm_type String,
    alarm_level String,
    description String,
    flooded_compartments Array(UInt8),
    metacentric_height Float64,
    heel_angle Float64,
    is_acknowledged Bool DEFAULT false
) ENGINE = MergeTree()
ORDER BY (ship_id, timestamp);

CREATE TABLE IF NOT EXISTS optimization_results (
    optimization_id UUID,
    ship_id String,
    timestamp DateTime64(3, 'Asia/Shanghai'),
    compartment_count UInt8,
    fitness_score Float64,
    max_flooded_compartments UInt8,
    survival_probability Float64,
    configuration Array(Float64)
) ENGINE = MergeTree()
ORDER BY (optimization_id, timestamp);

CREATE TABLE IF NOT EXISTS ship_config (
    ship_id String,
    ship_name String,
    length_overall Float64,
    beam Float64,
    depth Float64,
    design_draft Float64,
    displacement Float64,
    compartment_count UInt8,
    compartment_names Array(String),
    compartment_lengths Array(Float64),
    compartment_volumes Array(Float64),
    watertight_bulkheads Array(Float64),
    created_at DateTime DEFAULT now()
) ENGINE = ReplacingMergeTree()
ORDER BY ship_id;

INSERT INTO ship_config (
    ship_id, ship_name, length_overall, beam, depth, design_draft,
    displacement, compartment_count, compartment_names,
    compartment_lengths, compartment_volumes, watertight_bulkheads
) VALUES (
    'quanzhou_song_001',
    '泉州宋代海船',
    34.0,
    11.0,
    4.5,
    2.8,
    400.0,
    13,
    ['艏尖舱','前货舱1','前货舱2','中货舱1','中货舱2','中货舱3','中货舱4','后货舱1','后货舱2','机舱','艉尖舱','淡水舱1','淡水舱2'],
    [2.5, 2.8, 2.8, 3.0, 3.0, 3.0, 3.0, 2.8, 2.8, 4.0, 2.3, 1.5, 1.5],
    [15.0, 85.0, 85.0, 95.0, 95.0, 95.0, 95.0, 85.0, 85.0, 100.0, 12.0, 20.0, 20.0],
    [2.5, 5.3, 8.1, 11.1, 14.1, 17.1, 20.1, 22.9, 25.7, 29.7, 32.0, 33.5, 35.0]
);
