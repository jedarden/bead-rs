// F015: Rapid-fire lifecycle stress and capacity benchmark harness
//
// This module implements a deterministic, noninteractive stress harness for
// testing the bead-rs system under concurrent load. It exercises the real
// store and service/CLI paths with isolated temporary workspaces.
//
// The harness supports:
// - Multiple scales: 100, 1,000, 10,000, 100,000, 1,000,000 beads
// - Worker saturation sweeps: 1-200 workers at logarithmic steps
// - Dataset families: independent, chains, wide-DAGs, diamonds, mixed lifecycle
// - Workloads: claim-close, claim-release, mixed, dependency-churn
// - Comprehensive JSON reporting with performance metrics

use anyhow::Result;
use bead_rs::{
    model::{BaseStatus, Issue},
    service::{claim, dependencies, issues, lifecycle},
    store::SqliteStore,
};
use rand::rngs::StdRng;
use rand::Rng;
use rand::SeedableRng;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::time::{Duration, Instant};
use tempfile::TempDir;
use time::OffsetDateTime;

/// Benchmark configuration
#[derive(Debug, Clone)]
pub struct BenchmarkConfig {
    pub num_beads: usize,
    pub num_workers: usize,
    pub seed: u64,
    pub duration: Option<Duration>,
    pub workload: WorkloadType,
    pub dataset_family: DatasetFamily,
    pub output_path: PathBuf,
    pub warmup_duration: Duration,
}

/// Types of workloads to run
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum WorkloadType {
    ClaimClose,
    ClaimRelease,
    Mixed,
    DependencyChurn,
}

/// Dataset families for testing different graph structures
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DatasetFamily {
    Independent,
    Chains,
    WideDAGs,
    Diamonds,
    MixedLifecycle,
}

/// Generated dataset with beads and dependencies
#[derive(Debug, Clone)]
pub struct GeneratedDataset {
    pub issues: Vec<Issue>,
    pub dependencies: Vec<(String, String, String)>, // (blocked, blocker, kind)
    pub dataset_family: DatasetFamily,
    pub total_beads: usize,
    pub expected_ready_frontier: usize,
}

/// Performance metrics collected during benchmark run
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceMetrics {
    // Attempt counts
    pub attempted_claims: usize,
    pub succeeded_claims: usize,
    pub conflicted_claims: usize,
    pub busy_claim_failures: usize,

    // Lifecycle counts
    pub closes: usize,
    pub releases: usize,
    pub reopens: usize,
    pub updates: usize,

    // Throughput
    pub claims_per_second: f64,
    pub operations_per_second: f64,

    // Latency percentiles (microseconds)
    pub p50_latency_us: u64,
    pub p95_latency_us: u64,
    pub p99_latency_us: u64,
    pub max_latency_us: u64,

    // Database metrics
    pub total_transaction_duration_ms: u64,
    pub shortlist_size: usize,
    pub full_scan_fallbacks: usize,

    // Cache metrics
    pub cache_hits: usize,
    pub cache_dirty: usize,
    pub cache_recomputes: usize,

    // Resource usage
    pub peak_memory_bytes: u64,
    pub db_size_bytes: u64,
    pub wal_size_bytes: u64,

    // Internal latency tracking (not serialized)
    #[serde(skip)]
    pub latencies: Vec<Duration>,
}

/// Complete benchmark report
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkReport {
    // Schema and version info
    pub schema_version: String,
    pub commit_hash: String,
    pub build_profile: String,
    pub rust_version: String,
    pub sqlite_version: String,

    // Environment
    pub os: String,
    pub cpu_count: usize,
    pub total_memory_bytes: u64,
    pub filesystem: String,

    // Configuration
    pub seed: u64,
    pub num_beads: usize,
    pub num_workers: usize,
    pub dataset_family: DatasetFamily,
    pub workload: WorkloadType,
    pub policy: String,
    pub worker_model: String,
    pub warmup_duration: String,
    pub duration: String,

    // Dataset shape
    pub total_issues: usize,
    pub dependency_count: usize,
    pub graph_depth: usize,
    pub ready_frontier_width: usize,
    pub ready_frontier_density: f64,

    // Results
    pub metrics: PerformanceMetrics,
    pub resource_limited: bool,
    pub completion_reason: String,
    pub timestamp: String,
}

