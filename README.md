# 古代水密隔舱船舶抗沉性仿真与破舱稳性分析系统

基于 Rust + Three.js 的船舶抗沉性仿真平台，集成传感器数据采集(MQTT)、静力学/破舱稳性计算、遗传算法隔舱优化、实时告警推送，通过 Docker Compose 一键编排。

## 架构图

```
                          ┌─────────────────────────────────────────────────┐
                          │                   前端 (nginx)                    │
                          │   junk_ship_3d.js (Three.js 3D渲染+GPU粒子)      │
                          │   flooding_panel.js (图表+WebSocket+告警面板)     │
                          │   Gzip 压缩静态资源  :80                           │
                          └───────────┬───────────────────┬──────────────────┘
                                      │ HTTP /api          │ WebSocket /ws
                                      ▼                    ▼
┌──────────────┐   MQTT   ┌──────────────────────────────────────────────────┐
│ 传感器模拟器  │────────▶│              Rust 后端 (静态二进制) :8080          │
│  (Python)    │  publish │  ┌─────────────┐  mpsc  ┌────────────────────┐   │
│ 破损位置可配  │  ship/+/ │  │dtu_receiver │ channel│flooding_simulator  │   │
│ 隔舱数量可配  │  sensors │  │ 传感器校验  │───────▶│ 静力学/破舱稳性    │   │
└──────────────┘          │  └──────┬──────┘        │ (液舱动量法)       │   │
                          │         │ alarm_tx      └─────────┬──────────┘   │
┌──────────────┐          │         ▼                         │ mpsc         │
│ MQTT Broker  │◀─────────│  ┌─────────────┐        ┌──────────▼─────────┐   │
│ (mosquitto)  │ subscribe│  │  alarm_ws   │        │compartment_optimizer│   │
│   :1883      │          │  │ 告警评估+WS  │        │ 遗传算法+约束优化  │   │
└──────────────┘          │  │  广播       │        └────────────────────┘   │
                          │  └──────┬──────┘                                 │
                          └─────────┼───────────────┬─────────────────────────┘
                                    │               │
                          Prometheus │ /metrics      │ 持久化
                          tracing日志│               ▼
                                    ▼        ┌──────────────────┐
                          ┌──────────────┐ │    ClickHouse     │
                          │ Prometheus   │ │  降采样MV + TTL   │
                          │   抓取       │ │  保留策略         │
                          └──────────────┘ └──────────────────┘
                                            :8123  :9000
```

## 技术栈

| 层 | 技术 |
|----|------|
| 后端 | Rust 2021 edition, actix-web 4, tokio, clickhouse-rs, rumqttc |
| 可观测性 | tracing + tracing-subscriber, prometheus |
| 通信 | tokio mpsc channel (模块间), oneshot (请求-回复), MQTT (传感器接入) |
| 数据库 | ClickHouse (列式存储, TTL + 物化视图降采样) |
| 消息 | Eclipse Mosquitto (MQTT 5) |
| 前端 | Three.js (3D + GPU粒子), Chart.js, Vite, nginx(Gzip) |
| 模拟器 | Python + paho-mqtt |
| 部署 | Docker 多阶段构建, docker-compose |

## 目录结构

```
.
├── backend/
│   ├── Cargo.toml              # 依赖: tracing/prometheus/rumqttc
│   ├── Dockerfile               # 多阶段构建, musl静态二进制
│   ├── config/
│   │   ├── ship_config.json     # 船体参数(外置JSON)
│   │   └── damage_params.json   # 破损/静力学参数(外置JSON)
│   └── src/
│       ├── main.rs              # 入口: tracing初始化+4task+MQTT订阅+路由
│       ├── metrics.rs           # Prometheus指标采集
│       ├── models.rs            # 数据模型 (含 DamageParams)
│       ├── dtu_receiver.rs      # 传感器采集校验 + MQTT订阅task
│       ├── flooding_simulator.rs# 静力学/破舱稳性计算
│       ├── compartment_optimizer.rs # 遗传算法隔舱优化
│       ├── alarm_ws.rs          # 告警评估 + WebSocket广播
│       ├── handlers.rs          # HTTP薄层handler (mpsc+oneshot)
│       └── clickhouse_client.rs # ClickHouse客户端
├── frontend/
│   ├── Dockerfile               # node构建 + nginx(Gzip)
│   ├── nginx.conf               # Gzip压缩 + 反向代理 /api /ws
│   └── src/
│       ├── junk_ship_3d.js      # 3D渲染 (ShipModel + GPU粒子系统)
│       └── flooding_panel.js     # UI面板 (图表+WS+告警)
├── simulator/
│   ├── Dockerfile
│   ├── requirements.txt
│   └── sensor_simulator.py      # MQTT传感器模拟器
├── clickhouse/
│   └── init.sql                 # 表结构+TTL保留+降采样物化视图+种子配置
├── mosquitto/
│   └── mosquitto.conf
└── docker-compose.yml           # 5服务编排
```

