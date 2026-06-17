use crate::models::*;
use crate::ship_statics::ShipHydrostatics;
use rand::Rng;
use rand_distr::{Normal, Distribution};

pub struct GeneticOptimizer {
    base_config: ShipConfig,
    population_size: usize,
    generations: usize,
    mutation_rate: f64,
    crossover_rate: f64,
}

#[derive(Clone, Debug)]
struct Individual {
    bulkhead_positions: Vec<f64>,
    fitness: f64,
    survival_probability: f64,
}

impl GeneticOptimizer {
    pub fn new(
        base_config: ShipConfig,
        population_size: usize,
        generations: usize,
    ) -> Self {
        Self {
            base_config,
            population_size,
            generations,
            mutation_rate: 0.15,
            crossover_rate: 0.8,
        }
    }

    fn generate_random_individual(&self, num_compartments: usize) -> Individual {
        let mut rng = rand::thread_rng();
        let mut positions = Vec::with_capacity(num_compartments);

        let segment_length = self.base_config.length_overall / num_compartments as f64;
        for i in 0..num_compartments {
            let base_pos = (i as f64 + 0.5) * segment_length;
            let variation = rng.gen_range(-segment_length * 0.2..segment_length * 0.2);
            positions.push((base_pos + variation).max(0.5).min(self.base_config.length_overall - 0.5));
        }

        positions.sort_by(|a, b| a.partial_cmp(b).unwrap());

        let mut individual = Individual {
            bulkhead_positions: positions,
            fitness: 0.0,
            survival_probability: 0.0,
        };

        self.evaluate_fitness(&mut individual);
        individual
    }

    fn create_config_from_positions(&self, positions: &[f64]) -> ShipConfig {
        let compartment_count = (positions.len() + 1) as u8;
        let mut compartment_lengths = Vec::with_capacity(compartment_count as usize);
        let mut compartment_volumes = Vec::with_capacity(compartment_count as usize);
        let mut compartment_names = Vec::with_capacity(compartment_count as usize);

        let mut prev_pos = 0.0;
        for (i, &pos) in positions.iter().enumerate() {
            let length = pos - prev_pos;
            compartment_lengths.push(length);
            let volume = length * self.base_config.beam * self.base_config.depth * 0.7;
            compartment_volumes.push(volume);
            compartment_names.push(format!("舱室{}", i + 1));
            prev_pos = pos;
        }

        let last_length = self.base_config.length_overall - prev_pos;
        compartment_lengths.push(last_length);
        let last_volume = last_length * self.base_config.beam * self.base_config.depth * 0.7;
        compartment_volumes.push(last_volume);
        compartment_names.push(format!("舱室{}", compartment_count));

        ShipConfig {
            ship_id: self.base_config.ship_id.clone(),
            ship_name: self.base_config.ship_name.clone(),
            length_overall: self.base_config.length_overall,
            beam: self.base_config.beam,
            depth: self.base_config.depth,
            design_draft: self.base_config.design_draft,
            displacement: self.base_config.displacement,
            compartment_count,
            compartment_names,
            compartment_lengths,
            compartment_volumes,
            watertight_bulkheads: positions.to_vec(),
        }
    }

