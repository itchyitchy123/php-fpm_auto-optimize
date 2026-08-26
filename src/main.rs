use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand};
use fpm_lens::{Evidence, PolicyFile, build_plan, discover_pool_dirs, load_inventory};
use std::{
    collections::BTreeMap,
    fs,
    io::{self, IsTerminal, Read, Write},
    path::{Path, PathBuf},
    time::Duration,
};

#[derive(Parser)]
#[command(name="fpm-lens", version, about="Explainable, evidence-aware PHP-FPM capacity planner", long_about=None)]
struct Cli {
    #[arg(long, global = true, value_name = "DIR")]
    pool_dir: Vec<PathBuf>,
    #[arg(long, global = true, default_value = "fpm-lens.toml")]
    policy: PathBuf,
    #[arg(long, global = true)]
    memory_mb: Option<u64>,
    #[arg(long, global = true, value_name = "FILE")]
    evidence: Option<PathBuf>,
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Inventory pools and produce an explainable plan.
    Plan {
        #[arg(long)]
        json: bool,
        #[arg(long, value_name = "FILE")]
        output: Option<PathBuf>,
    },
    /// Interactively select pools and edit constraints/settings.
    Review {
        #[arg(long, default_value = "fpm-lens.reviewed.toml")]
        save_policy: PathBuf,
        #[arg(long, default_value = "fpm-lens.plan.json")]
        save_plan: PathBuf,
    },
    /// Render a reviewed plan into a staging directory. Never edits /etc.
    Render {
        plan: PathBuf,
        #[arg(long, default_value = "build/review")]
        output_dir: PathBuf,
    },
    /// Print discovered pool configuration as JSON.
    Inventory,
    /// Sample live PHP-FPM workers and write reusable evidence.
    Observe {
        #[arg(long, default_value_t = 12)]
        samples: u32,
        #[arg(long, default_value_t = 5)]
        interval_seconds: u64,
        #[arg(long, default_value = "fpm-lens.evidence.json")]
        output: PathBuf,
        /// Map a pool ID/name to its PHP-FPM JSON status URL: POOL=http://127.0.0.1/status?json
        #[arg(long, value_name = "POOL=URL")]
        status_url: Vec<String>,
    },
    /// Diagnose discovery, permissions, memory limits, and useful PHP-FPM binaries.
    Doktor,
    /// Validate a portable plan and optionally test the installed PHP-FPM configuration.
    Validate {
        plan: PathBuf,
        #[arg(long, value_name = "BINARY")]
        php_fpm: Option<PathBuf>,
    },
    /// Show the settings a plan would change without writing files.
    Diff { plan: PathBuf },
    /// Compare two evidence snapshots.
    Compare { older: PathBuf, newer: PathBuf },
    /// Summarize one or more evidence snapshots.
    Report { evidence: Vec<PathBuf> },
    /// Collect evidence and produce a plan in one guided, read-only run.
    Assess {
        #[arg(long, default_value_t = 12)]
        samples: u32,
        #[arg(long, default_value_t = 5)]
        interval_seconds: u64,
        #[arg(long, default_value = "fpm-lens.evidence.json")]
        save_evidence: PathBuf,
        #[arg(long, default_value = "fpm-lens.plan.json")]
        save_plan: PathBuf,
        #[arg(long, value_name = "POOL=URL")]
        status_url: Vec<String>,
    },
}

fn main() {
    if let Err(error) = run() {
        eprintln!("error: {error:#}");
        let code = error.downcast_ref::<InfeasiblePlan>().map_or(1, |_| 2);
        std::process::exit(code);
    }
}

#[derive(Debug)]
struct InfeasiblePlan;
impl std::fmt::Display for InfeasiblePlan {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("plan is infeasible; review warnings and policy")
    }
}
impl std::error::Error for InfeasiblePlan {}

