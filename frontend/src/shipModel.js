import * as THREE from 'three';

const COMPARTMENT_NAMES = [
    "艏尖舱", "前货舱1", "前货舱2", "中货舱1", "中货舱2",
    "中货舱3", "中货舱4", "后货舱1", "后货舱2", "机舱",
    "艉尖舱", "淡水舱1", "淡水舱2"
];

const COMPARTMENT_LENGTHS = [2.5, 2.8, 2.8, 3.0, 3.0, 3.0, 3.0, 2.8, 2.8, 4.0, 2.3, 1.5, 1.5];

const FLOOD_PARTICLE_VERTEX = `
attribute vec3 velocity;
attribute float birthTime;
attribute float maxLife;
attribute float seed;
attribute float size;

uniform float uTime;
uniform float uGravity;

varying float vAlpha;

void main() {
    float t = uTime - birthTime;
    float lifeRatio = clamp(t / max(maxLife, 0.0001), 0.0, 1.0);

    vec3 pos = position;
    if (t < 0.0 || t > maxLife) {
        vAlpha = 0.0;
    } else {
        pos += velocity * t;
        pos.y += 0.5 * uGravity * t * t;
        vAlpha = smoothstep(0.0, 0.1, lifeRatio) * (1.0 - smoothstep(0.5, 1.0, lifeRatio));
    }

    vec4 mvPosition = modelViewMatrix * vec4(pos, 1.0);
    gl_PointSize = size * (320.0 / max(1.0, -mvPosition.z)) * (0.4 + vAlpha);
    gl_Position = projectionMatrix * mvPosition;
}
`;

const FLOOD_PARTICLE_FRAGMENT = `
uniform vec3 uColor;
varying float vAlpha;

void main() {
    vec2 uv = gl_PointCoord - 0.5;
    float d = length(uv);
    if (d > 0.5) discard;
    float a = (1.0 - smoothstep(0.2, 0.5, d)) * vAlpha;
    gl_FragColor = vec4(uColor, a * 0.85);
}
`;

class FloodParticleSystem {
    constructor(maxParticles = 8000) {
        this.maxParticles = maxParticles;
        this.elapsed = 0.0;
        this.cursor = 0;
        this.dirty = false;

        const positions = new Float32Array(maxParticles * 3);
        const velocities = new Float32Array(maxParticles * 3);
        const birthTimes = new Float32Array(maxParticles);
        const maxLives = new Float32Array(maxParticles);
        const sizes = new Float32Array(maxParticles);
        const seeds = new Float32Array(maxParticles);

        for (let i = 0; i < maxParticles; i++) {
            birthTimes[i] = -1000.0;
            maxLives[i] = 0.0001;
            sizes[i] = 0.25 + Math.random() * 0.4;
            seeds[i] = Math.random();
        }

        this.geometry = new THREE.BufferGeometry();
        this.geometry.setAttribute('position', new THREE.BufferAttribute(positions, 3));
        this.geometry.setAttribute('velocity', new THREE.BufferAttribute(velocities, 3));
        this.geometry.setAttribute('birthTime', new THREE.BufferAttribute(birthTimes, 1));
        this.geometry.setAttribute('maxLife', new THREE.BufferAttribute(maxLives, 1));
        this.geometry.setAttribute('size', new THREE.BufferAttribute(sizes, 1));
        this.geometry.setAttribute('seed', new THREE.BufferAttribute(seeds, 1));
        this.geometry.setDrawRange(0, maxParticles);

        this.material = new THREE.ShaderMaterial({
            uniforms: {
                uTime: { value: 0.0 },
                uGravity: { value: -9.8 },
                uColor: { value: new THREE.Color(0x6fc3ff) }
            },
            vertexShader: FLOOD_PARTICLE_VERTEX,
            fragmentShader: FLOOD_PARTICLE_FRAGMENT,
            transparent: true,
            depthWrite: false,
            blending: THREE.AdditiveBlending
        });

        this.points = new THREE.Points(this.geometry, this.material);
        this.points.frustumCulled = false;
    }

