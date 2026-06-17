import * as THREE from 'three';
import { OrbitControls } from 'three/examples/jsm/controls/OrbitControls.js';
import { Chart, registerables } from 'chart.js';
import { ShipModel } from './shipModel.js';

Chart.register(...registerables);

const API_BASE = 'http://localhost:8080';
const WS_BASE = 'ws://localhost:8080/ws';

const COMPARTMENT_NAMES = [
    "艏尖舱", "前货舱1", "前货舱2", "中货舱1", "中货舱2",
    "中货舱3", "中货舱4", "后货舱1", "后货舱2", "机舱",
    "艉尖舱", "淡水舱1", "淡水舱2"
];

class ShipSimulationApp {
    constructor() {
        this.scene = null;
        this.camera = null;
        this.renderer = null;
        this.controls = null;
        this.shipModel = null;
        this.stabilityChart = null;
        this.draftChart = null;
        this.waterChart = null;
        this.ws = null;
        this.currentShipId = 'quanzhou_song_001';
        this.draftHistory = [];
        this.timeLabels = [];
        this.waterLevelHistory = {};
        this.alarms = [];
        this.latestSensorData = null;
        this.latestSimulationResult = null;

        this.init();
    }

    init() {
        this.initThreeJS();
        this.initCharts();
        this.initWebSocket();
        this.initUI();
        this.loadShipConfig();
        this.animate();
    }

    initThreeJS() {
        const container = document.getElementById('canvas-container');

        this.scene = new THREE.Scene();
        this.scene.background = new THREE.Color(0x87ceeb);
        this.scene.fog = new THREE.Fog(0x87ceeb, 50, 200);

        this.camera = new THREE.PerspectiveCamera(
            60,
            container.clientWidth / container.clientHeight,
            0.1,
            1000
        );
        this.camera.position.set(40, 25, 40);

        this.renderer = new THREE.WebGLRenderer({ antialias: true });
        this.renderer.setSize(container.clientWidth, container.clientHeight);
        this.renderer.setPixelRatio(window.devicePixelRatio);
        this.renderer.shadowMap.enabled = true;
        this.renderer.shadowMap.type = THREE.PCFSoftShadowMap;
        container.appendChild(this.renderer.domElement);

        this.controls = new OrbitControls(this.camera, this.renderer.domElement);
        this.controls.enableDamping = true;
        this.controls.dampingFactor = 0.05;
        this.controls.minDistance = 20;
        this.controls.maxDistance = 100;
        this.controls.maxPolarAngle = Math.PI / 2 - 0.1;

        const ambientLight = new THREE.AmbientLight(0xffffff, 0.6);
        this.scene.add(ambientLight);

        const directionalLight = new THREE.DirectionalLight(0xffffff, 0.8);
        directionalLight.position.set(50, 100, 50);
        directionalLight.castShadow = true;
        directionalLight.shadow.mapSize.width = 2048;
        directionalLight.shadow.mapSize.height = 2048;
        directionalLight.shadow.camera.near = 0.5;
        directionalLight.shadow.camera.far = 500;
        directionalLight.shadow.camera.left = -100;
        directionalLight.shadow.camera.right = 100;
        directionalLight.shadow.camera.top = 100;
        directionalLight.shadow.camera.bottom = -100;
        this.scene.add(directionalLight);

        const hemisphereLight = new THREE.HemisphereLight(0x87ceeb, 0x3d5c5c, 0.4);
        this.scene.add(hemisphereLight);

        this.shipModel = new ShipModel(this.scene);
        this.clock = new THREE.Clock();

        window.addEventListener('resize', () => this.onWindowResize());

        this.initCompartmentList();
    }