fn run() -> Result<()> {
    let cli = Cli::parse();
    let command = cli.command.unwrap_or(Command::Plan {
        json: false,
        output: None,
    });
    match &command {
        Command::Render { plan, output_dir } => {
            let plan = fpm_lens::artifact::load_plan(plan)?;
            let files = fpm_lens::render::render_overrides(&plan, output_dir)?;
            println!("Plan SHA-256: {}", fpm_lens::render::plan_digest(&plan)?);
            for file in files {
                println!("wrote {}", file.display());
            }
            return Ok(());
        }
        Command::Validate { plan, php_fpm } => {
            let plan = fpm_lens::artifact::load_plan(plan)?;
            println!(
                "valid schema v{} plan: {} pool(s), {} MB / {} MB",
                plan.schema_version,
                plan.pools.len(),
                plan.allocated_memory_mb,
                plan.available_fpm_memory_mb
            );
            if let Some(binary) = php_fpm {
                let output = fpm_lens::artifact::verify_staged_plan(&plan, binary)?;
                println!("{} staged -tt validation succeeded", binary.display());
                if !output.trim().is_empty() {
                    println!("{output}");
                }
            }
            return Ok(());
        }
        Command::Diff { plan } => {
            print_diff(&fpm_lens::artifact::load_plan(plan)?);
            return Ok(());
        }
        Command::Compare { older, newer } => {
            compare_evidence(older, newer)?;
            return Ok(());
        }
        Command::Report { evidence } => {
            if evidence.is_empty() {
                bail!("report requires at least one evidence file");
            }
            report_evidence(evidence)?;
            return Ok(());
        }
        _ => {}
    }
    let dirs = if cli.pool_dir.is_empty() {
        discover_pool_dirs()
    } else {
        cli.pool_dir.clone()
    };
    if matches!(command, Command::Doktor) {
        let pools = if dirs.is_empty() {
            Vec::new()
        } else {
            load_inventory(&dirs)?
        };
        return doktor(&dirs, &pools, fpm_lens::system::detect_memory_mb());
    }
    if dirs.is_empty() {
        bail!("no PHP-FPM pool directories found; pass --pool-dir");
    }
    let pools = load_inventory(&dirs)?;
    if pools.is_empty() {
        bail!("no pools with pm.max_children found")
    }
    if matches!(command, Command::Inventory) {
        println!("{}", serde_json::to_string_pretty(&pools)?);
        return Ok(());
    }
    if let Command::Observe {
        samples,
        interval_seconds,
        output,
        status_url,
    } = &command
    {
        let urls = parse_status_urls(status_url, &pools)?;
        let observations = fpm_lens::observe_with_status(
            &pools,
            *samples,
            Duration::from_secs(*interval_seconds),
            &urls,
        );
        write_json(output, &observations)?;
        println!(
            "Saved {} sample(s) to {}",
            (*samples).max(1),
            output.display()
        );
        return Ok(());
    }
    let policy = if cli.policy.exists() {
        PolicyFile::load(&cli.policy)?
    } else {
        PolicyFile::default()
    };
    let memory = cli
        .memory_mb
        .map_or_else(fpm_lens::system::detect_memory_mb, Ok)?;
    let evidence: BTreeMap<String, Evidence> = match cli
        .evidence
        .filter(|_| matches!(command, Command::Plan { .. } | Command::Review { .. }))
    {
        Some(path) => read_evidence(&path)?,
        None => BTreeMap::new(),
    };
    match command {
        Command::Plan { json, output } => {
            let plan = build_plan(&pools, &evidence, &policy, memory)?;
            if let Some(path) = output {
                write_json(&path, &plan)?;
            }
            if json {
                println!("{}", serde_json::to_string_pretty(&plan)?);
            } else {
                print_plan(&plan);
            }
            if !plan.feasible {
                return Err(InfeasiblePlan.into());
            }
        }
        Command::Review {
            save_policy,
            save_plan,
        } => {
            if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
                bail!("review requires an interactive terminal")
            }
            let initial = build_plan(&pools, &evidence, &policy, memory)?;
            let mut edited = policy;
            if fpm_lens::tui::review(&initial, &mut edited)? {
                let plan = build_plan(&pools, &evidence, &edited, memory)?;
                write_review_artifacts(&save_policy, &edited, &save_plan, &plan)?;
                println!(
                    "Saved {} and {}",
                    save_policy.display(),
                    save_plan.display()
                );
            }
        }
        Command::Assess {
            samples,
            interval_seconds,
            save_evidence,
            save_plan,
            status_url,
        } => {
            let urls = parse_status_urls(&status_url, &pools)?;
            println!(
                "Observing {} pool(s) for approximately {} seconds…",
                pools.len(),
                u64::from(samples.saturating_sub(1)) * interval_seconds
            );
            let observations = fpm_lens::observe_with_status(
                &pools,
                samples,
                Duration::from_secs(interval_seconds),
                &urls,
            );
            write_json(&save_evidence, &observations)?;
            let plan = build_plan(&pools, &observations, &policy, memory)?;
            write_json(&save_plan, &plan)?;
            print_plan(&plan);
            println!(
                "Saved {} and {}",
                save_evidence.display(),
                save_plan.display()
            );
            if !plan.feasible {
                return Err(InfeasiblePlan.into());
            }
        }
        Command::Doktor
        | Command::Inventory
        | Command::Observe { .. }
        | Command::Render { .. }
        | Command::Validate { .. }
        | Command::Diff { .. }
        | Command::Compare { .. }
        | Command::Report { .. } => unreachable!(),
    }
    Ok(())
}