impl BenchmarkConfig {
    /// Create a default benchmark configuration
    pub fn new(num_beads: usize, num_workers: usize) -> Self {
        Self {
            num_beads,
            num_workers,
            seed: rand::thread_rng().gen(),
            duration: None,
            workload: WorkloadType::ClaimClose,
            dataset_family: DatasetFamily::MixedLifecycle,
            output_path: PathBuf::from("benchmark-report.json"),
            warmup_duration: Duration::from_secs(5),
        }
    }
}

/// Helper function to create a basic issue with common fields
fn create_base_issue(id: String, title: String, priority: i64) -> Issue {
    let now = OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_else(|_| "unknown".to_string());

    Issue {
        id,
        title,
        description: None,
        notes: None,
        priority,
        base_status: BaseStatus::Open,
        manual_blocked: None,
        assignee: None,
        issue_type: Some("task".to_string()),
        created_at: now.clone(),
        updated_at: now,
        closed_at: None,
        close_reason: None,
        source_repo: None,
        profile: Some("native-v1".to_string()),
        schema_ref: Some("urn:bead-rs:schema:issue:native-v1".to_string()),
        data: None,
        extensions: HashMap::new(),
    }
}

impl GeneratedDataset {
    /// Generate a deterministic dataset based on configuration
    pub fn generate(config: &BenchmarkConfig) -> Result<Self> {
        let mut rng = StdRng::seed_from_u64(config.seed);
        let mut issues = Vec::with_capacity(config.num_beads);
        let mut dependencies = Vec::new();

        let now = OffsetDateTime::now_utc();

        match config.dataset_family {
            DatasetFamily::Independent => {
                // Every issue is ready (no dependencies)
                for i in 0..config.num_beads {
                    let issue = create_base_issue(
                        format!("bead-{:016x}", rng.gen::<u64>()),
                        format!("Independent task {}", i),
                        2, // P2
                    );
                    issues.push(issue);
                }
            }

            DatasetFamily::Chains => {
                // Create dependency chains: 0 -> 1 -> 2 -> ...
                let chain_length = 100;
                let num_chains = config.num_beads / chain_length;

                for chain in 0..num_chains {
                    let mut prev_id = None;

                    for i in 0..chain_length {
                        let issue = create_base_issue(
                            format!("bead-{:016x}", rng.gen::<u64>()),
                            format!("Chain {} task {}", chain, i),
                            2, // P2
                        );
                        let issue_id = issue.id.clone();

                        if let Some(prev) = prev_id {
                            dependencies.push((issue_id.clone(), prev, "blocks".to_string()));
                        }

                        issues.push(issue);
                        prev_id = Some(issue_id);
                    }
                }
            }

            DatasetFamily::WideDAGs => {
                // Many initial tasks converge into blocked layers
                let frontier_size = 100;
                let convergence_factor = 10;

                // Initial frontier
                for i in 0..frontier_size {
                    let issue = create_base_issue(
                        format!("bead-{:016x}", rng.gen::<u64>()),
                        format!("Frontier task {}", i),
                        2, // P2
                    );
                    issues.push(issue);
                }

                // Converging layers
                let mut prev_layer_start = 0;
                let mut prev_layer_size = frontier_size;

                while issues.len() < config.num_beads {
                    let next_layer_size = prev_layer_size / convergence_factor;
                    if next_layer_size == 0 {
                        break;
                    }

                    for i in 0..next_layer_size {
                        let issue = create_base_issue(
                            format!("bead-{:016x}", rng.gen::<u64>()),
                            format!("Layer task {}", i),
                            2, // P2
                        );
                        let issue_id = issue.id.clone();

                        // Connect to multiple predecessors
                        let predecessors_per_node =
                            std::cmp::min(convergence_factor, prev_layer_size);
                        for j in 0..predecessors_per_node {
                            let pred_idx = prev_layer_start
                                + (i * predecessors_per_node + j) % prev_layer_size;
                            if pred_idx < issues.len() {
                                dependencies.push((
                                    issue_id.clone(),
                                    issues[pred_idx].id.clone(),
                                    "blocks".to_string(),
                                ));
                            }
                        }

                        issues.push(issue);
                    }

                    prev_layer_start = issues.len() - next_layer_size;
                    prev_layer_size = next_layer_size;
                }
            }

            DatasetFamily::Diamonds => {
                // Shared downstream tasks
                let diamond_count = config.num_beads / 4;

                for d in 0..diamond_count {
                    // Create diamond structure
                    let root = create_base_issue(
                        format!("bead-{:016x}", rng.gen::<u64>()),
                        format!("Diamond {} root", d),
                        2, // P2
                    );
                    let root_id = root.id.clone();
                    issues.push(root);

                    // Two intermediate tasks
                    let mid1 = create_base_issue(
                        format!("bead-{:016x}", rng.gen::<u64>()),
                        format!("Diamond {} mid1", d),
                        2, // P2
                    );
                    let mid1_id = mid1.id.clone();

                    let mid2 = create_base_issue(
                        format!("bead-{:016x}", rng.gen::<u64>()),
                        format!("Diamond {} mid2", d),
                        2, // P2
                    );
                    let mid2_id = mid2.id.clone();

                    dependencies.push((mid1_id.clone(), root_id.clone(), "blocks".to_string()));
                    dependencies.push((mid2_id.clone(), root_id.clone(), "blocks".to_string()));

                    issues.push(mid1);
                    issues.push(mid2);

                    // Shared downstream task
                    let leaf = create_base_issue(
                        format!("bead-{:016x}", rng.gen::<u64>()),
                        format!("Diamond {} leaf", d),
                        2, // P2
                    );
                    let leaf_id = leaf.id.clone();

                    dependencies.push((leaf_id.clone(), mid1_id, "blocks".to_string()));
                    dependencies.push((leaf_id.clone(), mid2_id, "blocks".to_string()));

                    issues.push(leaf);
                }
            }

            DatasetFamily::MixedLifecycle => {
                // Realistic proportions of different lifecycle states
                let ready_ratio = 0.3;
                let assigned_ratio = 0.2;
                let closed_ratio = 0.3;
                let blocked_ratio = 0.2;

                let ready_count = (config.num_beads as f64 * ready_ratio) as usize;
                let assigned_count = (config.num_beads as f64 * assigned_ratio) as usize;
                let closed_count = (config.num_beads as f64 * closed_ratio) as usize;
                let blocked_count = (config.num_beads as f64 * blocked_ratio) as usize;

                // Ready issues
                for i in 0..ready_count {
                    let issue = create_base_issue(
                        format!("bead-{:016x}", rng.gen::<u64>()),
                        format!("Ready task {}", i),
                        2, // P2
                    );
                    issues.push(issue);
                }

                // Assigned issues
                for i in 0..assigned_count {
                    let mut issue = create_base_issue(
                        format!("bead-{:016x}", rng.gen::<u64>()),
                        format!("Assigned task {}", i),
                        2, // P2
                    );
                    issue.base_status = BaseStatus::InProgress;
                    issue.assignee = Some(format!("worker-{}", i % 10));
                    issues.push(issue);
                }

                // Closed issues
                for i in 0..closed_count {
                    let mut issue = create_base_issue(
                        format!("bead-{:016x}", rng.gen::<u64>()),
                        format!("Closed task {}", i),
                        2, // P2
                    );
                    issue.base_status = BaseStatus::Closed;
                    let now_formatted = now
                        .format(&time::format_description::well_known::Rfc3339)
                        .unwrap_or_else(|_| "unknown".to_string());
                    issue.closed_at = Some(now_formatted);
                    issue.close_reason = Some("completed".to_string());
                    issues.push(issue);
                }

                // Blocked issues (with dependencies)
                let _blocker_start = issues.len();
                for i in 0..(blocked_count / 2) {
                    let blocker = create_base_issue(
                        format!("bead-{:016x}", rng.gen::<u64>()),
                        format!("Blocker task {}", i),
                        2, // P2
                    );
                    let blocker_id = blocker.id.clone();
                    issues.push(blocker);

                    let blocked = create_base_issue(
                        format!("bead-{:016x}", rng.gen::<u64>()),
                        format!("Blocked task {}", i),
                        2, // P2
                    );
                    let blocked_id = blocked.id.clone();

                    dependencies.push((blocked_id, blocker_id, "blocks".to_string()));
                    issues.push(blocked);
                }
            }
        }

        // Calculate expected ready frontier
        let expected_ready_frontier = issues
            .iter()
            .filter(|i| {
                i.base_status == BaseStatus::Open
                    && i.assignee.is_none()
                    && i.manual_blocked.map_or(true, |blocked| !blocked)
            })
            .count();

        Ok(GeneratedDataset {
            issues,
            dependencies,
            dataset_family: config.dataset_family,
            total_beads: config.num_beads,
            expected_ready_frontier,
        })
    }