    initCharts() {
        const chartOptions = {
            responsive: true,
            maintainAspectRatio: false,
            plugins: {
                legend: {
                    labels: {
                        color: '#e8e8e8',
                        font: { size: 11 }
                    }
                }
            },
            scales: {
                x: {
                    ticks: { color: '#888', font: { size: 10 } },
                    grid: { color: 'rgba(255,255,255,0.1)' }
                },
                y: {
                    ticks: { color: '#888', font: { size: 10 } },
                    grid: { color: 'rgba(255,255,255,0.1)' }
                }
            }
        };

        this.stabilityChart = new Chart(
            document.getElementById('stability-chart'),
            {
                type: 'line',
                data: {
                    labels: Array.from({ length: 91 }, (_, i) => i),
                    datasets: [{
                        label: '复原力臂 GZ (m)',
                        data: [],
                        borderColor: '#4facfe',
                        backgroundColor: 'rgba(79, 172, 254, 0.2)',
                        fill: true,
                        tension: 0.3,
                        pointRadius: 0
                    }]
                },
                options: {
                    ...chartOptions,
                    scales: {
                        ...chartOptions.scales,
                        x: { ...chartOptions.scales.x, title: { display: true, text: '横倾角 (°)', color: '#ffd700' } },
                        y: { ...chartOptions.scales.y, title: { display: true, text: '复原力臂 (m)', color: '#ffd700' } }
                    }
                }
            }
        );

        this.draftChart = new Chart(
            document.getElementById('draft-chart'),
            {
                type: 'line',
                data: {
                    labels: [],
                    datasets: [{
                        label: '吃水深度 (m)',
                        data: [],
                        borderColor: '#ff6b6b',
                        backgroundColor: 'rgba(255, 107, 107, 0.2)',
                        fill: true,
                        tension: 0.4
                    }, {
                        label: '横倾角 (°)',
                        data: [],
                        borderColor: '#ffd700',
                        backgroundColor: 'rgba(255, 215, 0, 0.1)',
                        fill: false,
                        tension: 0.4,
                        yAxisID: 'y1'
                    }]
                },
                options: {
                    ...chartOptions,
                    scales: {
                        ...chartOptions.scales,
                        x: { ...chartOptions.scales.x, title: { display: true, text: '时间', color: '#ffd700' } },
                        y: { ...chartOptions.scales.y, title: { display: true, text: '吃水 (m)', color: '#ffd700' } },
                        y1: {
                            position: 'right',
                            title: { display: true, text: '横倾角 (°)', color: '#ffd700' },
                            ticks: { color: '#888', font: { size: 10 } },
                            grid: { drawOnChartArea: false }
                        }
                    }
                }
            }
        );

        this.waterChart = new Chart(
            document.getElementById('water-chart'),
            {
                type: 'bar',
                data: {
                    labels: COMPARTMENT_NAMES,
                    datasets: [{
                        label: '水位 (m)',
                        data: new Array(COMPARTMENT_NAMES.length).fill(0),
                        backgroundColor: COMPARTMENT_NAMES.map(() => 'rgba(79, 172, 254, 0.6)'),
                        borderColor: '#4facfe',
                        borderWidth: 1
                    }]
                },
                options: {
                    ...chartOptions,
                    scales: {
                        ...chartOptions.scales,
                        x: {
                            ...chartOptions.scales.x,
                            ticks: { ...chartOptions.scales.x.ticks, maxRotation: 45, minRotation: 45 }
                        },
                        y: {
                            ...chartOptions.scales.y,
                            title: { display: true, text: '水位 (m)', color: '#ffd700' },
                            beginAtZero: true
                        }
                    }
                }
            }
        );

        this.generateInitialStabilityCurve();
    }

    generateInitialStabilityCurve() {
        const gm = 0.5;
        const curveData = [];
        for (let angle = 0; angle <= 90; angle++) {
            const rad = angle * Math.PI / 180;
            let gz = gm * Math.sin(rad);
            if (angle > 30) {
                gz *= Math.max(0.3, 1 - (angle - 30) / 15);
            }
            curveData.push(Math.max(0, gz));
        }
        this.stabilityChart.data.datasets[0].data = curveData;
        this.stabilityChart.update();
    }

