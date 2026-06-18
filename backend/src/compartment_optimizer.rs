use crate::clickhouse_client::ClickHouseClient;
use crate::flooding_simulator::ShipHydrostatics;
use crate::models::*;
use rand::seq::SliceRandom;
use rand::Rng;
use tokio::sync::{mpsc, oneshot};

const MIN_COMPARTMENT_LENGTH_RATIO: f64 = 0.05;
const MAX_SCENARIO_BUDGET: usize = 40;
const CONSTRAINT_PENALTY_WEIGHT: f64 = 10.0;

#[derive(Clone)]
struct Individual {
    bulkhead_positions: Vec<f64>,
    fitness: f64,
    survival_probability: f64,
}

pub struct GeneticOptimizer {
    base_config: ShipConfig,
    damage_params: DamageParams,
    population_size: usize,
    generations: usize,
    mutation_rate: f64,
    crossover_rate: f64,
}

impl GeneticOptimizer {
    pub fn new(config: ShipConfig, damage_params: DamageParams) -> Self {
        Self {
            base_config: config,
            damage_params,
            population_size: 100,
            generations: 200,
            mutation_rate: 0.15,
            crossover_rate: 0.8,
        }
    }

    pub fn with_params(
        config: ShipConfig,
        damage_params: DamageParams,
        population_size: usize,
        generations: usize,
        mutation_rate: f64,
        crossover_rate: f64,
    ) -> Self {
        Self {
            base_config: config,
            damage_params,
            population_size,
            generations,
            mutation_rate,
            crossover_rate,
        }
    }

    fn repair_positions(&self, positions: &mut Vec<f64>, n: usize) {
        positions.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        positions.dedup_by(|a, b| (*a - *b).abs() < 1e-6);

        let l = self.base_config.length_overall;
        let min_gap = l * MIN_COMPARTMENT_LENGTH_RATIO;

        while positions.len() < n {
            let mut rng = rand::thread_rng();
            let pos = rng.gen_range(0.1..0.9) * l;
            positions.push(pos);
        }
        positions.truncate(n);
        positions.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

        for i in 0..positions.len() {
            if i == 0 {
                positions[i] = positions[i].max(min_gap);
            } else {
                positions[i] = positions[i].max(positions[i - 1] + min_gap);
            }
        }
        for i in (0..positions.len()).rev() {
            if i == positions.len() - 1 {
                positions[i] = positions[i].min(l - min_gap);
            } else {
                positions[i] = positions[i].min(positions[i + 1] - min_gap);
            }
        }
    }

    fn constraint_violation(&self, positions: &[f64]) -> f64 {
        let l = self.base_config.length_overall;
        let min_gap = l * MIN_COMPARTMENT_LENGTH_RATIO;
        let mut violation = 0.0;

        for i in 0..positions.len() {
            if positions[i] < min_gap {
                violation += (min_gap - positions[i]).abs();
            }
            if positions[i] > l - min_gap {
                violation += (positions[i] - (l - min_gap)).abs();
            }
            if i > 0 {
                let gap = positions[i] - positions[i - 1];
                if gap < min_gap {
                    violation += (min_gap - gap).abs();
                }
            }
        }

        violation
    }

    fn generate_individual(&self, n: usize) -> Individual {
        let mut rng = rand::thread_rng();
        let l = self.base_config.length_overall;
        let min_gap = l * MIN_COMPARTMENT_LENGTH_RATIO;

        let mut positions: Vec<f64> = Vec::with_capacity(n);
        for _ in 0..n {
            positions.push(rng.gen_range(min_gap..(l - min_gap)));
        }

        self.repair_positions(&mut positions, n);

        Individual {
            bulkhead_positions: positions,
            fitness: 0.0,
            survival_probability: 0.0,
        }
    }

    fn evaluate_fitness(&self, individual: &mut Individual) {
        let config = self.build_config_from_positions(&individual.bulkhead_positions);
        let hydrostatics = ShipHydrostatics::new(config.clone(), self.damage_params.clone());

        let scenarios = self.generate_test_scenarios(&config);
        let mut total_survival = 0.0;
        let survived = scenarios
            .iter()
            .filter(|scenario| {
                let result = hydrostatics.simulate_damage(scenario);
                result.is_safe
            })
            .count();

        if !scenarios.is_empty() {
            total_survival = survived as f64 / scenarios.len() as f64;
        }

        let avg_length = config.length_overall / config.compartment_count as f64;
        let uniformity_bonus = 1.0 / (1.0 + avg_length * 0.01);

        let violation = self.constraint_violation(&individual.bulkhead_positions);
        let penalty = violation * CONSTRAINT_PENALTY_WEIGHT;

        individual.survival_probability = total_survival;
        individual.fitness = total_survival * 0.7 + uniformity_bonus * 0.3 - penalty;
    }

