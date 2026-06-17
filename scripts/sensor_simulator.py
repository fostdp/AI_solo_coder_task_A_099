#!/usr/bin/env python3
"""
古代水密隔舱船舶传感器模拟器
模拟泉州宋代海船的传感器数据，每分钟上报一次
"""

import requests
import time
import random
import json
import logging
from datetime import datetime, timezone
from typing import List, Dict

logging.basicConfig(
    level=logging.INFO,
    format='%(asctime)s - %(levelname)s - %(message)s'
)
logger = logging.getLogger(__name__)

API_URL = "http://localhost:8080/api/sensor"
SHIP_ID = "quanzhou_song_001"

COMPARTMENT_NAMES = [
    "艏尖舱", "前货舱1", "前货舱2", "中货舱1", "中货舱2",
    "中货舱3", "中货舱4", "后货舱1", "后货舱2", "机舱",
    "艉尖舱", "淡水舱1", "淡水舱2"
]

COMPARTMENT_VOLUMES = [15.0, 85.0, 85.0, 95.0, 95.0, 95.0, 95.0, 85.0, 85.0, 100.0, 12.0, 20.0, 20.0]
COMPARTMENT_LENGTHS = [2.5, 2.8, 2.8, 3.0, 3.0, 3.0, 3.0, 2.8, 2.8, 4.0, 2.3, 1.5, 1.5]

DESIGN_DRAFT = 2.8
SHIP_BEAM = 11.0
SHIP_DEPTH = 4.5