    initWebSocket() {
        const wsUrl = `${WS_BASE}?ship_id=${this.currentShipId}`;
        this.ws = new WebSocket(wsUrl);

        this.ws.onopen = () => {
            console.log('WebSocket connected');
            this.updateConnectionStatus(true);
            this.ws.send(JSON.stringify({
                message_type: 'subscribe',
                data: this.currentShipId
            }));
        };

        this.ws.onmessage = (event) => {
            try {
                const msg = JSON.parse(event.data);
                this.handleWebSocketMessage(msg);
            } catch (e) {
                console.error('WebSocket message parse error:', e);
            }
        };

        this.ws.onclose = () => {
            console.log('WebSocket disconnected');
            this.updateConnectionStatus(false);
            setTimeout(() => this.initWebSocket(), 3000);
        };

        this.ws.onerror = (error) => {
            console.error('WebSocket error:', error);
            this.updateConnectionStatus(false);
        };
    }

    handleWebSocketMessage(msg) {
        switch (msg.message_type) {
            case 'sensor_data':
                this.handleSensorData(msg.data);
                break;
            case 'simulation_result':
                this.handleSimulationResult(msg.data);
                break;
            case 'alarm':
                this.handleAlarm(msg.data);
                break;
        }
    }

    handleSensorData(data) {
        if (!Array.isArray(data) || data.length === 0) return;

        this.latestSensorData = data;

        const first = data[0];
        this.updateStatusDisplay(first);
        this.updateCompartmentStates(data);
        this.updateWaterChart(data);
        this.updateDraftHistory(first);

        data.forEach(d => {
            this.shipModel.updateWaterLevel(d.compartment_id, d.water_level, d.max_water_level);
            this.shipModel.setFlooded(d.compartment_id, d.is_flooded);
        });

        this.shipModel.updateShipPose(first.draft, first.heel_angle, first.trim_angle);
    }

    handleSimulationResult(result) {
        this.latestSimulationResult = result;

        this.updateStatusDisplay({
            draft: result.final_draft,
            heel_angle: result.final_heel_angle,
            trim_angle: result.final_trim_angle,
            metacentric_height: result.metacentric_height,
            righting_arm: result.righting_arm_max
        });

        if (result.stability_curve) {
            this.updateStabilityChart(result.stability_curve);
        }

        document.getElementById('buoyancy-value').textContent = `${result.reserve_buoyancy?.toFixed(1) || '0.0'}%`;
        const buoyancyEl = document.getElementById('buoyancy-value');
        buoyancyEl.className = 'value ' + (result.reserve_buoyancy > 20 ? 'success' : result.reserve_buoyancy > 10 ? '' : 'danger');

        if (result.is_safe) {
            document.getElementById('ship-name').className = 'status-item connected';
        } else {
            document.getElementById('ship-name').className = 'status-item warning';
        }

        if (result.flooded_compartments) {
            const targetLevels = result.flooded_compartments.map(() => 2.5);
            this.shipModel.startFloodingAnimation(result.flooded_compartments, targetLevels, 5000);
        }
    }

    handleAlarm(alarm) {
        this.alarms.unshift(alarm);
        if (this.alarms.length > 20) this.alarms.pop();

        this.updateAlarmDisplay();
        this.playAlarmSound();
    }

    updateAlarmDisplay() {
        const container = document.getElementById('alarms-panel');

        if (this.alarms.length === 0) {
            container.innerHTML = '<div style="color: #888; font-size: 12px; text-align: center; padding: 20px;">暂无告警</div>';
            return;
        }

        container.innerHTML = this.alarms.map(alarm => {
            const levelClass = alarm.alarm_level === 'Critical' ? 'critical' : 'warning';
            const time = new Date(alarm.timestamp).toLocaleTimeString();
            return `
                <div class="alarm-item ${levelClass}">
                    <div style="font-weight: bold; margin-bottom: 3px;">
                        [${time}] ${this.getAlarmTypeName(alarm.alarm_type)}
                    </div>
                    <div>${alarm.description}</div>
                </div>
            `;
        }).join('');
    }