    emit(x, y, z) {
        const i = this.cursor;
        const pos = this.geometry.attributes.position.array;
        const vel = this.geometry.attributes.velocity.array;
        const birth = this.geometry.attributes.birthTime.array;
        const maxLife = this.geometry.attributes.maxLife.array;

        pos[i * 3] = x;
        pos[i * 3 + 1] = y;
        pos[i * 3 + 2] = z;
        vel[i * 3] = (Math.random() - 0.5) * 2.2;
        vel[i * 3 + 1] = 2.5 + Math.random() * 4.0;
        vel[i * 3 + 2] = (Math.random() - 0.5) * 2.2;
        birth[i] = this.elapsed;
        maxLife[i] = 1.2 + Math.random() * 1.8;

        this.cursor = (this.cursor + 1) % this.maxParticles;
        this.dirty = true;
    }

    update(dt) {
        this.elapsed += dt;
        this.material.uniforms.uTime.value = this.elapsed;
        if (this.dirty) {
            this.geometry.attributes.position.needsUpdate = true;
            this.geometry.attributes.velocity.needsUpdate = true;
            this.geometry.attributes.birthTime.needsUpdate = true;
            this.geometry.attributes.maxLife.needsUpdate = true;
            this.dirty = false;
        }
    }

    clear() {
        const birth = this.geometry.attributes.birthTime.array;
        const maxLife = this.geometry.attributes.maxLife.array;
        for (let i = 0; i < this.maxParticles; i++) {
            birth[i] = -1000.0;
            maxLife[i] = 0.0001;
        }
        this.geometry.attributes.birthTime.needsUpdate = true;
        this.geometry.attributes.maxLife.needsUpdate = true;
    }

    dispose() {
        this.geometry.dispose();
        this.material.dispose();
    }
}

export class ShipModel {
    constructor(scene) {
        this.scene = scene;
        this.shipGroup = new THREE.Group();
        this.compartments = [];
        this.waterMeshes = [];
        this.bulkheads = [];
        this.hullMesh = null;
        this.waterPlane = null;
        this.shipLength = 34;
        this.shipBeam = 11;
        this.shipDepth = 4.5;
        this.designDraft = 2.8;
        this.currentDraft = this.designDraft;
        this.currentHeel = 0;
        this.currentTrim = 0;
        this.animationProgress = 0;
        this.isAnimating = false;
        this.targetWaterLevels = new Array(COMPARTMENT_LENGTHS.length).fill(0);
        this.currentWaterLevels = new Array(COMPARTMENT_LENGTHS.length).fill(0);
        this.compartmentLengths = COMPARTMENT_LENGTHS.slice();
        this.floodedState = new Array(COMPARTMENT_LENGTHS.length).fill(false);
        this.particleSystem = new FloodParticleSystem(8000);
        this.shipGroup.add(this.particleSystem.points);

        this.createHull();
        this.createCompartments();
        this.createBulkheads();
        this.createWaterPlane();

        this.scene.add(this.shipGroup);
    }