fn parse_status_urls(
    values: &[String],
    pools: &[fpm_lens::Pool],
) -> Result<BTreeMap<String, String>> {
    let mut result = BTreeMap::new();
    for value in values {
        let (name, url) = value
            .split_once('=')
            .context("--status-url must be POOL=http://URL")?;
        let matches: Vec<_> = pools
            .iter()
            .filter(|p| {
                p.id.name == name || format!("{}:{}", p.id.directory.display(), p.id.name) == name
            })
            .collect();
        if matches.len() != 1 {
            bail!(
                "status pool {name:?} matched {} pools; use directory:name when ambiguous",
                matches.len()
            );
        }
        let pool = matches[0];
        if result
            .insert(
                format!("{}:{}", pool.id.directory.display(), pool.id.name),
                url.into(),
            )
            .is_some()
        {
            bail!("duplicate --status-url for pool {name}");
        }
    }
    Ok(result)
}

fn doktor(dirs: &[PathBuf], pools: &[fpm_lens::Pool], memory: Result<u64>) -> Result<()> {
    println!("FPM Lens doktor");
    let (memory, memory_error) = match memory {
        Ok(value) => (value, None),
        Err(error) => (0, Some(format!("{error:#}"))),
    };
    println!(
        "[{}] detected memory envelope: {memory} MB",
        if memory > 0 { "ok" } else { "warn" }
    );
    if let Some(error) = memory_error {
        println!("      {error}");
    }
    println!(
        "[{}] readable pool directories: {}",
        if dirs.is_empty() { "warn" } else { "ok" },
        dirs.len()
    );
    println!(
        "[{}] pools with pm.max_children: {}",
        if pools.is_empty() { "warn" } else { "ok" },
        pools.len()
    );
    let proc_access = fs::read_dir("/proc").is_ok();
    println!(
        "[{}] procfs access for worker memory",
        if proc_access { "ok" } else { "warn" }
    );
    let binaries = ["php-fpm", "php-fpm8.4", "php-fpm8.3", "php-fpm8.2"];
    let found: Vec<_> = binaries
        .iter()
        .filter(|name| executable_on_path(name))
        .collect();
    println!(
        "[{}] PHP-FPM validation binaries: {}",
        if found.is_empty() { "warn" } else { "ok" },
        if found.is_empty() {
            "none found".into()
        } else {
            found.into_iter().copied().collect::<Vec<_>>().join(", ")
        }
    );
    println!(
        "[info] configure each pool's JSON status endpoint for active-demand and saturation evidence"
    );
    if memory == 0 || dirs.is_empty() || pools.is_empty() || !proc_access {
        bail!("one or more essential doktor checks failed");
    }
    Ok(())
}

fn executable_on_path(name: &str) -> bool {
    std::env::var_os("PATH").is_some_and(|paths| {
        std::env::split_paths(&paths).any(|dir| {
            let path = dir.join(name);
            path.is_file()
                && fs::metadata(path).is_ok_and(|metadata| {
                    std::os::unix::fs::PermissionsExt::mode(&metadata.permissions()) & 0o111 != 0
                })
        })
    })
}

