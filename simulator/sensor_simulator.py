#!/usr/bin/env python3
"""
古代海船传感器模拟器

通过 MQTT 向后端发布传感器数据，模拟船舶破损后进水过程。
可配置破损位置(隔舱编号)、隔舱数量、破损严重度、发布间隔。

用法:
  python sensor_simulator.py --ship-id quanzhou_song_001 \
      --damage-location 5 --compartment-count 13 --severity 0.6 --interval 2
"""

import argparse
import json
import math
import os
import random
import signal
import sys
import time
from datetime import datetime, timezone

import paho.mqtt.client as mqtt

DEFAULT_COMPARTMENT_NAMES = [
    "艏尖舱", "前货舱1", "前货舱2", "中货舱1", "中货舱2",
    "中货舱3", "中货舱4", "后货舱1", "后货舱2", "机舱",
    "艉尖舱", "淡水舱1", "淡水舱2",
]

DESIGN_DRAFT = 2.8
SHIP_DEPTH = 4.5
INITIAL_GM = 0.85


def now_iso():
    return datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%S.%fZ")


def build_payload(args, elapsed, compartment_names):
    damage_idx = args.damage_location
    severity = args.severity

    k = severity * 0.08
    progress = 1.0 - math.exp(-k * elapsed)

    max_water = args.max_water
    water_level = max_water * progress

    flooded_draft = DESIGN_DRAFT + severity * 1.6
    draft = DESIGN_DRAFT + (flooded_draft - DESIGN_DRAFT) * progress

    max_heel = severity * 14.0
    heel = max_heel * progress + random.uniform(-0.3, 0.3)

    max_trim = severity * 2.5
    trim = max_trim * progress + random.uniform(-0.1, 0.1)

    gm = INITIAL_GM * (1.0 - 0.6 * progress)
    righting_arm = max(0.0, gm * math.cos(math.radians(heel)))

    is_flooded = water_level > 0.5

    if damage_idx < len(compartment_names):
        damage_location = compartment_names[damage_idx]
    else:
        damage_location = f"compartment_{damage_idx}"

    return {
        "ship_id": args.ship_id,
        "timestamp": now_iso(),
        "compartment_id": damage_idx,
        "water_level": round(water_level, 3),
        "max_water_level": max_water,
        "is_flooded": is_flooded,
        "draft": round(draft, 3),
        "heel_angle": round(heel, 2),
        "trim_angle": round(trim, 2),
        "damage_location": damage_location,
        "damage_severity": severity,
        "metacentric_height": round(gm, 3),
        "righting_arm": round(righting_arm, 3),
    }


def main():
    parser = argparse.ArgumentParser(description="古代海船传感器模拟器 (MQTT)")
    parser.add_argument("--ship-id", default=os.getenv("SHIP_ID", "quanzhou_song_001"))
    parser.add_argument("--damage-location", type=int,
                        default=int(os.getenv("DAMAGE_LOCATION", "5")),
                        help="破损隔舱编号 (0-based)")
    parser.add_argument("--compartment-count", type=int,
                        default=int(os.getenv("COMPARTMENT_COUNT", "13")))
    parser.add_argument("--severity", type=float,
                        default=float(os.getenv("SEVERITY", "0.5")),
                        help="破损严重度 0.0-1.0")
    parser.add_argument("--interval", type=float,
                        default=float(os.getenv("INTERVAL", "2")),
                        help="发布间隔(秒)")
    parser.add_argument("--max-water", type=float,
                        default=float(os.getenv("MAX_WATER", "3.0")),
                        help="最大进水水位(m)")
    parser.add_argument("--duration", type=float,
                        default=float(os.getenv("DURATION", "0")),
                        help="模拟时长(秒), 0=无限")
    parser.add_argument("--mqtt-host", default=os.getenv("MQTT_HOST", "localhost"))
    parser.add_argument("--mqtt-port", type=int,
                        default=int(os.getenv("MQTT_PORT", "1883")))
    parser.add_argument("--topic", default=os.getenv("MQTT_TOPIC", None),
                        help="MQTT主题, 默认 ship/{ship_id}/sensors")
    args = parser.parse_args()

    if args.severity < 0 or args.severity > 1:
        print("severity 必须在 0.0-1.0 之间", file=sys.stderr)
        sys.exit(1)

    compartment_names = DEFAULT_COMPARTMENT_NAMES[:args.compartment_count]
    if args.compartment_count > len(DEFAULT_COMPARTMENT_NAMES):
        for i in range(len(DEFAULT_COMPARTMENT_NAMES), args.compartment_count):
            compartment_names.append(f"舱{i}")

    topic = args.topic or f"ship/{args.ship_id}/sensors"

    client = mqtt.Client(client_id=f"sim-{args.ship_id}-{random.randint(1000, 9999)}")
    client.connect(args.mqtt_host, args.mqtt_port, 60)
    client.loop_start()

    print(f"[模拟器] 已连接 MQTT {args.mqtt_host}:{args.mqtt_port}")
    print(f"[模拟器] 主题: {topic}")
    print(f"[模拟器] 船舶={args.ship_id} 破舱位置={args.damage_location}"
          f"({compartment_names[min(args.damage_location, len(compartment_names)-1)]}) "
          f"严重度={args.severity} 间隔={args.interval}s")

    running = [True]

    def stop(signum, frame):
        running[0] = False
    signal.signal(signal.SIGINT, stop)
    signal.signal(signal.SIGTERM, stop)

    start = time.time()
    count = 0
    try:
        while running[0]:
            elapsed = time.time() - start
            if args.duration > 0 and elapsed >= args.duration:
                break
            payload = build_payload(args, elapsed, compartment_names)
            info = client.publish(topic, json.dumps(payload, ensure_ascii=False), qos=1)
            info.wait_for_publish(timeout=5)
            count += 1
            print(f"[{count}] t={elapsed:6.1f}s draft={payload['draft']:.2f}m "
                  f"heel={payload['heel_angle']:.1f}° water={payload['water_level']:.2f}m "
                  f"gm={payload['metacentric_height']:.2f} flooded={payload['is_flooded']}")
            time.sleep(args.interval)
    finally:
        client.loop_stop()
        client.disconnect()
        print(f"[模拟器] 已停止, 共发布 {count} 条数据")


if __name__ == "__main__":
    main()