## 部署步骤

### 前置要求

- Docker 20.10+
- Docker Compose v2

### 一键启动

```bash
docker-compose up -d --build
```

服务启动顺序：ClickHouse(健康检查通过) → MQTT → 后端(依赖CH+MQTT) → 模拟器(依赖MQTT+后端) → 前端。

### 验证

```bash
# 健康检查
curl http://localhost:8080/health

# Prometheus 指标
curl http://localhost:8080/metrics

# 前端
打开浏览器访问 http://localhost

# ClickHouse 查询降采样数据
curl 'http://localhost:8123/?query=SELECT+*+FROM+ship_simulation.sensor_data_1min+LIMIT+5'
```

### 端口映射

| 服务 | 端口 | 说明 |
|------|------|------|
| 前端 (nginx) | 80 | Web UI + Gzip |
| 后端 (Rust) | 8080 | REST API + WebSocket + /metrics |
| ClickHouse | 8123 / 9000 | HTTP / 原生TCP |
| MQTT Broker | 1883 | 传感器数据 |

### 停止

```bash
docker-compose down          # 停止并移除容器
docker-compose down -v       # 同时删除 ClickHouse 数据卷
```

## 传感器模拟器用法

模拟器通过 MQTT 发布船舶破损后的进水过程数据，可配置破损位置与隔舱数量。

### 通过 docker-compose (默认参数)

修改 `docker-compose.yml` 中 `simulator` 服务的环境变量：

```yaml
simulator:
  environment:
    - SHIP_ID=quanzhou_song_001
    - DAMAGE_LOCATION=5        # 破损隔舱编号 (0-based, 0=艏尖舱)
    - COMPARTMENT_COUNT=13     # 隔舱数量
    - SEVERITY=0.6             # 破损严重度 0.0-1.0
    - INTERVAL=2               # 发布间隔(秒)
```

### 直接运行 (本地调试)

```bash
pip install -r simulator/requirements.txt

# 破损在第5号舱(中货舱2), 严重度0.7, 每2秒一条
python simulator/sensor_simulator.py \
    --mqtt-host localhost --mqtt-port 1883 \
    --ship-id quanzhou_song_001 \
    --damage-location 5 \
    --compartment-count 13 \
    --severity 0.7 \
    --interval 2

# 模拟机舱(9号)破损, 严重度0.9, 运行60秒后自动停止
python simulator/sensor_simulator.py \
    --damage-location 9 --severity 0.9 --duration 60
```

### 参数说明

| 参数 | 环境变量 | 默认值 | 说明 |
|------|---------|--------|------|
| `--ship-id` | SHIP_ID | quanzhou_song_001 | 船舶ID |
| `--damage-location` | DAMAGE_LOCATION | 5 | 破损隔舱编号(0-based) |
| `--compartment-count` | COMPARTMENT_COUNT | 13 | 隔舱数量 |
| `--severity` | SEVERITY | 0.5 | 破损严重度 0.0-1.0 |
| `--interval` | INTERVAL | 2 | 发布间隔(秒) |
| `--max-water` | MAX_WATER | 3.0 | 最大进水水位(m) |
| `--duration` | DURATION | 0 | 运行时长(秒), 0=无限 |
| `--mqtt-host` | MQTT_HOST | localhost | MQTT主机 |
| `--mqtt-port` | MQTT_PORT | 1883 | MQTT端口 |
| `--topic` | MQTT_TOPIC | ship/{id}/sensors | 发布主题 |

### 模拟器进水模型