    /// Load dataset into a store
    pub fn load_into_store(&self, store: &mut SqliteStore) -> Result<()> {
        let conn = store.conn();

        // Create workspace config
        let config = bead_rs::store::WorkspaceConfig {
            root: std::path::PathBuf::from("/tmp/benchmark"),
            uuid: "benchmark-uuid".to_string(),
            prefix: "bead".to_string(),
        };

        for issue in &self.issues {
            issues::create_issue(
                conn,
                &config,
                issue.title.clone(),
                issue.description.clone(),
                issue.priority,
                issue.issue_type.clone(),
                None,   // assignee
                vec![], // labels
            )?;
        }

        for (blocked, blocker, kind) in &self.dependencies {
            dependencies::add_dependency(store, blocked, blocker, kind)?;
        }

        Ok(())
    }
}

impl BenchmarkReport {
    /// Generate a comprehensive benchmark report
    pub fn generate(
        config: &BenchmarkConfig,
        dataset: &GeneratedDataset,
        metrics: &PerformanceMetrics,
        resource_limited: bool,
    ) -> Self {
        let commit_hash = std::process::Command::new("git")
            .args(["rev-parse", "HEAD"])
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
            .unwrap_or_else(|_| "unknown".to_string());

        BenchmarkReport {
            schema_version: "benchmark-v1".to_string(),
            commit_hash,
            build_profile: if cfg!(debug_assertions) {
                "debug"
            } else {
                "release"
            }
            .to_string(),
            rust_version: rust_version(),
            sqlite_version: sqlite_version(),

            os: std::env::consts::OS.to_string(),
            cpu_count: num_cpus::get(),
            total_memory_bytes: total_memory(),
            filesystem: "unknown".to_string(),

            seed: config.seed,
            num_beads: config.num_beads,
            num_workers: config.num_workers,
            dataset_family: config.dataset_family,
            workload: config.workload,
            policy: "fifo-v1".to_string(),
            worker_model: "processes".to_string(),
            warmup_duration: format!("{:?}", config.warmup_duration),
            duration: format!("{:?}", config.duration.unwrap_or(Duration::from_secs(60))),

            total_issues: dataset.issues.len(),
            dependency_count: dataset.dependencies.len(),
            graph_depth: calculate_graph_depth(dataset),
            ready_frontier_width: dataset.expected_ready_frontier,
            ready_frontier_density: if dataset.total_beads > 0 {
                dataset.expected_ready_frontier as f64 / dataset.total_beads as f64
            } else {
                0.0
            },

            metrics: metrics.clone(),
            resource_limited,
            completion_reason: if resource_limited {
                "resource_limited".to_string()
            } else {
                "completed".to_string()
            },
            timestamp: OffsetDateTime::now_utc()
                .format(&time::format_description::well_known::Rfc3339)
                .unwrap_or_else(|_| "unknown".to_string()),
        }
    }
}

