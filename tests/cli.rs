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
    let rendered =
        fs::read_to_string(output.join("tests__fixtures__pool.d/zz-fpm-lens.conf")).unwrap();
    assert!(rendered.contains("pm.max_children = 13"));
    assert!(rendered.contains("pm.max_requests = 500"));
    assert!(!rendered.contains("[blog]"));
}