class ShipSensorSimulator:
    def __init__(self, ship_id: str, api_url: str):
        self.ship_id = ship_id
        self.api_url = api_url
        self.water_levels = [0.0 for _ in COMPARTMENT_VOLUMES]
        self.base_draft = DESIGN_DRAFT
        self.base_heel = 0.0
        self.base_trim = 0.0
        self.flooded_compartments = set()
        self.damage_location = ""
        self.damage_severity = 0.0
        self.simulation_step = 0

    def calculate_max_water_level(self, compartment_idx: int) -> float:
        volume = COMPARTMENT_VOLUMES[compartment_idx]
        length = COMPARTMENT_LENGTHS[compartment_idx]
        width = SHIP_BEAM * 0.85
        return volume / (length * width)

    def induce_damage(self, compartment_ids: List[int], severity: float = 0.7):
        """模拟破损进水"""
        for cid in compartment_ids:
            if 0 <= cid < len(COMPARTMENT_NAMES):
                self.flooded_compartments.add(cid)
        self.damage_severity = severity
        if compartment_ids:
            self.damage_location = COMPARTMENT_NAMES[compartment_ids[0]]
        logger.warning(f"模拟破损: 舱室 {[COMPARTMENT_NAMES[c] for c in compartment_ids]}, 严重度: {severity}")

    def repair_damage(self):
        """修复所有破损"""
        self.flooded_compartments.clear()
        self.damage_location = ""
        self.damage_severity = 0.0
        logger.info("破损已修复")

    def update_water_levels(self):
        """更新各舱室水位"""
        for i in range(len(self.water_levels)):
            max_level = self.calculate_max_water_level(i)

            if i in self.flooded_compartments:
                inflow_rate = self.damage_severity * 0.02 * (1 + random.gauss(0, 0.1))
                self.water_levels[i] = min(self.water_levels[i] + inflow_rate, max_level * 0.95)
            else:
                if self.water_levels[i] > 0.01:
                    self.water_levels[i] = max(0.0, self.water_levels[i] - 0.005)
                else:
                    self.water_levels[i] = 0.005 + abs(random.gauss(0, 0.002))

    def calculate_hydrostatics(self):
        """计算船舶浮态和稳性参数"""
        total_flooded_volume = sum(
            self.water_levels[i] * COMPARTMENT_LENGTHS[i] * SHIP_BEAM * 0.85
            for i in self.flooded_compartments
        )

        waterplane_area = 34.0 * SHIP_BEAM * 0.75
        additional_draft = total_flooded_volume / waterplane_area
        draft = self.base_draft + additional_draft + random.gauss(0, 0.02)

        heel_moment = 0.0
        trim_moment = 0.0
        for i in self.flooded_compartments:
            volume = self.water_levels[i] * COMPARTMENT_LENGTHS[i] * SHIP_BEAM * 0.85
            lateral_offset = SHIP_BEAM * 0.15 if i % 2 == 0 else -SHIP_BEAM * 0.15
            longitudinal_pos = 34.0 * (0.1 + i * 0.6 / len(COMPARTMENT_NAMES))
            heel_moment += volume * lateral_offset
            trim_moment += volume * (longitudinal_pos - 17.0)

        heel = self.base_heel + heel_moment * 0.00008 + random.gauss(0, 0.3)
        trim = self.base_trim + trim_moment * 0.00005 + random.gauss(0, 0.1)

        displacement = 1025 * 34 * SHIP_BEAM * draft * 0.68
        kb = draft * 0.55
        bm = (34 * SHIP_BEAM ** 3 / 12) / (displacement / 1025)
        km = kb + bm
        kg = SHIP_DEPTH * 0.5

        flooded_count = len(self.flooded_compartments)
        free_surface_correction = min(0.1, (SHIP_BEAM * 0.85) ** 3 * flooded_count / (12 * displacement / 1025)) if flooded_count > 0 else 0.0

        gm = km - kg - free_surface_correction
        gm = max(gm, -0.5)

        heel_rad = abs(heel) * 3.14159 / 180
        righting_arm = gm * __import__('math').sin(heel_rad)
        if abs(heel) > 30:
            righting_arm *= max(0.3, 1 - (abs(heel) - 30) / 15)

        return draft, heel, trim, gm, righting_arm

    def generate_sensor_data(self) -> List[Dict]:
        """生成传感器数据"""
        self.simulation_step += 1
        self.update_water_levels()
        draft, heel, trim, gm, righting_arm = self.calculate_hydrostatics()

        timestamp = datetime.now(timezone.utc).isoformat()
        data = []

        for i in range(len(COMPARTMENT_NAMES)):
            max_level = self.calculate_max_water_level(i)
            is_flooded = i in self.flooded_compartments

            data.append({
                "ship_id": self.ship_id,
                "timestamp": timestamp,
                "compartment_id": i,
                "water_level": round(self.water_levels[i], 4),
                "max_water_level": round(max_level, 4),
                "is_flooded": is_flooded,
                "draft": round(draft, 4),
                "heel_angle": round(heel, 4),
                "trim_angle": round(trim, 4),
                "damage_location": self.damage_location,
                "damage_severity": round(self.damage_severity, 4),
                "metacentric_height": round(gm, 4),
                "righting_arm": round(righting_arm, 4)
            })

        return data

    def send_data(self, data: List[Dict]) -> bool:
        """发送数据到后端API"""
        try:
            response = requests.post(
                self.api_url,
                json=data,
                headers={"Content-Type": "application/json"},
                timeout=10
            )
            if response.status_code == 200:
                result = response.json()
                logger.info(f"数据发送成功: {result.get('count', 0)} 条记录")
                return True
            else:
                logger.error(f"数据发送失败: HTTP {response.status_code} - {response.text}")
                return False
        except requests.exceptions.RequestException as e:
            logger.error(f"连接失败: {e}")
            return False

    def run(self, interval: int = 60):
        """运行模拟器"""
        logger.info(f"船舶传感器模拟器启动，船名: {self.ship_id}")
        logger.info(f"上报间隔: {interval}秒")
        logger.info(f"API地址: {self.api_url}")

        step_count = 0

        while True:
            try:
                if step_count % 10 == 5 and not self.flooded_compartments:
                    num_compartments = random.choice([1, 2])
                    damaged = random.sample(range(len(COMPARTMENT_NAMES)), num_compartments)
                    severity = random.uniform(0.5, 0.9)
                    self.induce_damage(damaged, severity)

                if step_count > 0 and step_count % 15 == 0 and self.flooded_compartments:
                    self.repair_damage()

                data = self.generate_sensor_data()
                self.send_data(data)

                if self.flooded_compartments:
                    logger.info(
                        f"步骤 {self.simulation_step}: 进水舱室 {len(self.flooded_compartments)} 个, "
                        f"吃水 {data[0]['draft']:.2f}m, 横倾 {data[0]['heel_angle']:.1f}°, "
                        f"GM {data[0]['metacentric_height']:.3f}m"
                    )

                step_count += 1
                time.sleep(interval)

            except KeyboardInterrupt:
                logger.info("模拟器已停止")
                break
            except Exception as e:
                logger.error(f"模拟器错误: {e}", exc_info=True)
                time.sleep(interval)

def main():
    import argparse

    parser = argparse.ArgumentParser(description='古代水密隔舱船舶传感器模拟器')
    parser.add_argument('--ship-id', default=SHIP_ID, help='船舶ID')
    parser.add_argument('--api-url', default=API_URL, help='后端API地址')
    parser.add_argument('--interval', type=int, default=60, help='上报间隔（秒）')
    parser.add_argument('--damage-compartments', type=int, nargs='+', help='初始破损舱室ID列表')
    parser.add_argument('--damage-severity', type=float, default=0.7, help='初始破损严重度')

    args = parser.parse_args()

    simulator = ShipSensorSimulator(args.ship_id, args.api_url)

    if args.damage_compartments:
        simulator.induce_damage(args.damage_compartments, args.damage_severity)

    simulator.run(args.interval)

if __name__ == "__main__":
    main()
