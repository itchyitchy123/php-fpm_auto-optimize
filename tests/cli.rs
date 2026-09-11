use std::{fs, process::Command};

#[test]
fn plan_and_render_workflow() {
    let binary = env!("CARGO_BIN_EXE_fpm-lens");
    let temp = tempfile::tempdir().unwrap();
    let plan = temp.path().join("plan.json");
    let status = Command::new(binary)
        .args([
            "--pool-dir",
            "tests/fixtures/pool.d",
            "--policy",
            "tests/fixtures/policy.toml",
            "--evidence",
            "tests/fixtures/evidence.json",
            "--memory-mb",
            "4096",
            "plan",
            "--output",
        ])
        .arg(&plan)
        .status()
        .unwrap();
    assert!(status.success());
    let value: serde_json::Value = serde_json::from_slice(&fs::read(&plan).unwrap()).unwrap();
    assert_eq!(value["schema_version"], 1);
    let checkout = value["pools"]
        .as_array()
        .unwrap()
        .iter()
        .find(|pool| pool["id"]["name"] == "checkout")
        .unwrap();
    assert_eq!(checkout["proposed"]["max_children"], 13);
    let output = temp.path().join("rendered");
    let status = Command::new(binary)
        .arg("render")
        .arg(&plan)
        .arg("--output-dir")
        .arg(&output)
        .status()
        .unwrap();
    assert!(status.success());
    let manifest: serde_json::Value =
        serde_json::from_slice(&fs::read(output.join("fpm-lens-render-manifest.json")).unwrap())
            .unwrap();
    let staged = manifest["files"][0]["staged"].as_str().unwrap().to_owned();
    let rendered = fs::read_to_string(output.join(&staged)).unwrap();
    assert!(rendered.contains("pm.max_children = 13"));
    assert!(rendered.contains("pm.max_requests = 500"));
    assert!(!rendered.contains("[blog]"));

    let mut escaped = value.clone();
    escaped["pools"][1]["id"]["directory"] = serde_json::Value::String("..".into());
    let escaped_plan = temp.path().join("escaped.json");
    fs::write(&escaped_plan, serde_json::to_vec(&escaped).unwrap()).unwrap();
    let escaped_output = temp.path().join("contained");
    let status = Command::new(binary)
        .arg("render")
        .arg(&escaped_plan)
        .arg("--output-dir")
        .arg(&escaped_output)
        .status()
        .unwrap();
    assert!(status.success());
    assert!(!temp.path().join("zz-fpm-lens.conf").exists());

    let mut empty = value;
    for pool in empty["pools"].as_array_mut().unwrap() {
        pool["selected"] = serde_json::Value::Bool(false);
        pool["proposed"] = pool["current"].clone();
    }
    empty["allocated_memory_mb"] = serde_json::Value::from(
        empty["pools"]
            .as_array()
            .unwrap()
            .iter()
            .map(|pool| {
                pool["proposed"]["max_children"].as_u64().unwrap()
                    * pool["worker_memory_mb"].as_u64().unwrap()
            })
            .sum::<u64>(),
    );
    let empty_plan = temp.path().join("empty.json");
    fs::write(&empty_plan, serde_json::to_vec(&empty).unwrap()).unwrap();
    let status = Command::new(binary)
        .arg("render")
        .arg(&empty_plan)
        .arg("--output-dir")
        .arg(&output)
        .status()
        .unwrap();
    assert!(status.success());
    assert!(
        !output.join(staged).exists(),
        "obsolete staged override was retained"
    );
}

#[test]
fn command_dependencies_and_exit_codes_are_stable() {
    let binary = env!("CARGO_BIN_EXE_fpm-lens");
    let temp = tempfile::tempdir().unwrap();
    let invalid_policy = temp.path().join("invalid.toml");
    fs::write(&invalid_policy, "this is not toml = [").unwrap();
    let inventory = Command::new(binary)
        .args(["--pool-dir", "tests/fixtures/pool.d", "--policy"])
        .arg(&invalid_policy)
        .arg("inventory")
        .status()
        .unwrap();
    assert!(inventory.success(), "inventory loaded an irrelevant policy");

    let infeasible = Command::new(binary)
        .args([
            "--pool-dir",
            "tests/fixtures/pool.d",
            "--policy",
            "tests/fixtures/policy.toml",
            "--memory-mb",
            "600",
            "plan",
        ])
        .status()
        .unwrap();
    assert_eq!(infeasible.code(), Some(2));
}

#[test]
fn observation_inputs_fail_before_collection() {
    let binary = env!("CARGO_BIN_EXE_fpm-lens");
    let temporary = tempfile::tempdir().unwrap();

    let zero_samples = Command::new(binary)
        .args([
            "--pool-dir",
            "tests/fixtures/pool.d",
            "observe",
            "--samples",
            "0",
        ])
        .output()
        .unwrap();
    assert_eq!(zero_samples.status.code(), Some(2));

    let evidence = temporary.path().join("evidence.json");
    let invalid_url = Command::new(binary)
        .args([
            "--pool-dir",
            "tests/fixtures/pool.d",
            "observe",
            "--samples",
            "1",
            "--status-url",
            "checkout=https://127.0.0.1/status",
            "--output",
        ])
        .arg(&evidence)
        .output()
        .unwrap();
    assert!(!invalid_url.status.success());
    assert!(!evidence.exists());

    let remote_url = Command::new(binary)
        .args([
            "--pool-dir",
            "tests/fixtures/pool.d",
            "observe",
            "--samples",
            "1",
            "--status-url",
            "checkout=http://192.0.2.1/status",
            "--output",
        ])
        .arg(&evidence)
        .output()
        .unwrap();
    assert!(!remote_url.status.success());
    assert!(!evidence.exists());
}