    createHull() {
        const hullShape = new THREE.Shape();
        const l = this.shipLength / 2;
        const b = this.shipBeam / 2;
        const d = this.shipDepth;

        hullShape.moveTo(-l, 0);

        const bowHeight = d * 0.6;
        hullShape.quadraticCurveTo(-l * 1.05, bowHeight * 0.5, -l * 0.9, bowHeight);
        hullShape.lineTo(-l * 0.7, d * 0.9);

        for (let x = -l * 0.7; x <= l * 0.7; x += l * 0.1) {
            const y = d - 0.1 * Math.abs(Math.sin(x / l * Math.PI));
            hullShape.lineTo(x, y);
        }

        hullShape.lineTo(l * 0.9, d * 0.7);
        hullShape.quadraticCurveTo(l * 1.02, d * 0.3, l, 0);

        for (let x = l; x >= -l; x -= l * 0.1) {
            const y = -this.designDraft * 0.9 * (1 - Math.pow(x / l, 2) * 0.3);
            hullShape.lineTo(x, y);
        }

        const hullGeometry = new THREE.ExtrudeGeometry(hullShape, {
            depth: this.shipBeam,
            bevelEnabled: true,
            bevelThickness: 0.1,
            bevelSize: 0.1,
            bevelSegments: 2
        });

        hullGeometry.center();
        hullGeometry.rotateX(Math.PI / 2);
        hullGeometry.translate(0, 0, 0);

        const hullMaterial = new THREE.MeshPhongMaterial({
            color: 0x8B5A2B,
            shininess: 30,
            transparent: true,
            opacity: 0.85,
            side: THREE.DoubleSide
        });

        this.hullMesh = new THREE.Mesh(hullGeometry, hullMaterial);
        this.hullMesh.castShadow = true;
        this.hullMesh.receiveShadow = true;
        this.shipGroup.add(this.hullMesh);

        const deckGeometry = new THREE.BoxGeometry(this.shipLength * 0.95, 0.1, this.shipBeam * 0.9);
        const deckMaterial = new THREE.MeshPhongMaterial({
            color: 0x6B4423,
            shininess: 20
        });
        const deck = new THREE.Mesh(deckGeometry, deckMaterial);
        deck.position.y = this.shipDepth * 0.95;
        deck.castShadow = true;
        this.shipGroup.add(deck);

        const cabinGeometry = new THREE.BoxGeometry(8, 2, 5);
        const cabinMaterial = new THREE.MeshPhongMaterial({
            color: 0x8B4513,
            shininess: 20
        });
        const cabin = new THREE.Mesh(cabinGeometry, cabinMaterial);
        cabin.position.set(2, this.shipDepth + 1, 0);
        cabin.castShadow = true;
        this.shipGroup.add(cabin);

        const mastGeometry = new THREE.CylinderGeometry(0.15, 0.25, 15, 8);
        const mastMaterial = new THREE.MeshPhongMaterial({
            color: 0x4A2511,
            shininess: 10
        });
        const mast = new THREE.Mesh(mastGeometry, mastMaterial);
        mast.position.set(0, this.shipDepth + 7.5, 0);
        mast.castShadow = true;
        this.shipGroup.add(mast);

        const sailShape = new THREE.Shape();
        sailShape.moveTo(0, 0);
        sailShape.quadraticCurveTo(-3, 5, -1, 12);
        sailShape.lineTo(1, 12);
        sailShape.quadraticCurveTo(3, 5, 0, 0);

        const sailGeometry = new THREE.ShapeGeometry(sailShape);
        const sailMaterial = new THREE.MeshPhongMaterial({
            color: 0xF5DEB3,
            shininess: 5,
            side: THREE.DoubleSide,
            transparent: true,
            opacity: 0.9
        });
        const sail = new THREE.Mesh(sailGeometry, sailMaterial);
        sail.position.set(0.3, this.shipDepth + 1.5, 0);
        sail.rotation.y = Math.PI / 2;
        sail.castShadow = true;
        this.shipGroup.add(sail);
    }

    createCompartments() {
        const totalLength = COMPARTMENT_LENGTHS.reduce((a, b) => a + b, 0);
        let currentX = -totalLength / 2;

        for (let i = 0; i < COMPARTMENT_LENGTHS.length; i++) {
            const length = COMPARTMENT_LENGTHS[i];
            const width = this.shipBeam * 0.85;
            const height = this.shipDepth * 0.85;

            const compartmentGeometry = new THREE.BoxGeometry(length, height, width);
            const compartmentMaterial = new THREE.MeshPhongMaterial({
                color: 0x4facfe,
                transparent: true,
                opacity: 0.35,
                side: THREE.DoubleSide
            });

            const compartment = new THREE.Mesh(compartmentGeometry, compartmentMaterial);
            compartment.position.x = currentX + length / 2;
            compartment.position.y = height / 2 - 0.2;

            const edges = new THREE.EdgesGeometry(compartmentGeometry);
            const lineMaterial = new THREE.LineBasicMaterial({
                color: 0xffffff,
                transparent: true,
                opacity: 0.5
            });
            const wireframe = new THREE.LineSegments(edges, lineMaterial);
            compartment.add(wireframe);

            this.shipGroup.add(compartment);
            this.compartments.push(compartment);

            const waterGeometry = new THREE.BoxGeometry(length * 0.95, this.shipDepth * 0.85, width * 0.95);
            const waterMaterial = new THREE.MeshPhongMaterial({
                color: 0x006994,
                transparent: true,
                opacity: 0.7,
                side: THREE.DoubleSide
            });
            const waterMesh = new THREE.Mesh(waterGeometry, waterMaterial);
            waterMesh.position.x = compartment.position.x;
            waterMesh.position.y = -0.2;
            waterMesh.position.z = compartment.position.z;
            waterMesh.scale.y = 0.001;
            waterMesh.visible = false;

            this.shipGroup.add(waterMesh);
            this.waterMeshes.push(waterMesh);

            currentX += length;
        }
    }