    fn build_config_from_positions(&self, positions: &[f64]) -> ShipConfig {
        let mut config = self.base_config.clone();
        let n = positions.len() + 1;

        config.compartment_count = n as u8;

        let mut bounds: Vec<f64> = vec![0.0];
        bounds.extend_from_slice(positions);
        bounds.push(self.base_config.length_overall);

        let mut lengths = Vec::with_capacity(n);
        let mut volumes = Vec::with_capacity(n);

        let beam = self.base_config.beam;
        let depth = self.base_config.depth;
        let permeability = self.damage_params.permeability;

        for i in 0..n {
            let length = (bounds[i + 1] - bounds[i]).max(1.0);
            let volume = length * beam * depth * permeability;
            lengths.push(length);
            volumes.push(volume);
        }

        config.compartment_lengths = lengths;
        config.compartment_volumes = volumes;
        config.watertight_bulkheads = positions.to_vec();

        config
    }

    fn generate_test_scenarios(&self, config: &ShipConfig) -> Vec<FloodingScenario> {
        let mut scenarios = Vec::new();
        let n = config.compartment_count as usize;

        for i in 0..n {
            scenarios.push(FloodingScenario {
                ship_id: config.ship_id.clone(),
                flooded_compartments: vec![i as u8],
                damage_severity: 0.5,
            });
        }

        for i in 0..n.saturating_sub(1) {
            scenarios.push(FloodingScenario {
                ship_id: config.ship_id.clone(),
                flooded_compartments: vec![i as u8, (i + 1) as u8],
                damage_severity: 0.5,
            });
        }

        if scenarios.len() > MAX_SCENARIO_BUDGET {
            scenarios.shuffle(&mut rand::thread_rng());
            scenarios.truncate(MAX_SCENARIO_BUDGET);
        }

        scenarios
    }

    fn crossover(&self, parent1: &Individual, parent2: &Individual) -> Individual {
        let mut rng = rand::thread_rng();
        let n = parent1.bulkhead_positions.len();

        if rng.gen_bool(self.crossover_rate) {
            let crossover_point = rng.gen_range(1..n);
            let mut child_positions = parent1.bulkhead_positions[..crossover_point].to_vec();
            child_positions.extend_from_slice(&parent2.bulkhead_positions[crossover_point..]);
            self.repair_positions(&mut child_positions, n);

            Individual {
                bulkhead_positions: child_positions,
                fitness: 0.0,
                survival_probability: 0.0,
            }
        } else {
            parent1.clone()
        }
    }

    fn mutate(&self, individual: &mut Individual) {
        let mut rng = rand::thread_rng();
        let l = self.base_config.length_overall;
        let min_gap = l * MIN_COMPARTMENT_LENGTH_RATIO;

        for pos in &mut individual.bulkhead_positions {
            if rng.gen_bool(self.mutation_rate) {
                *pos += rng.gen_range(-l * 0.05..l * 0.05);
                *pos = pos.clamp(min_gap, l - min_gap);
            }
        }

        self.repair_positions(&mut individual.bulkhead_positions, individual.bulkhead_positions.len());
    }