    fn evaluate_fitness(&self, individual: &mut Individual) {
        let config = self.create_config_from_positions(&individual.bulkhead_positions);
        let hydrostatics = ShipHydrostatics::new(config.clone());

        let mut total_fitness = 0.0;
        let mut survival_count = 0;
        let total_scenarios = (1..=2).flat_map(|n| {
            (0..config.compartment_count as usize)
                .combinations(n)
                .collect::<Vec<_>>()
                .into_iter()
        }).count().min(50);

        let max_floodable = self.calculate_max_floodable_compartments(&config, &hydrostatics);

        let scenarios = self.generate_test_scenarios(config.compartment_count as usize);
        let tested_scenarios = scenarios.into_iter().take(total_scenarios);

        for scenario_compartments in tested_scenarios {
            let scenario = FloodingScenario {
                ship_id: config.ship_id.clone(),
                flooded_compartments: scenario_compartments.iter().map(|&x| x as u8).collect(),
                damage_severity: 0.8,
            };

            let result = hydrostatics.simulate_damage(&scenario);

            let scenario_score = if result.is_safe {
                survival_count += 1;
                let gm_score = (result.metacentric_height / 0.5).min(1.0);
                let reserve_score = (result.reserve_buoyancy / 30.0).min(1.0);
                let time_score = if result.sinking_time_seconds > 3600.0 {
                    1.0
                } else {
                    result.sinking_time_seconds / 3600.0
                };
                0.4 * gm_score + 0.3 * reserve_score + 0.3 * time_score
            } else {
                0.0
            };

            total_fitness += scenario_score;
        }

        let survival_probability = survival_count as f64 / total_scenarios.max(1) as f64;
        let floodable_bonus = (max_floodable as f64 / config.compartment_count as f64) * 0.5;
        let efficiency_bonus = 1.0 / (config.compartment_count as f64).sqrt() * 0.2;

        individual.fitness = (total_fitness / total_scenarios.max(1) as f64)
            + floodable_bonus
            + efficiency_bonus;
        individual.survival_probability = survival_probability;
    }

    fn generate_test_scenarios(&self, max_compartments: usize) -> Vec<Vec<usize>> {
        let mut scenarios = Vec::new();

        for i in 0..max_compartments {
            scenarios.push(vec![i]);
        }

        for i in 0..max_compartments {
            for j in (i + 1)..max_compartments {
                if j == i + 1 {
                    scenarios.push(vec![i, j]);
                }
            }
        }

        for i in 0..max_compartments {
            for j in (i + 2)..max_compartments {
                scenarios.push(vec![i, j]);
            }
        }

        scenarios
    }

    fn calculate_max_floodable_compartments(
        &self,
        config: &ShipConfig,
        hydrostatics: &ShipHydrostatics,
    ) -> u8 {
        for n in (1..=config.compartment_count).rev() {
            let compartments: Vec<u8> = (0..n).collect();
            let scenario = FloodingScenario {
                ship_id: config.ship_id.clone(),
                flooded_compartments: compartments,
                damage_severity: 0.5,
            };

            let result = hydrostatics.simulate_damage(&scenario);
            if result.is_safe {
                return n;
            }
        }
        0
    }

    fn tournament_selection(&self, population: &[Individual], tournament_size: usize) -> Individual {
        let mut rng = rand::thread_rng();
        let mut best = None;

        for _ in 0..tournament_size {
            let idx = rng.gen_range(0..population.len());
            let candidate = &population[idx];
            if best.is_none() || candidate.fitness > best.as_ref().unwrap().fitness {
                best = Some(candidate.clone());
            }
        }

        best.unwrap()
    }

    fn crossover(&self, parent1: &Individual, parent2: &Individual) -> Individual {
        let mut rng = rand::thread_rng();
        let len = parent1.bulkhead_positions.len();
        let crossover_point = rng.gen_range(1..len.saturating_sub(1).max(1));

        let mut child_positions = Vec::with_capacity(len);
        child_positions.extend_from_slice(&parent1.bulkhead_positions[..crossover_point]);
        child_positions.extend_from_slice(&parent2.bulkhead_positions[crossover_point..]);

        child_positions.sort_by(|a, b| a.partial_cmp(b).unwrap());

        let mut child = Individual {
            bulkhead_positions: child_positions,
            fitness: 0.0,
            survival_probability: 0.0,
        };

        self.evaluate_fitness(&mut child);
        child
    }

    fn mutate(&self, individual: &Individual) -> Individual {
        let mut rng = rand::thread_rng();
        let normal = Normal::new(0.0, self.base_config.length_overall * 0.05).unwrap();

        let mut positions = individual.bulkhead_positions.clone();

        for i in 0..positions.len() {
            if rng.gen_bool(self.mutation_rate) {
                let delta = normal.sample(&mut rng);
                positions[i] = (positions[i] + delta)
                    .max(0.5)
                    .min(self.base_config.length_overall - 0.5);
            }
        }

        positions.sort_by(|a, b| a.partial_cmp(b).unwrap());

        let mut mutated = Individual {
            bulkhead_positions: positions,
            fitness: 0.0,
            survival_probability: 0.0,
        };

        self.evaluate_fitness(&mut mutated);
        mutated
    }