    createBulkheads() {
        const totalLength = COMPARTMENT_LENGTHS.reduce((a, b) => a + b, 0);
        let currentX = -totalLength / 2;

        for (let i = 0; i <= COMPARTMENT_LENGTHS.length; i++) {
            const bulkheadGeometry = new THREE.BoxGeometry(0.08, this.shipDepth * 0.9, this.shipBeam * 0.88);
            const bulkheadMaterial = new THREE.MeshPhongMaterial({
                color: 0xffd700,
                transparent: true,
                opacity: 0.6,
                side: THREE.DoubleSide
            });

            const bulkhead = new THREE.Mesh(bulkheadGeometry, bulkheadMaterial);
            bulkhead.position.x = currentX;
            bulkhead.position.y = this.shipDepth * 0.45;

            this.shipGroup.add(bulkhead);
            this.bulkheads.push(bulkhead);

            if (i < COMPARTMENT_LENGTHS.length) {
                currentX += COMPARTMENT_LENGTHS[i];
            }
        }
    }

    createWaterPlane() {
        const waterGeometry = new THREE.PlaneGeometry(200, 200, 50, 50);
        const waterMaterial = new THREE.MeshPhongMaterial({
            color: 0x1e90ff,
            transparent: true,
            opacity: 0.6,
            side: THREE.DoubleSide
        });

        this.waterPlane = new THREE.Mesh(waterGeometry, waterMaterial);
        this.waterPlane.rotation.x = -Math.PI / 2;
        this.waterPlane.position.y = -this.designDraft;
        this.waterPlane.receiveShadow = true;

        this.scene.add(this.waterPlane);
    }

    updateWaterLevel(compartmentIndex, waterLevel, maxLevel) {
        if (compartmentIndex >= this.waterMeshes.length) return;

        const waterMesh = this.waterMeshes[compartmentIndex];
        const compartment = this.compartments[compartmentIndex];
        const maxHeight = this.shipDepth * 0.85;

        if (waterLevel > 0.01) {
            waterMesh.visible = true;
            const heightRatio = Math.min(waterLevel / maxLevel, 0.95);
            const height = heightRatio * maxHeight;

            waterMesh.scale.y = Math.max(heightRatio, 0.001);
            waterMesh.position.y = height / 2 - 0.2;

            if (heightRatio > 0.3) {
                waterMesh.material.color.setHex(0xff4757);
                waterMesh.material.opacity = 0.75;
                compartment.material.color.setHex(0xff6b6b);
                compartment.material.opacity = 0.45;
            } else {
                waterMesh.material.color.setHex(0x006994);
                waterMesh.material.opacity = 0.7;
                compartment.material.color.setHex(0x4facfe);
                compartment.material.opacity = 0.35;
            }

            const nowFlooded = heightRatio > 0.3;
            if (nowFlooded && !this.floodedState[compartmentIndex]) {
                this.emitFloodParticles(compartmentIndex, 24);
            }
            this.floodedState[compartmentIndex] = nowFlooded;
        } else {
            waterMesh.visible = false;
            compartment.material.color.setHex(0x4facfe);
            compartment.material.opacity = 0.35;
            this.floodedState[compartmentIndex] = false;
        }

        this.currentWaterLevels[compartmentIndex] = waterLevel;
    }

    updateParticles(dt) {
        if (this.particleSystem) {
            this.particleSystem.update(dt);
        }
    }