    getAlarmTypeName(type) {
        const names = {
            'StabilityLoss': '稳性丧失',
            'FloodingSpread': '进水蔓延',
            'DraftExceeded': '吃水超限',
            'HeelExcessive': '横倾过大'
        };
        return names[type] || type;
    }

    playAlarmSound() {
        const audioContext = new (window.AudioContext || window.webkitAudioContext)();
        const oscillator = audioContext.createOscillator();
        const gainNode = audioContext.createGain();

        oscillator.connect(gainNode);
        gainNode.connect(audioContext.destination);

        oscillator.frequency.value = 800;
        oscillator.type = 'square';
        gainNode.gain.setValueAtTime(0.1, audioContext.currentTime);
        gainNode.gain.exponentialRampToValueAtTime(0.01, audioContext.currentTime + 0.3);

        oscillator.start(audioContext.currentTime);
        oscillator.stop(audioContext.currentTime + 0.3);
    }

    initUI() {
        document.getElementById('simulate-btn').addEventListener('click', () => this.runSimulation());
        document.getElementById('reset-btn').addEventListener('click', () => this.resetShip());
        document.getElementById('animate-btn').addEventListener('click', () => this.playDemoAnimation());
        document.getElementById('optimize-btn').addEventListener('click', () => this.runOptimization());

        document.getElementById('view-side').addEventListener('click', () => this.setView('side'));
        document.getElementById('view-top').addEventListener('click', () => this.setView('top'));
        document.getElementById('view-3d').addEventListener('click', () => this.setView('3d'));
    }

    async loadShipConfig() {
        try {
            const response = await fetch(`${API_BASE}/api/config/default`);
            const config = await response.json();
            console.log('Ship config loaded:', config);
        } catch (e) {
            console.error('Failed to load ship config:', e);
        }
    }

    async runSimulation() {
        const compartmentsInput = document.getElementById('damage-compartments').value;
        const severity = parseFloat(document.getElementById('damage-severity').value);

        if (!compartmentsInput.trim()) {
            alert('请输入破损舱室ID');
            return;
        }

        const compartments = compartmentsInput.split(',').map(s => parseInt(s.trim())).filter(n => !isNaN(n));

        if (compartments.length === 0) {
            alert('请输入有效的舱室ID');
            return;
        }

        try {
            const response = await fetch(`${API_BASE}/api/simulate`, {
                method: 'POST',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify({
                    ship_id: this.currentShipId,
                    flooded_compartments: compartments,
                    damage_severity: severity
                })
            });

            const result = await response.json();
            this.handleSimulationResult(result);
        } catch (e) {
            console.error('Simulation failed:', e);
            alert('仿真失败，请检查后端服务是否运行');
        }
    }

    async runOptimization() {
        const minCompartments = parseInt(document.getElementById('min-compartments').value);
        const maxCompartments = parseInt(document.getElementById('max-compartments').value);

        if (minCompartments >= maxCompartments) {
            alert('最小舱数必须小于最大舱数');
            return;
        }

        const btn = document.getElementById('optimize-btn');
        const originalText = btn.textContent;
        btn.textContent = '优化中...';
        btn.disabled = true;

        try {
            const response = await fetch(`${API_BASE}/api/optimize`, {
                method: 'POST',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify({
                    ship_id: this.currentShipId,
                    min_compartments: minCompartments,
                    max_compartments: maxCompartments,
                    population_size: 30,
                    generations: 50
                })
            });

            const result = await response.json();
            console.log('Optimization result:', result);

            if (result.configuration && result.configuration.length > 0) {
                this.shipModel.setCompartmentConfiguration(result.configuration);
                alert(`优化完成！\n最优舱数: ${result.compartment_count}\n适应度: ${result.fitness_score.toFixed(4)}\n生存概率: ${(result.survival_probability * 100).toFixed(1)}%`);
            }
        } catch (e) {
            console.error('Optimization failed:', e);
            alert('优化失败，请检查后端服务是否运行');
        } finally {
            btn.textContent = originalText;
            btn.disabled = false;
        }
    }