    pub fn optimize(&self, num_compartments: usize) -> OptimizedConfig {
        let mut population: Vec<Individual> = (0..self.population_size)
            .map(|_| self.generate_random_individual(num_compartments - 1))
            .collect();

        for gen in 0..self.generations {
            let mut new_population = Vec::with_capacity(self.population_size);

            population.sort_by(|a, b| b.fitness.partial_cmp(&a.fitness).unwrap());

            let elite_size = (self.population_size as f64 * 0.1) as usize;
            new_population.extend(population.iter().take(elite_size).cloned());

            while new_population.len() < self.population_size {
                let parent1 = self.tournament_selection(&population, 5);
                let parent2 = self.tournament_selection(&population, 5);

                let child = if rand::thread_rng().gen_bool(self.crossover_rate) {
                    self.crossover(&parent1, &parent2)
                } else {
                    if rand::thread_rng().gen_bool(0.5) {
                        parent1
                    } else {
                        parent2
                    }
                };

                let child = self.mutate(&child);
                new_population.push(child);
            }

            population = new_population;

            if gen % 10 == 0 {
                if let Some(best) = population.first() {
                    log::debug!(
                        "Generation {}: Best fitness = {:.4}, Survival = {:.1}%",
                        gen,
                        best.fitness,
                        best.survival_probability * 100.0
                    );
                }
            }
        }

        population.sort_by(|a, b| b.fitness.partial_cmp(&a.fitness).unwrap());
        let best = population.first().unwrap();

        let max_floodable = {
            let config = self.create_config_from_positions(&best.bulkhead_positions);
            let hydrostatics = ShipHydrostatics::new(config);
            self.calculate_max_floodable_compartments(&self.base_config, &hydrostatics)
        };

        OptimizedConfig {
            compartment_count: num_compartments as u8,
            bulkhead_positions: best.bulkhead_positions.clone(),
            fitness: best.fitness,
            survival_probability: best.survival_probability,
        }
    }

    pub fn optimize_compartment_count(
        &self,
        min_compartments: u8,
        max_compartments: u8,
    ) -> OptimizationResult {
        let mut best_configs = Vec::new();

        for n in min_compartments..=max_compartments {
            log::info!("Optimizing for {} compartments...", n);
            let config = self.optimize(n as usize);
            best_configs.push(config);
        }

        best_configs.sort_by(|a, b| b.fitness.partial_cmp(&a.fitness).unwrap());

        let best = &best_configs[0];
        let max_floodable = {
            let config = self.create_config_from_positions(&best.bulkhead_positions);
            let hydrostatics = ShipHydrostatics::new(config);
            self.calculate_max_floodable_compartments(&self.base_config, &hydrostatics)
        };

        OptimizationResult {
            optimization_id: uuid::Uuid::new_v4(),
            ship_id: self.base_config.ship_id.clone(),
            timestamp: chrono::Utc::now(),
            compartment_count: best.compartment_count,
            fitness_score: best.fitness,
            max_flooded_compartments: max_floodable,
            survival_probability: best.survival_probability,
            configuration: best.bulkhead_positions.clone(),
            best_configurations: best_configs,
        }
    }
}

trait Combinations {
    fn combinations(self, n: usize) -> Vec<Vec<usize>>;
}

impl Combinations for std::ops::Range<usize> {
    fn combinations(self, n: usize) -> Vec<Vec<usize>> {
        let items: Vec<usize> = self.collect();
        let mut result = Vec::new();

        if n == 0 || n > items.len() {
            return result;
        }

        let mut indices: Vec<usize> = (0..n).collect();
        result.push(indices.iter().map(|&i| items[i]).collect());

        loop {
            let mut i = n as isize - 1;
            while i >= 0 && indices[i as usize] == items.len() - n + i as usize {
                i -= 1;
            }

            if i < 0 {
                break;
            }

            indices[i as usize] += 1;
            for j in (i as usize + 1)..n {
                indices[j] = indices[j - 1] + 1;
            }
            result.push(indices.iter().map(|&i| items[i]).collect());
        }

        result
    }
}