    emitFloodParticles(compartmentIndex, count) {
        if (!this.particleSystem || compartmentIndex >= this.compartments.length) return;
        const compartment = this.compartments[compartmentIndex];
        const length = this.compartmentLengths[compartmentIndex] || 3;
        const maxHeight = this.shipDepth * 0.85;
        const heightRatio = Math.min(
            (this.currentWaterLevels[compartmentIndex] || 0) / maxHeight,
            0.95
        );
        const surfaceY = heightRatio * maxHeight - 0.2;

        for (let i = 0; i < count; i++) {
            const x = compartment.position.x + (Math.random() - 0.5) * length * 0.8;
            const y = Math.max(surfaceY, 0.2) + Math.random() * 0.3;
            const z = (Math.random() - 0.5) * this.shipBeam * 0.7;
            this.particleSystem.emit(x, y, z);
        }
    }

    setFlooded(compartmentIndex, isFlooded) {
        if (compartmentIndex >= this.compartments.length) return;

        const compartment = this.compartments[compartmentIndex];
        if (isFlooded) {
            compartment.material.color.setHex(0xff6b6b);
            compartment.material.opacity = 0.5;
        } else {
            compartment.material.color.setHex(0x4facfe);
            compartment.material.opacity = 0.35;
        }
    }

    updateShipPose(draft, heelAngle, trimAngle) {
        this.currentDraft = draft;
        this.currentHeel = heelAngle;
        this.currentTrim = trimAngle;

        const targetY = -draft;
        this.shipGroup.position.y += (targetY - this.shipGroup.position.y) * 0.1;

        const targetHeel = THREE.MathUtils.degToRad(heelAngle);
        this.shipGroup.rotation.z += (targetHeel - this.shipGroup.rotation.z) * 0.1;

        const targetTrim = THREE.MathUtils.degToRad(trimAngle);
        this.shipGroup.rotation.x += (targetTrim - this.shipGroup.rotation.x) * 0.1;

        if (this.waterPlane) {
            this.waterPlane.position.y += (-draft - this.waterPlane.position.y) * 0.05;
        }
    }

    startFloodingAnimation(floodedCompartments, targetLevels, duration = 5000) {
        this.isAnimating = true;
        this.animationProgress = 0;
        this.targetWaterLevels = new Array(COMPARTMENT_LENGTHS.length).fill(0);

        floodedCompartments.forEach((idx, i) => {
            if (idx < this.targetWaterLevels.length) {
                this.targetWaterLevels[idx] = targetLevels[i] || 2.0;
            }
        });

        const startTime = Date.now();

        const animate = () => {
            if (!this.isAnimating) return;

            const elapsed = Date.now() - startTime;
            this.animationProgress = Math.min(elapsed / duration, 1);

            const easeProgress = 1 - Math.pow(1 - this.animationProgress, 3);

            for (let i = 0; i < this.targetWaterLevels.length; i++) {
                const currentLevel = this.targetWaterLevels[i] * easeProgress;
                const maxLevel = this.shipDepth * 0.85;
                this.updateWaterLevel(i, currentLevel, maxLevel);
                this.setFlooded(i, currentLevel > 0.1);

                if (currentLevel > 0.2 && this.compartmentLengths[i] > 0) {
                    const burst = Math.max(1, Math.round(3 * easeProgress));
                    this.emitFloodParticles(i, burst);
                }
            }

            const totalFloodedVolume = this.currentWaterLevels.reduce((sum, level, i) => {
                const len = this.compartmentLengths[i] || COMPARTMENT_LENGTHS[i] || 0;
                return sum + level * len * this.shipBeam * 0.85;
            }, 0);

            const additionalDraft = totalFloodedVolume / (this.shipLength * this.shipBeam * 0.75);
            const simulatedDraft = this.designDraft + additionalDraft;
            const simulatedHeel = floodedCompartments.length > 0 ?
                floodedCompartments.reduce((sum, idx) => sum + (idx % 2 === 0 ? 1 : -1), 0) * 2 * easeProgress : 0;

            this.updateShipPose(simulatedDraft, simulatedHeel, 0);

            if (this.animationProgress < 1) {
                requestAnimationFrame(animate);
            } else {
                this.isAnimating = false;
            }
        };

        animate();
    }