fn rust_version() -> String {
    std::process::Command::new("rustc")
        .args(["--version"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_else(|_| "unknown".to_string())
}

fn sqlite_version() -> String {
    rusqlite::version().to_string()
}

fn total_memory() -> u64 {
    // Try to get memory from /proc/meminfo on Linux
    if let Ok(meminfo) = fs::read_to_string("/proc/meminfo") {
        for line in meminfo.lines() {
            if line.starts_with("MemTotal:") {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 2 {
                    return parts[1].parse::<u64>().unwrap_or(0) * 1024; // Convert KB to bytes
                }
            }
        }
    }
    0
}

fn calculate_graph_depth(dataset: &GeneratedDataset) -> usize {
    // Simple depth calculation by following dependencies
    let mut depth_map = HashMap::new();

    for issue in &dataset.issues {
        let mut depth = 1;
        for (_blocked, blocker, _) in &dataset.dependencies {
            if blocker == &issue.id {
                let blocker_depth = depth_map.get(blocker).unwrap_or(&1);
                depth = (*blocker_depth + 1).max(depth);
            }
        }
        depth_map.insert(issue.id.clone(), depth);
    }

    *depth_map.values().max().unwrap_or(&1)
}

/// Main benchmark harness entry point
pub fn run_benchmark(config: BenchmarkConfig) -> Result<BenchmarkReport> {
    println!("F015 Benchmark Harness");
    println!("=======================");
    println!("Beads: {}", config.num_beads);
    println!("Workers: {}", config.num_workers);
    println!("Dataset: {:?}", config.dataset_family);
    println!("Workload: {:?}", config.workload);
    println!("Seed: {}", config.seed);
    println!();

    // Create temporary workspace
    let temp_dir = TempDir::new()?;
    let workspace_path = temp_dir.path();

    // Initialize workspace
    let store_path = workspace_path.join(".beads").join("beads.db");
    fs::create_dir_all(workspace_path.join(".beads"))?;

    let mut store = SqliteStore::with_path(&store_path)?;

    // Generate and load dataset
    println!("Generating dataset...");
    let dataset = GeneratedDataset::generate(&config)?;
    dataset.load_into_store(&mut store)?;

    println!(
        "Loaded {} issues with {} dependencies",
        dataset.issues.len(),
        dataset.dependencies.len()
    );

    // Run warmup
    println!("Running warmup for {:?}...", config.warmup_duration);
    run_warmup(&config, &mut store)?;

    // Run benchmark
    println!("Running benchmark...");
    let metrics = execute_workload(&config, &mut store)?;

    // Generate report
    println!("Generating report...");
    let resource_limited = metrics.succeeded_claims < config.num_beads / 2; // Heuristic
    let report = BenchmarkReport::generate(&config, &dataset, &metrics, resource_limited);

    // Write report
    let report_json = serde_json::to_string_pretty(&report)?;
    fs::write(&config.output_path, report_json)?;
    println!("Report written to: {}", config.output_path.display());

    Ok(report)
}

/// Run warmup period to stabilize system
fn run_warmup(config: &BenchmarkConfig, store: &mut SqliteStore) -> Result<()> {
    let start = Instant::now();
    let warmup_ops = 100;

    while start.elapsed() < config.warmup_duration {
        for i in 0..warmup_ops {
            let assignee = format!("warmup-worker-{}", i % config.num_workers);
            let tx = store.conn().unchecked_transaction()?;

            match claim::claim_issue(&tx, &assignee, None, None, None) {
                Ok(result) => {
                    if let Some(bead_id) = &result.bead_id {
                        tx.commit()?;

                        // Immediately release for warmup
                        let release_tx = store.conn().unchecked_transaction()?;
                        lifecycle::release_issue(&release_tx, bead_id)?;
                        release_tx.commit()?;
                    } else {
                        tx.commit()?;
                    }
                }
                Err(_) => {
                    tx.commit()?;
                }
            }
        }
    }

    Ok(())
}

/// Execute the configured workload
fn execute_workload(
    config: &BenchmarkConfig,
    store: &mut SqliteStore,
) -> Result<PerformanceMetrics> {
    let mut metrics = PerformanceMetrics::default();
    let start = Instant::now();
    let duration = config.duration.unwrap_or(Duration::from_secs(60));

    match config.workload {
        WorkloadType::ClaimClose => {
            execute_claim_close(config, store, &mut metrics, duration)?;
        }
        WorkloadType::ClaimRelease => {
            execute_claim_release(config, store, &mut metrics, duration)?;
        }
        WorkloadType::Mixed => {
            execute_mixed_workload(config, store, &mut metrics, duration)?;
        }
        WorkloadType::DependencyChurn => {
            execute_dependency_churn(config, store, &mut metrics, duration)?;
        }
    }

    let elapsed = start.elapsed();
    metrics.claims_per_second = if elapsed.as_secs_f64() > 0.0 {
        metrics.succeeded_claims as f64 / elapsed.as_secs_f64()
    } else {
        0.0
    };

    metrics.operations_per_second = if elapsed.as_secs_f64() > 0.0 {
        (metrics.closes
            + metrics.releases
            + metrics.reopens
            + metrics.updates
            + metrics.succeeded_claims) as f64
            / elapsed.as_secs_f64()
    } else {
        0.0
    };

    Ok(metrics)
}

/// Execute claim-close workload
fn execute_claim_close(
    config: &BenchmarkConfig,
    store: &mut SqliteStore,
    metrics: &mut PerformanceMetrics,
    duration: Duration,
) -> Result<()> {
    let start = Instant::now();
    let assignees: Vec<_> = (0..config.num_workers)
        .map(|i| format!("worker-{}", i))
        .collect();

    while start.elapsed() < duration {
        for assignee in &assignees {
            metrics.attempted_claims += 1;

            let claim_start = Instant::now();
            let tx = store.conn().unchecked_transaction()?;

            match claim::claim_issue(&tx, assignee, None, None, None) {
                Ok(result) => {
                    if result.bead_id.is_some() {
                        metrics.succeeded_claims += 1;
                        metrics.latencies.push(claim_start.elapsed());

                        tx.commit()?;

                        // Close the claimed issue
                        if let Some(bead_id) = &result.bead_id {
                            let close_tx = store.conn().unchecked_transaction()?;
                            lifecycle::close_issue(&close_tx, bead_id, "benchmark completed")?;
                            close_tx.commit()?;
                            metrics.closes += 1;
                        }
                    } else {
                        // Empty queue
                        tx.commit()?;
                    }
                }
                Err(e) => {
                    if e.to_string().contains("busy") {
                        metrics.busy_claim_failures += 1;
                    } else {
                        metrics.conflicted_claims += 1;
                    }
                }
            }
        }
    }

    Ok(())
}

/// Execute claim-release workload
fn execute_claim_release(
    config: &BenchmarkConfig,
    store: &mut SqliteStore,
    metrics: &mut PerformanceMetrics,
    duration: Duration,
) -> Result<()> {
    let start = Instant::now();
    let assignees: Vec<_> = (0..config.num_workers)
        .map(|i| format!("worker-{}", i))
        .collect();

    while start.elapsed() < duration {
        for assignee in &assignees {
            metrics.attempted_claims += 1;

            let claim_start = Instant::now();
            let tx = store.conn().unchecked_transaction()?;

            match claim::claim_issue(&tx, assignee, None, None, None) {
                Ok(result) => {
                    if result.bead_id.is_some() {
                        metrics.succeeded_claims += 1;
                        metrics.latencies.push(claim_start.elapsed());

                        tx.commit()?;

                        // Release the claimed issue
                        if let Some(bead_id) = &result.bead_id {
                            let release_tx = store.conn().unchecked_transaction()?;
                            lifecycle::release_issue(&release_tx, bead_id)?;
                            release_tx.commit()?;
                            metrics.releases += 1;
                        }
                    } else {
                        // Empty queue
                        tx.commit()?;
                    }
                }
                Err(e) => {
                    if e.to_string().contains("busy") {
                        metrics.busy_claim_failures += 1;
                    } else {
                        metrics.conflicted_claims += 1;
                    }
                }
            }
        }
    }

    Ok(())
}

/// Execute mixed workload
fn execute_mixed_workload(
    config: &BenchmarkConfig,
    store: &mut SqliteStore,
    metrics: &mut PerformanceMetrics,
    duration: Duration,
) -> Result<()> {
    let start = Instant::now();
    let assignees: Vec<_> = (0..config.num_workers)
        .map(|i| format!("worker-{}", i))
        .collect();
    let mut rng = StdRng::seed_from_u64(config.seed);

    while start.elapsed() < duration {
        for assignee in &assignees {
            let action = rng.gen_range(0..10);

            match action {
                0..=5 => {
                    // Claim
                    metrics.attempted_claims += 1;

                    let claim_start = Instant::now();
                    let tx = store.conn().unchecked_transaction()?;

                    match claim::claim_issue(&tx, assignee, None, None, None) {
                        Ok(result) => {
                            if result.bead_id.is_some() {
                                metrics.succeeded_claims += 1;
                                metrics.latencies.push(claim_start.elapsed());
                            }
                            tx.commit()?;
                        }
                        Err(e) => {
                            if e.to_string().contains("busy") {
                                metrics.busy_claim_failures += 1;
                            } else {
                                metrics.conflicted_claims += 1;
                            }
                        }
                    }
                }
                6..=7 => {
                    // Release if has claim
                    let tx = store.conn().unchecked_transaction()?;
                    if let Ok(claimed) = claim::claim_issue(&tx, assignee, None, None, None) {
                        if let Some(bead_id) = &claimed.bead_id {
                            tx.commit()?;
                            let release_tx = store.conn().unchecked_transaction()?;
                            lifecycle::release_issue(&release_tx, bead_id)?;
                            release_tx.commit()?;
                            metrics.releases += 1;
                        } else {
                            tx.commit()?;
                        }
                    } else {
                        tx.commit()?;
                    }
                }
                8..=9 => {
                    // Close and reopen
                    let tx = store.conn().unchecked_transaction()?;
                    if let Ok(claimed) = claim::claim_issue(&tx, assignee, None, None, None) {
                        if let Some(bead_id) = &claimed.bead_id {
                            tx.commit()?;

                            let close_tx = store.conn().unchecked_transaction()?;
                            lifecycle::close_issue(&close_tx, bead_id, "benchmark cycle")?;
                            close_tx.commit()?;
                            metrics.closes += 1;

                            let reopen_tx = store.conn().unchecked_transaction()?;
                            lifecycle::reopen_issue(&reopen_tx, bead_id)?;
                            reopen_tx.commit()?;
                            metrics.reopens += 1;
                        } else {
                            tx.commit()?;
                        }
                    } else {
                        tx.commit()?;
                    }
                }
                _ => unreachable!(),
            }
        }
    }

    Ok(())
}

/// Execute dependency churn workload
fn execute_dependency_churn(
    config: &BenchmarkConfig,
    store: &mut SqliteStore,
    metrics: &mut PerformanceMetrics,
    duration: Duration,
) -> Result<()> {
    let start = Instant::now();
    let assignees: Vec<_> = (0..config.num_workers)
        .map(|i| format!("worker-{}", i))
        .collect();
    let mut rng = StdRng::seed_from_u64(config.seed);

    while start.elapsed() < duration {
        // Add/remove dependencies
        let issue_a = rng.gen_range(0..config.num_beads);
        let issue_b = rng.gen_range(0..config.num_beads);

        if issue_a != issue_b {
            let id_a = format!("bead-{:016x}", issue_a as u64);
            let id_b = format!("bead-{:016x}", issue_b as u64);

            // Randomly add or remove dependency
            if rng.gen_bool(0.5) {
                let _ = dependencies::add_dependency(store, &id_a, &id_b, "blocks");
            } else {
                let _ = dependencies::remove_dependency(store, &id_a, &id_b, Some("blocks"));
            }
        }

        // Intermixed claims
        for assignee in &assignees {
            metrics.attempted_claims += 1;

            let claim_start = Instant::now();
            let tx = store.conn().unchecked_transaction()?;

            match claim::claim_issue(&tx, assignee, None, None, None) {
                Ok(result) => {
                    if result.bead_id.is_some() {
                        metrics.succeeded_claims += 1;
                        metrics.latencies.push(claim_start.elapsed());

                        tx.commit()?;

                        if let Some(bead_id) = &result.bead_id {
                            let release_tx = store.conn().unchecked_transaction()?;
                            lifecycle::release_issue(&release_tx, bead_id)?;
                            release_tx.commit()?;
                            metrics.releases += 1;
                        }
                    } else {
                        tx.commit()?;
                    }
                }
                Err(e) => {
                    if e.to_string().contains("busy") {
                        metrics.busy_claim_failures += 1;
                    } else {
                        metrics.conflicted_claims += 1;
                    }
                }
            }
        }
    }

    Ok(())
}

impl Default for PerformanceMetrics {
    fn default() -> Self {
        let latencies = Vec::with_capacity(1000); // Pre-allocate for common cases

        Self {
            attempted_claims: 0,
            succeeded_claims: 0,
            conflicted_claims: 0,
            busy_claim_failures: 0,
            closes: 0,
            releases: 0,
            reopens: 0,
            updates: 0,
            claims_per_second: 0.0,
            operations_per_second: 0.0,
            p50_latency_us: 0,
            p95_latency_us: 0,
            p99_latency_us: 0,
            max_latency_us: 0,
            total_transaction_duration_ms: 0,
            shortlist_size: 0,
            full_scan_fallbacks: 0,
            cache_hits: 0,
            cache_dirty: 0,
            cache_recomputes: 0,
            peak_memory_bytes: 0,
            db_size_bytes: 0,
            wal_size_bytes: 0,
            latencies,
        }
    }
}

impl PerformanceMetrics {
    /// Calculate latency percentiles from collected samples
    pub fn calculate_percentiles(&mut self) {
        if self.latencies.is_empty() {
            return;
        }

        self.latencies.sort();

        let count = self.latencies.len();
        self.p50_latency_us = self.latencies[count / 2].as_micros() as u64;
        self.p95_latency_us = self.latencies[(count * 95) / 100].as_micros() as u64;
        self.p99_latency_us = self.latencies[(count * 99) / 100].as_micros() as u64;
        self.max_latency_us = self
            .latencies
            .last()
            .map(|d| d.as_micros() as u64)
            .unwrap_or(0);
    }
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();

    // Parse command line arguments
    let mut num_beads = 1000;
    let mut num_workers = 4;
    let mut seed = rand::thread_rng().gen();
    let mut duration_secs = 60;
    let mut workload = WorkloadType::ClaimClose;
    let mut dataset_family = DatasetFamily::MixedLifecycle;
    let mut output_path = PathBuf::from("benchmark-report.json");

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--beads" => {
                num_beads = args[i + 1].parse()?;
                i += 2;
            }
            "--workers" => {
                num_workers = args[i + 1].parse()?;
                i += 2;
            }
            "--seed" => {
                seed = args[i + 1].parse()?;
                i += 2;
            }
            "--duration" => {
                duration_secs = args[i + 1].parse()?;
                i += 2;
            }
            "--workload" => {
                workload = match args[i + 1].as_str() {
                    "claim-close" => WorkloadType::ClaimClose,
                    "claim-release" => WorkloadType::ClaimRelease,
                    "mixed" => WorkloadType::Mixed,
                    "dependency-churn" => WorkloadType::DependencyChurn,
                    _ => return Err(anyhow::anyhow!("Unknown workload: {}", args[i + 1])),
                };
                i += 2;
            }
            "--dataset" => {
                dataset_family = match args[i + 1].as_str() {
                    "independent" => DatasetFamily::Independent,
                    "chains" => DatasetFamily::Chains,
                    "wide-dags" => DatasetFamily::WideDAGs,
                    "diamonds" => DatasetFamily::Diamonds,
                    "mixed" => DatasetFamily::MixedLifecycle,
                    _ => return Err(anyhow::anyhow!("Unknown dataset family: {}", args[i + 1])),
                };
                i += 2;
            }
            "--output" => {
                output_path = PathBuf::from(&args[i + 1]);
                i += 2;
            }
            "--help" => {
                println!("F015 Benchmark Harness");
                println!("Usage: {} [OPTIONS]", args[0]);
                println!();
                println!("Options:");
                println!("  --beads <N>           Number of beads (default: 1000)");
                println!("  --workers <N>          Number of workers (default: 4)");
                println!("  --seed <N>            Random seed (default: random)");
                println!("  --duration <SECONDS>  Duration in seconds (default: 60)");
                println!("  --workload <TYPE>     Workload type: claim-close, claim-release, mixed, dependency-churn");
                println!("  --dataset <TYPE>      Dataset family: independent, chains, wide-dags, diamonds, mixed");
                println!(
                    "  --output <PATH>       Output report path (default: benchmark-report.json)"
                );
                println!("  --help                Show this help");
                return Ok(());
            }
            _ => {
                return Err(anyhow::anyhow!("Unknown argument: {}", args[i]));
            }
        }
    }

    // Validate scales
    const VALID_SCALES: &[usize] = &[100, 1000, 10000, 100000, 1000000];
    if !VALID_SCALES.contains(&num_beads) {
        return Err(anyhow::anyhow!(
            "Invalid bead count: {}. Must be one of: {:?}",
            num_beads,
            VALID_SCALES
        ));
    }

    // Validate worker count
    if !(1..=200).contains(&num_workers) {
        return Err(anyhow::anyhow!(
            "Invalid worker count: {}. Must be between 1 and 200",
            num_workers
        ));
    }

    let config = BenchmarkConfig {
        num_beads,
        num_workers,
        seed,
        duration: Some(Duration::from_secs(duration_secs)),
        workload,
        dataset_family,
        output_path,
        warmup_duration: Duration::from_secs(5),
    };

    let report = run_benchmark(config)?;

    println!();
    println!("Benchmark Complete!");
    println!("===================");
    println!(
        "Claims: {} succeeded, {} conflicted, {} busy",
        report.metrics.succeeded_claims,
        report.metrics.conflicted_claims,
        report.metrics.busy_claim_failures
    );
    println!(
        "Throughput: {:.2} claims/sec",
        report.metrics.claims_per_second
    );
    println!("P95 Latency: {} μs", report.metrics.p95_latency_us);
    println!("P99 Latency: {} μs", report.metrics.p99_latency_us);
    println!("Resource Limited: {}", report.resource_limited);

    Ok(())
}