    resetShip() {
        this.shipModel.reset();
        this.alarms = [];
        this.updateAlarmDisplay();
        this.draftHistory = [];
        this.timeLabels = [];
        this.updateDraftChart();
        this.generateInitialStabilityCurve();

        document.getElementById('ship-name').className = 'status-item';

        ['draft-value', 'heel-value', 'trim-value', 'gm-value', 'gz-value', 'buoyancy-value'].forEach(id => {
            document.getElementById(id).className = 'value';
        });

        document.getElementById('draft-value').textContent = '2.80 m';
        document.getElementById('heel-value').textContent = '0.00°';
        document.getElementById('trim-value').textContent = '0.00°';
        document.getElementById('gm-value').textContent = '0.50 m';
        document.getElementById('gz-value').textContent = '0.00 m';
        document.getElementById('buoyancy-value').textContent = '35.0%';

        this.initCompartmentList();
    }

    playDemoAnimation() {
        const compartments = [3, 4];
        const levels = [2.0, 2.5];
        this.shipModel.startFloodingAnimation(compartments, levels, 6000);
    }

    setView(mode) {
        const duration = 1000;
        const startPos = this.camera.position.clone();
        const startTarget = this.controls.target.clone();

        let endPos, endTarget;

        switch (mode) {
            case 'side':
                endPos = new THREE.Vector3(0, 10, 60);
                endTarget = new THREE.Vector3(0, 0, 0);
                break;
            case 'top':
                endPos = new THREE.Vector3(0, 80, 0.1);
                endTarget = new THREE.Vector3(0, 0, 0);
                break;
            case '3d':
                endPos = new THREE.Vector3(40, 25, 40);
                endTarget = new THREE.Vector3(0, 0, 0);
                break;
        }

        const startTime = Date.now();

        const animate = () => {
            const elapsed = Date.now() - startTime;
            const progress = Math.min(elapsed / duration, 1);
            const ease = 1 - Math.pow(1 - progress, 3);

            this.camera.position.lerpVectors(startPos, endPos, ease);
            this.controls.target.lerpVectors(startTarget, endTarget, ease);
            this.controls.update();

            if (progress < 1) {
                requestAnimationFrame(animate);
            }
        };

        animate();
    }

    updateConnectionStatus(connected) {
        const el = document.getElementById('connection-status');
        if (connected) {
            el.textContent = '已连接';
            el.className = 'status-item connected';
        } else {
            el.textContent = '未连接';
            el.className = 'status-item warning';
        }
    }

    updateStatusDisplay(data) {
        const draftEl = document.getElementById('draft-value');
        const heelEl = document.getElementById('heel-value');
        const trimEl = document.getElementById('trim-value');
        const gmEl = document.getElementById('gm-value');
        const gzEl = document.getElementById('gz-value');

        draftEl.textContent = `${data.draft?.toFixed(2) || '0.00'} m`;
        heelEl.textContent = `${data.heel_angle?.toFixed(2) || '0.00'}°`;
        trimEl.textContent = `${data.trim_angle?.toFixed(2) || '0.00'}°`;
        gmEl.textContent = `${data.metacentric_height?.toFixed(3) || '0.000'} m`;
        gzEl.textContent = `${data.righting_arm?.toFixed(3) || '0.000'} m`;

        gmEl.className = 'value ' + (data.metacentric_height > 0.3 ? 'success' : data.metacentric_height > 0.15 ? '' : 'danger');
        heelEl.className = 'value ' + (Math.abs(data.heel_angle) < 10 ? '' : 'danger');
        draftEl.className = 'value ' + (data.draft < 4.0 ? '' : 'danger');
    }