    reset() {
        this.isAnimating = false;
        this.currentWaterLevels = new Array(this.compartments.length).fill(0);
        this.targetWaterLevels = new Array(this.compartments.length).fill(0);
        this.floodedState = new Array(this.compartments.length).fill(false);

        if (this.particleSystem) {
            this.particleSystem.clear();
        }

        for (let i = 0; i < this.compartments.length; i++) {
            this.updateWaterLevel(i, 0, this.shipDepth * 0.85);
            this.setFlooded(i, false);
        }

        this.updateShipPose(this.designDraft, 0, 0);
    }

    getCompartmentName(index) {
        return COMPARTMENT_NAMES[index] || `舱室${index + 1}`;
    }

    getCompartmentCount() {
        return COMPARTMENT_LENGTHS.length;
    }

    setCompartmentConfiguration(bulkheadPositions) {
        this.bulkheads.forEach(b => this.shipGroup.remove(b));
        this.compartments.forEach(c => this.shipGroup.remove(c));
        this.waterMeshes.forEach(w => this.shipGroup.remove(w));

        this.bulkheads = [];
        this.compartments = [];
        this.waterMeshes = [];
        const newLengths = [];

        const totalLength = this.shipLength;
        let currentX = -totalLength / 2;

        const positions = [0, ...bulkheadPositions, totalLength];

        for (let i = 0; i < positions.length - 1; i++) {
            const length = positions[i + 1] - positions[i];
            const width = this.shipBeam * 0.85;
            const height = this.shipDepth * 0.85;

            const compartmentGeometry = new THREE.BoxGeometry(length, height, width);
            const compartmentMaterial = new THREE.MeshPhongMaterial({
                color: 0x4facfe,
                transparent: true,
                opacity: 0.35,
                side: THREE.DoubleSide
            });

            const compartment = new THREE.Mesh(compartmentGeometry, compartmentMaterial);
            compartment.position.x = currentX + length / 2;
            compartment.position.y = height / 2 - 0.2;

            const edges = new THREE.EdgesGeometry(compartmentGeometry);
            const lineMaterial = new THREE.LineBasicMaterial({
                color: 0xffffff,
                transparent: true,
                opacity: 0.5
            });
            const wireframe = new THREE.LineSegments(edges, lineMaterial);
            compartment.add(wireframe);

            this.shipGroup.add(compartment);
            this.compartments.push(compartment);

            const waterGeometry = new THREE.BoxGeometry(length * 0.95, this.shipDepth * 0.85, width * 0.95);
            const waterMaterial = new THREE.MeshPhongMaterial({
                color: 0x006994,
                transparent: true,
                opacity: 0.7,
                side: THREE.DoubleSide
            });
            const waterMesh = new THREE.Mesh(waterGeometry, waterMaterial);
            waterMesh.position.x = compartment.position.x;
            waterMesh.position.y = -0.2;
            waterMesh.position.z = compartment.position.z;
            waterMesh.scale.y = 0.001;
            waterMesh.visible = false;

            this.shipGroup.add(waterMesh);
            this.waterMeshes.push(waterMesh);
            newLengths.push(length);

            currentX += length;
        }

        currentX = -totalLength / 2;
        for (let i = 0; i <= positions.length - 1; i++) {
            const bulkheadGeometry = new THREE.BoxGeometry(0.08, this.shipDepth * 0.9, this.shipBeam * 0.88);
            const bulkheadMaterial = new THREE.MeshPhongMaterial({
                color: 0xffd700,
                transparent: true,
                opacity: 0.6,
                side: THREE.DoubleSide
            });

            const bulkhead = new THREE.Mesh(bulkheadGeometry, bulkheadMaterial);
            bulkhead.position.x = currentX;
            bulkhead.position.y = this.shipDepth * 0.45;

            this.shipGroup.add(bulkhead);
            this.bulkheads.push(bulkhead);

            if (i < positions.length - 1) {
                currentX = positions[i + 1] - totalLength / 2;
            }
        }

        this.currentWaterLevels = new Array(this.compartments.length).fill(0);
        this.targetWaterLevels = new Array(this.compartments.length).fill(0);
        this.compartmentLengths = newLengths;
        this.floodedState = new Array(this.compartments.length).fill(false);

        if (this.particleSystem) {
            this.particleSystem.clear();
        }
    }
}