    fn tournament_select(&self, population: &[Individual]) -> &Individual {
        let mut rng = rand::thread_rng();
        let tournament_size = 5.min(population.len());

        let indices: Vec<usize> = (0..population.len()).collect();
        let best_idx = indices
            .choose_multiple(&mut rng, tournament_size)
            .cloned()
            .max_by(|a, b| {
                population[a]
                    .fitness
                    .partial_cmp(&population[b].fitness)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .unwrap_or(0);

        &population[best_idx]
    }

    pub fn optimize(&self, compartment_count: usize) -> OptimizationResult {
        let n = compartment_count.max(2);
        let mut population: Vec<Individual> = (0..self.population_size)
            .map(|_| self.generate_individual(n))
            .collect();

        for individual in &mut population {
            self.evaluate_fitness(individual);
        }

        let mut best = population
            .iter()
            .max_by(|a, b| a.fitness.partial_cmp(&b.fitness).unwrap_or(std::cmp::Ordering::Equal))
            .cloned()
            .unwrap();

        for _generation in 0..self.generations {
            let mut new_population = Vec::with_capacity(self.population_size);

            new_population.push(best.clone());

            while new_population.len() < self.population_size {
                let parent1 = self.tournament_select(&population);
                let parent2 = self.tournament_select(&population);
                let mut child = self.crossover(parent1, parent2);
                self.mutate(&mut child);
                self.evaluate_fitness(&mut child);
                new_population.push(child);
            }

            population = new_population;

            let current_best = population
                .iter()
                .max_by(|a, b| a.fitness.partial_cmp(&b.fitness).unwrap_or(std::cmp::Ordering::Equal))
                .cloned()
                .unwrap();

            if current_best.fitness > best.fitness {
                best = current_best;
            }
        }

        let optimized_config = self.build_config_from_positions(&best.bulkhead_positions);
        let hydrostatics = ShipHydrostatics::new(optimized_config.clone(), self.damage_params.clone());
        let max_flooded = hydrostatics.calculate_max_floodable_compartments();

        OptimizationResult {
            optimization_id: uuid::Uuid::new_v4(),
            ship_id: optimized_config.ship_id.clone(),
            timestamp: chrono::Utc::now(),
            compartment_count: optimized_config.compartment_count,
            fitness_score: best.fitness,
            max_flooded_compartments: max_flooded,
            survival_probability: best.survival_probability,
            configuration: best.bulkhead_positions.clone(),
            best_configurations: vec![OptimizedConfig {
                compartment_count: optimized_config.compartment_count,
                bulkhead_positions: best.bulkhead_positions.clone(),
                fitness: best.fitness,
                survival_probability: best.survival_probability,
            }],
        }
    }

    pub fn optimize_compartment_count(
        &self,
        min_compartments: usize,
        max_compartments: usize,
    ) -> OptimizationResult {
        let mut best_result: Option<OptimizationResult> = None;
        let mut best_fitness = f64::NEG_INFINITY;

        for n in min_compartments..=max_compartments {
            let result = self.optimize(n);
            if result.fitness_score > best_fitness {
                best_fitness = result.fitness_score;
                best_result = Some(result);
            }
        }

        best_result.unwrap_or_else(|| self.optimize(min_compartments))
    }
}

pub enum OptimizeCommand {
    Optimize {
        request: OptimizationRequest,
        reply: oneshot::Sender<Result<OptimizationResult, String>>,
    },
}

pub struct CompartmentOptimizer {
    rx: mpsc::Receiver<OptimizeCommand>,
    clickhouse: ClickHouseClient,
    damage_params: DamageParams,
}

impl CompartmentOptimizer {
    pub fn new(
        rx: mpsc::Receiver<OptimizeCommand>,
        clickhouse: ClickHouseClient,
        damage_params: DamageParams,
    ) -> Self {
        Self {
            rx,
            clickhouse,
            damage_params,
        }
    }

    pub async fn run(mut self) {
        log::info!("CompartmentOptimizer task started");
        while let Some(cmd) = self.rx.recv().await {
            match cmd {
                OptimizeCommand::Optimize { request, reply } => {
                    let result = self.handle_optimize(request).await;
                    let _ = reply.send(result);
                }
            }
        }
        log::info!("CompartmentOptimizer task stopped");
    }

    async fn handle_optimize(
        &self,
        request: OptimizationRequest,
    ) -> Result<OptimizationResult, String> {
        let config = self
            .clickhouse
            .get_ship_config(&request.ship_id)
            .await
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("Ship config not found for id: {}", request.ship_id))?;

        let optimizer = GeneticOptimizer::with_params(
            config,
            self.damage_params.clone(),
            request.population_size.max(20),
            request.generations.max(50),
            0.15,
            0.8,
        );

        let result = optimizer.optimize_compartment_count(
            request.min_compartments.max(3) as usize,
            request.max_compartments.min(20) as usize,
        );

        if let Err(e) = self.clickhouse.insert_optimization_result(&result).await {
            log::error!("Failed to insert optimization result: {}", e);
        }

        Ok(result)
    }
}
