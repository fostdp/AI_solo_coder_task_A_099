import * as THREE from 'three';

const COMPARTMENT_NAMES = [
    "艏尖舱", "前货舱1", "前货舱2", "中货舱1", "中货舱2",
    "中货舱3", "中货舱4", "后货舱1", "后货舱2", "机舱",
    "艉尖舱", "淡水舱1", "淡水舱2"
];

const COMPARTMENT_LENGTHS = [2.5, 2.8, 2.8, 3.0, 3.0, 3.0, 3.0, 2.8, 2.8, 4.0, 2.3, 1.5, 1.5];

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

            const waterGeometry = new THREE.BoxGeometry(length * 0.95, 0.01, width * 0.95);
            const waterMaterial = new THREE.MeshPhongMaterial({
                color: 0x006994,
                transparent: true,
                opacity: 0.7,
                side: THREE.DoubleSide
            });
            const waterMesh = new THREE.Mesh(waterGeometry, waterMaterial);
            waterMesh.position.x = compartment.position.x;
            waterMesh.position.y = 0.01;
            waterMesh.position.z = compartment.position.z;
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

        if (waterLevel > 0.01) {
            waterMesh.visible = true;
            const heightRatio = Math.min(waterLevel / maxLevel, 0.95);
            const height = heightRatio * (this.shipDepth * 0.85);

            const length = COMPARTMENT_LENGTHS[compartmentIndex];
            const width = this.shipBeam * 0.85;

            waterMesh.geometry.dispose();
            waterMesh.geometry = new THREE.BoxGeometry(length * 0.95, height, width * 0.95);
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
        } else {
            waterMesh.visible = false;
            compartment.material.color.setHex(0x4facfe);
            compartment.material.opacity = 0.35;
        }

        this.currentWaterLevels[compartmentIndex] = waterLevel;
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
            }

            const totalFloodedVolume = this.currentWaterLevels.reduce((sum, level, i) => {
                return sum + level * COMPARTMENT_LENGTHS[i] * this.shipBeam * 0.85;
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
        this.currentWaterLevels = new Array(COMPARTMENT_LENGTHS.length).fill(0);
        this.targetWaterLevels = new Array(COMPARTMENT_LENGTHS.length).fill(0);

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

            const waterGeometry = new THREE.BoxGeometry(length * 0.95, 0.01, width * 0.95);
            const waterMaterial = new THREE.MeshPhongMaterial({
                color: 0x006994,
                transparent: true,
                opacity: 0.7,
                side: THREE.DoubleSide
            });
            const waterMesh = new THREE.Mesh(waterGeometry, waterMaterial);
            waterMesh.position.x = compartment.position.x;
            waterMesh.position.y = 0.01;
            waterMesh.visible = false;

            this.shipGroup.add(waterMesh);
            this.waterMeshes.push(waterMesh);

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
    }
}
