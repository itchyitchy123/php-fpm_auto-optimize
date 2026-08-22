use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand};
use fpm_lens::{Evidence, PolicyFile, build_plan, discover_pool_dirs, load_inventory};
use std::{
    collections::BTreeMap,
    fs,
    io::{self, IsTerminal},
    path::PathBuf,
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
    },
}

fn main() {
    if let Err(error) = run() {
        eprintln!("error: {error:#}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let cli = Cli::parse();
    let dirs = if cli.pool_dir.is_empty() {
        discover_pool_dirs()
    } else {
        cli.pool_dir.clone()
    };
    if dirs.is_empty() {
        bail!("no PHP-FPM pool directories found; pass --pool-dir");
    }
    let pools = load_inventory(&dirs)?;
    if pools.is_empty() {
        bail!("no pools with pm.max_children found")
    }
    let policy = if cli.policy.exists() {
        PolicyFile::load(&cli.policy)?
    } else {
        PolicyFile::default()
    };
    let memory = cli
        .memory_mb
        .map_or_else(fpm_lens::system::detect_memory_mb, Ok)?;
    let evidence: BTreeMap<String, Evidence> = match cli.evidence {
        Some(path) => serde_json::from_slice(
            &fs::read(&path).with_context(|| format!("could not read {}", path.display()))?,
        )?,
        None => BTreeMap::new(),
    };
    match cli.command.unwrap_or(Command::Plan {
        json: false,
        output: None,
    }) {
        Command::Inventory => println!("{}", serde_json::to_string_pretty(&pools)?),
        Command::Observe {
            samples,
            interval_seconds,
            output,
        } => {
            let observations =
                fpm_lens::observe(&pools, samples, Duration::from_secs(interval_seconds));
            write_json(&output, &observations)?;
            println!("Saved {} sample(s) to {}", samples.max(1), output.display());
        }
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
                fs::write(&save_policy, toml::to_string_pretty(&edited)?)?;
                let plan = build_plan(&pools, &evidence, &edited, memory)?;
                write_json(&save_plan, &plan)?;
                println!(
                    "Saved {} and {}",
                    save_policy.display(),
                    save_plan.display()
                );
            }
        }
        Command::Render { plan, output_dir } => {
            let plan: fpm_lens::Plan = serde_json::from_slice(&fs::read(&plan)?)?;
            let files = fpm_lens::render::render_overrides(&plan, &output_dir)?;
            println!("Plan SHA-256: {}", fpm_lens::render::plan_digest(&plan)?);
            for f in files {
                println!("wrote {}", f.display());
            }
        }
    }
    Ok(())
}
fn write_json(path: &PathBuf, value: &impl serde::Serialize) -> Result<()> {
    let mut bytes = serde_json::to_vec_pretty(value)?;
    bytes.push(b'\n');
    fs::write(path, bytes).with_context(|| format!("could not write {}", path.display()))
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
