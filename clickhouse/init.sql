-- ClickHouse 初始化: 表结构 + TTL保留策略 + 降采样物化视图 + 种子配置

CREATE DATABASE IF NOT EXISTS ship_simulation;

-- ===== 船舶配置表 =====
CREATE TABLE IF NOT EXISTS ship_simulation.ship_config (
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
    watertight_bulkheads Array(Float64)
) ENGINE = MergeTree
ORDER BY ship_id;

-- ===== 传感器原始数据表 (保留30天) =====
CREATE TABLE IF NOT EXISTS ship_simulation.sensor_data (
    ship_id String,
    timestamp DateTime,
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
) ENGINE = MergeTree
PARTITION BY toYYYYMM(timestamp)
ORDER BY (ship_id, timestamp)
TTL timestamp + INTERVAL 30 DAY;

-- ===== 仿真结果表 (保留90天) =====
CREATE TABLE IF NOT EXISTS ship_simulation.simulation_results (
    simulation_id String,
    ship_id String,
    timestamp DateTime,
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
) ENGINE = MergeTree
PARTITION BY toYYYYMM(timestamp)
ORDER BY (ship_id, timestamp)
TTL timestamp + INTERVAL 90 DAY;

-- ===== 稳性曲线表 (无时间列, 按仿真ID关联, 不单独设TTL) =====
CREATE TABLE IF NOT EXISTS ship_simulation.stability_curves (
    simulation_id String,
    heel_angle Float64,
    righting_arm Float64,
    righting_moment Float64
) ENGINE = MergeTree
ORDER BY (simulation_id, heel_angle);

-- ===== 告警事件表 (保留90天) =====
CREATE TABLE IF NOT EXISTS ship_simulation.alarm_events (
    alarm_id String,
    ship_id String,
    timestamp DateTime,
    alarm_type String,
    alarm_level String,
    description String,
    flooded_compartments Array(UInt8),
    metacentric_height Float64,
    heel_angle Float64
) ENGINE = MergeTree
PARTITION BY toYYYYMM(timestamp)
ORDER BY (ship_id, timestamp)
TTL timestamp + INTERVAL 90 DAY;

-- ===== 优化结果表 (保留180天) =====
CREATE TABLE IF NOT EXISTS ship_simulation.optimization_results (
    optimization_id String,
    ship_id String,
    timestamp DateTime,
    compartment_count UInt8,
    fitness_score Float64,
    max_flooded_compartments UInt8,
    survival_probability Float64,
    configuration Array(Float64)
) ENGINE = MergeTree
PARTITION BY toYYYYMM(timestamp)
ORDER BY (ship_id, timestamp)
TTL timestamp + INTERVAL 180 DAY;

-- ===== 降采样: 1分钟聚合 (保留180天) =====
CREATE TABLE IF NOT EXISTS ship_simulation.sensor_data_1min (
    ship_id String,
    bucket DateTime,
    avg_draft Float64,
    max_water_level Float64,
    avg_heel_angle Float64,
    avg_metacentric_height Float64,
    flooded_count UInt32,
    sample_count UInt32
) ENGINE = MergeTree
PARTITION BY toYYYYMM(bucket)
ORDER BY (ship_id, bucket)
TTL bucket + INTERVAL 180 DAY;

CREATE MATERIALIZED VIEW IF NOT EXISTS ship_simulation.mv_sensor_data_1min
TO ship_simulation.sensor_data_1min
AS SELECT
    ship_id,
    toStartOfMinute(timestamp) AS bucket,
    avg(draft) AS avg_draft,
    max(water_level) AS max_water_level,
    avg(heel_angle) AS avg_heel_angle,
    avg(metacentric_height) AS avg_metacentric_height,
    countIf(is_flooded) AS flooded_count,
    count() AS sample_count
FROM ship_simulation.sensor_data
GROUP BY ship_id, bucket;

-- ===== 降采样: 1小时聚合 (保留365天) =====
CREATE TABLE IF NOT EXISTS ship_simulation.sensor_data_1hour (
    ship_id String,
    bucket DateTime,
    avg_draft Float64,
    max_water_level Float64,
    avg_heel_angle Float64,
    avg_metacentric_height Float64,
    flooded_count UInt32,
    sample_count UInt32
) ENGINE = MergeTree
PARTITION BY toYYYYMM(bucket)
ORDER BY (ship_id, bucket)
TTL bucket + INTERVAL 365 DAY;

CREATE MATERIALIZED VIEW IF NOT EXISTS ship_simulation.mv_sensor_data_1hour
TO ship_simulation.sensor_data_1hour
AS SELECT
    ship_id,
    toStartOfHour(timestamp) AS bucket,
    avg(draft) AS avg_draft,
    max(water_level) AS max_water_level,
    avg(heel_angle) AS avg_heel_angle,
    avg(metacentric_height) AS avg_metacentric_height,
    countIf(is_flooded) AS flooded_count,
    count() AS sample_count
FROM ship_simulation.sensor_data
GROUP BY ship_id, bucket;

-- ===== 种子: 默认船舶配置 (泉州宋代海船) =====
INSERT INTO ship_simulation.ship_config VALUES (
    'quanzhou_song_001',
    '泉州宋代海船',
    34.0, 11.0, 4.5, 2.8, 400.0, 13,
    ['艏尖舱','前货舱1','前货舱2','中货舱1','中货舱2','中货舱3','中货舱4','后货舱1','后货舱2','机舱','艉尖舱','淡水舱1','淡水舱2'],
    [2.5,2.8,2.8,3.0,3.0,3.0,3.0,2.8,2.8,4.0,2.3,1.5,1.5],
    [15.0,85.0,85.0,95.0,95.0,95.0,95.0,85.0,85.0,100.0,12.0,20.0,20.0],
    [2.5,5.3,8.1,11.1,14.1,17.1,20.1,22.9,25.7,29.7,32.0,33.5]
);