- 进水水位按指数渐进 `water(t) = max_water × (1 - e^(-k·t))`，`k = severity × 0.08`
- 吃水从设计吃水(2.8m)随进水渐进上升
- 横倾角随严重度发展(最大约 severity×14°)
- 初稳性高度 GM 随进水衰减(最低降至初始值40%)
- 复原力臂 `GZ = GM × cos(heel)`

## 后端 API

| 方法 | 路径 | 说明 |
|------|------|------|
| GET | /health | 健康检查 |
| GET | /metrics | Prometheus 指标 |
| GET | /api/config/default | 默认船舶配置 |
| GET | /api/config/{ship_id} | 查询船舶配置 |
| GET | /api/sensor/{ship_id} | 最近传感器数据 |
| POST | /api/sensor | 批量上报传感器数据 |
| GET | /api/alarm/{ship_id} | 最近告警事件 |
| POST | /api/simulate | 单次破舱稳性仿真 |
| POST | /api/simulate/batch | 批量仿真 |
| POST | /api/optimize | 隔舱布局遗传算法优化 |
| GET | /ws?ship_id={id} | WebSocket 实时推送 |

## 可观测性

### 日志 (tracing)

后端使用 `tracing` + `tracing-subscriber`，通过 `RUST_LOG` 环境变量控制级别：

```bash
RUST_LOG=debug  # 调试
RUST_LOG=info   # 默认
```

### Prometheus 指标

`GET /metrics` 暴露以下指标：

| 指标 | 类型 | 说明 |
|------|------|------|
| sensor_ingested_total | Counter | 传感器数据入库总数 |
| simulations_total | Counter | 仿真执行总数 |
| alarms_total | Counter | 告警生成总数 |
| optimizations_total | Counter | 优化执行总数 |
| mqtt_messages_total | Counter | MQTT接收消息数 |
| mqtt_errors_total | Counter | MQTT连接错误数 |
| ws_connections | Gauge | 活跃WebSocket连接数 |

### ClickHouse 降采样与保留策略

`clickhouse/init.sql` 配置分层保留：

| 表 | 保留期 | 说明 |
|----|--------|------|
| sensor_data | 30 天 | 原始高频数据 |
| simulation_results | 90 天 | 仿真结果 |
| alarm_events | 90 天 | 告警事件 |
| optimization_results | 180 天 | 优化结果 |
| sensor_data_1min | 180 天 | 1分钟聚合(物化视图) |
| sensor_data_1hour | 365 天 | 1小时聚合(物化视图) |

物化视图 `mv_sensor_data_1min` / `mv_sensor_data_1hour` 自动对原始传感器数据按分钟/小时聚合(avg吃水、max水位、avg横倾等)，原始数据过期后仍可查询聚合历史。

## 配置

### 船体参数 (JSON 外置)

- `backend/config/ship_config.json` — 泉州宋代海船13舱配置(舱名、舱长、舱容、舱壁位置)
- `backend/config/damage_params.json` — 静力学参数(海水密度、渗透率、最小GM、安全横倾角等)

运行时通过环境变量 `SHIP_CONFIG_PATH` / `DAMAGE_PARAMS_PATH` 指定路径，加载失败时回退到内置默认值。

### 后端环境变量

| 变量 | 默认值 |
|------|--------|
| CLICKHOUSE_URL | tcp://localhost:9000?compression=lz4 |
| MQTT_HOST | 127.0.0.1 |
| MQTT_PORT | 1883 |
| MQTT_TOPIC | ship/+/sensors |
| SERVER_HOST | 0.0.0.0 |
| SERVER_PORT | 8080 |
| RUST_LOG | info |

## 模块间通信

Rust 后端4个模块通过 tokio mpsc channel 解耦通信：

```
HTTP handler ──oneshot──▶ mpsc channel ──▶ 模块task
   ▲                                            │
   └──────────── oneshot reply ◀────────────────┘

dtu_receiver ──alarm_tx──▶ alarm_ws ──WsServer──▶ 前端WebSocket
flooding_simulator ─alarm_tx──▶ alarm_ws
```

- handler 收到 HTTP 请求后创建 `oneshot::channel`，命令经 `mpsc::Sender` 发往对应 task
- task 处理完毕通过 oneshot 回传结果，handler 返回 HTTP 响应
- 仿真/传感器结果经 `alarm_tx` 推送到 alarm_ws，由其评估告警并广播 WebSocket