fn read_evidence(path: &Path) -> Result<BTreeMap<String, Evidence>> {
    let evidence: BTreeMap<String, Evidence> =
        serde_json::from_slice(&read_file_limited(path, 16 * 1024 * 1024)?)
            .with_context(|| format!("invalid evidence {}", path.display()))?;
    validate_evidence(&evidence)?;
    Ok(evidence)
}

fn validate_evidence(evidence: &BTreeMap<String, Evidence>) -> Result<()> {
    fpm_lens::artifact::validate_evidence_map(evidence)
}

fn read_file_limited(path: &Path, limit: u64) -> Result<Vec<u8>> {
    let file =
        fs::File::open(path).with_context(|| format!("could not read {}", path.display()))?;
    if file.metadata()?.len() > limit {
        bail!("{} exceeds the {} byte limit", path.display(), limit);
    }
    let mut bytes = Vec::new();
    file.take(limit + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > limit {
        bail!("{} exceeds the {} byte limit", path.display(), limit);
    }
    Ok(bytes)
}

fn report_evidence(paths: &[PathBuf]) -> Result<()> {
    for path in paths {
        let evidence = read_evidence(path)?;
        println!("{}", path.display());
        for (pool, e) in evidence {
            println!(
                "  {pool}: active_peak={} queue_peak={} worker_p75={} MB saturation={} status_samples={}",
                show(e.peak_workers),
                show(e.listen_queue_peak),
                show(e.worker_memory_mb),
                e.saturation_events,
                e.status_samples
            );
        }
    }
    Ok(())
}

fn compare_evidence(older: &Path, newer: &Path) -> Result<()> {
    let old = read_evidence(older)?;
    let new = read_evidence(newer)?;
    println!("POOL  ACTIVE(old→new)  P75 MB(old→new)  QUEUE(old→new)  SATURATION(old→new)");
    for key in old
        .keys()
        .chain(new.keys())
        .collect::<std::collections::BTreeSet<_>>()
    {
        let a = old.get(key).cloned().unwrap_or_default();
        let b = new.get(key).cloned().unwrap_or_default();
        println!(
            "{key}  {}→{}  {}→{}  {}→{}  {}→{}",
            show(a.peak_workers),
            show(b.peak_workers),
            show(a.worker_memory_mb),
            show(b.worker_memory_mb),
            show(a.listen_queue_peak),
            show(b.listen_queue_peak),
            a.saturation_events,
            b.saturation_events
        );
    }
    Ok(())
}

fn show(value: Option<u32>) -> String {
    value.map_or_else(|| "?".into(), |v| v.to_string())
}

fn print_diff(plan: &fpm_lens::Plan) {
    for pool in plan
        .pools
        .iter()
        .filter(|p| p.selected && p.current != p.proposed)
    {
        println!("[{}]", pool.id.name);
        diff_field(
            "pm.max_children",
            pool.current.max_children,
            pool.proposed.max_children,
        );
        diff_field(
            "pm.max_requests",
            pool.current.max_requests,
            pool.proposed.max_requests,
        );
        diff_field(
            "pm.process_idle_timeout",
            pool.current.process_idle_timeout_seconds,
            pool.proposed.process_idle_timeout_seconds,
        );
        diff_field(
            "request_terminate_timeout",
            pool.current.request_terminate_timeout_seconds,
            pool.proposed.request_terminate_timeout_seconds,
        );
    }
}
fn diff_field(name: &str, old: Option<u32>, new: Option<u32>) {
    if old != new {
        println!("  {name}: {} -> {}", show(old), show(new));
    }
}
fn write_json(path: &Path, value: &impl serde::Serialize) -> Result<()> {
    let mut bytes = serde_json::to_vec_pretty(value)?;
    bytes.push(b'\n');
    atomic_write(path, &bytes)
}

fn atomic_write(path: &Path, bytes: &[u8]) -> Result<()> {
    let temp = stage_file(path, bytes)?;
    let result =
        fs::rename(&temp, path).with_context(|| format!("could not install {}", path.display()));
    if result.is_err() {
        let _ = fs::remove_file(temp);
    }
    result?;
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    fs::File::open(parent)?.sync_all()?;
    Ok(())
}

fn stage_file(path: &Path, bytes: &[u8]) -> Result<PathBuf> {
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        fs::create_dir_all(parent)
            .with_context(|| format!("could not create {}", parent.display()))?;
    }
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let name = path
        .file_name()
        .and_then(|v| v.to_str())
        .unwrap_or("artifact");
    let mut attempt = 0_u32;
    let (temp, mut file) = loop {
        let temp = parent.join(format!(".{name}.{}.{}.tmp", std::process::id(), attempt));
        match fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temp)
        {
            Ok(file) => break (temp, file),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                attempt = attempt.checked_add(1).context("too many staged files")?;
            }
            Err(error) => {
                return Err(error).with_context(|| format!("could not stage {}", path.display()));
            }
        }
    };
    if let Err(error) = file.write_all(bytes).and_then(|()| file.sync_all()) {
        let _ = fs::remove_file(&temp);
        return Err(error).with_context(|| format!("could not stage {}", path.display()));
    }
    Ok(temp)
}