    initCompartmentList() {
        const container = document.getElementById('compartment-list');
        container.innerHTML = COMPARTMENT_NAMES.map((name, i) => `
            <div class="compartment-item" id="compartment-${i}">
                <span>${i + 1}. ${name}</span>
                <div class="water-bar">
                    <div class="water-fill" style="width: 0%"></div>
                </div>
                <span style="width: 50px; text-align: right;">0%</span>
            </div>
        `).join('');
    }

    updateCompartmentStates(data) {
        data.forEach(d => {
            const el = document.getElementById(`compartment-${d.compartment_id}`);
            if (!el) return;

            const percentage = Math.min((d.water_level / d.max_water_level) * 100, 100);
            const fillEl = el.querySelector('.water-fill');
            const pctEl = el.querySelector('span:last-child');

            if (fillEl) fillEl.style.width = `${percentage}%`;
            if (pctEl) pctEl.textContent = `${percentage.toFixed(0)}%`;

            if (d.is_flooded || percentage > 30) {
                el.classList.add('flooded');
            } else {
                el.classList.remove('flooded');
            }
        });
    }

    updateStabilityChart(curve) {
        if (!curve || curve.length === 0) return;

        const data = curve.map(p => p.righting_arm);
        this.stabilityChart.data.datasets[0].data = data;

        const maxGz = Math.max(...data);
        document.getElementById('gz-value').textContent = `${maxGz.toFixed(3)} m`;

        this.stabilityChart.update();
    }

    updateWaterChart(data) {
        const waterData = this.waterChart.data.datasets[0].data;
        const colors = this.waterChart.data.datasets[0].backgroundColor;

        data.forEach(d => {
            if (d.compartment_id < waterData.length) {
                waterData[d.compartment_id] = d.water_level;
                colors[d.compartment_id] = d.is_flooded || d.water_level > 1.0
                    ? 'rgba(255, 107, 107, 0.7)'
                    : 'rgba(79, 172, 254, 0.6)';
            }
        });

        this.waterChart.update();
    }

    updateDraftHistory(data) {
        const now = new Date().toLocaleTimeString();
        this.timeLabels.push(now);
        this.draftHistory.push(data.draft);

        if (this.timeLabels.length > 30) {
            this.timeLabels.shift();
            this.draftHistory.shift();
        }

        this.updateDraftChart(data);
    }

    updateDraftChart(data) {
        this.draftChart.data.labels = this.timeLabels;
        this.draftChart.data.datasets[0].data = this.draftHistory;

        if (data) {
            const heelData = this.draftChart.data.datasets[1].data;
            heelData.push(data.heel_angle);
            if (heelData.length > 30) heelData.shift();
        }

        this.draftChart.update();
    }

    onWindowResize() {
        const container = document.getElementById('canvas-container');
        this.camera.aspect = container.clientWidth / container.clientHeight;
        this.camera.updateProjectionMatrix();
        this.renderer.setSize(container.clientWidth, container.clientHeight);
    }

    animate() {
        requestAnimationFrame(() => this.animate());
        this.controls.update();

        const dt = this.clock ? this.clock.getDelta() : 0.016;

        if (this.shipModel) {
            if (this.shipModel.updateParticles) {
                this.shipModel.updateParticles(dt);
            }
            if (this.shipModel.waterPlane) {
                const time = Date.now() * 0.001;
                this.shipModel.waterPlane.position.y += Math.sin(time) * 0.002;
            }
        }

        this.renderer.render(this.scene, this.camera);
    }
}

document.addEventListener('DOMContentLoaded', () => {
    new ShipSimulationApp();
});