fn write_review_artifacts(
    policy_path: &Path,
    policy: &PolicyFile,
    plan_path: &Path,
    plan: &fpm_lens::Plan,
) -> Result<()> {
    if policy_path == plan_path {
        bail!("policy and plan output paths must differ");
    }
    let policy_bytes = toml::to_string_pretty(policy)?.into_bytes();
    let mut plan_bytes = serde_json::to_vec_pretty(plan)?;
    plan_bytes.push(b'\n');
    let backup =
        |path: &Path| path.with_extension(format!("fpm-lens-backup-{}", std::process::id()));
    let policy_backup = backup(policy_path);
    let plan_backup = backup(plan_path);
    if policy_backup.exists() || plan_backup.exists() {
        bail!("review transaction backup already exists");
    }
    let policy_temp = stage_file(policy_path, &policy_bytes)?;
    let plan_temp = match stage_file(plan_path, &plan_bytes) {
        Ok(path) => path,
        Err(error) => {
            let _ = fs::remove_file(policy_temp);
            return Err(error);
        }
    };
    let had_policy = policy_path.exists();
    let had_plan = plan_path.exists();
    if had_policy {
        fs::rename(policy_path, &policy_backup)?;
    }
    if had_plan {
        if let Err(error) = fs::rename(plan_path, &plan_backup) {
            if had_policy {
                let _ = fs::rename(&policy_backup, policy_path);
            }
            return Err(error.into());
        }
    }
    let commit =
        fs::rename(&policy_temp, policy_path).and_then(|()| fs::rename(&plan_temp, plan_path));
    if let Err(error) = commit {
        let _ = fs::remove_file(&policy_temp);
        let _ = fs::remove_file(&plan_temp);
        let _ = fs::remove_file(policy_path);
        let _ = fs::remove_file(plan_path);
        if had_policy {
            let _ = fs::rename(&policy_backup, policy_path);
        }
        if had_plan {
            let _ = fs::rename(&plan_backup, plan_path);
        }
        return Err(error).context("could not commit reviewed artifacts");
    }
    if had_policy {
        let _ = fs::remove_file(policy_backup);
    }
    if had_plan {
        let _ = fs::remove_file(plan_backup);
    }
    for parent in [policy_path.parent(), plan_path.parent()]
        .into_iter()
        .flatten()
    {
        fs::File::open(if parent.as_os_str().is_empty() {
            Path::new(".")
        } else {
            parent
        })?
        .sync_all()?;
    }
    Ok(())
}
fn print_plan(p: &fpm_lens::Plan) {
    println!(
        "FPM Lens plan — {} MB allocated / {} MB budget",
        p.allocated_memory_mb, p.available_fpm_memory_mb
    );
    println!(
        "{:<18} {:>7} {:>7} {:>7} {:>7}  EVIDENCE",
        "POOL", "NOW", "PLAN", "MIN", "MAX"
    );
    for d in &p.pools {
        println!(
            "{:<18} {:>7} {:>7} {:>7} {:>7}  {:?}",
            d.id.name,
            d.current.max_children.unwrap_or(0),
            d.proposed.max_children.unwrap_or(0),
            d.minimum_children,
            d.maximum_children,
            d.confidence
        );
    }
    for w in &p.warnings {
        println!("warning: {w}");
    }
    if !p.feasible {
        println!("INFEASIBLE: adjust bounds or memory policy before rendering");
    }
}
