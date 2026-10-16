//! Scheduled independent-tool preparation retains a shared memory lease around its child process.
use super::*;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Plan {
    script: PathBuf,
    script_sha256: String,
    node: PathBuf,
    node_sha256: String,
    receipt: PathBuf,
}

#[test]
#[ignore = "explicit serial fixture preparation with a source/tool-pinned private plan"]
fn scheduled_x2d_references_hold_shared_admission() {
    let plan: Plan = serde_json::from_slice(
        &fs::read(std::env::var_os("OMA_RAW_REFERENCE_PLAN").expect("OMA_RAW_REFERENCE_PLAN"))
            .unwrap(),
    )
    .unwrap();
    for path in [&plan.script, &plan.node, &plan.receipt] {
        assert!(path.is_absolute() && path.starts_with("/home"));
    }
    fn digest(path: &Path) -> String {
        let mut file = File::open(path).unwrap();
        let mut hash = Sha256::new();
        let mut bytes = [0u8; 65536];
        loop {
            let count = file.read(&mut bytes).unwrap();
            if count == 0 {
                break;
            }
            hash.update(&bytes[..count]);
        }
        format!("{:x}", hash.finalize())
    }
    assert_eq!(digest(&plan.script), plan.script_sha256);
    assert_eq!(digest(&plan.node), plan.node_sha256);
    let cancel = super::super::Cancellation::default();
    let before = crate::resources::global().usage().0;
    let child_address_limit = 5 * 1024 * 1024 * 1024u64 / 2;
    let reserved = child_address_limit + 128 * 1024 * 1024;
    let owner = crate::resources::global()
        .reserve(reserved, &cancel)
        .unwrap();
    let status = std::process::Command::new(&plan.node)
        .arg(&plan.script)
        .env(
            "OMA_REFERENCE_CHILD_LIMIT_KIB",
            (child_address_limit / 1024).to_string(),
        )
        .env(
            "OMA_REFERENCE_PARENT_ADMITTED_BYTES",
            owner.bytes().to_string(),
        )
        .status()
        .unwrap();
    assert_eq!(digest(&plan.script), plan.script_sha256);
    assert_eq!(digest(&plan.node), plan.node_sha256);
    assert!(
        status.success(),
        "reference preparation failed; retain its exact failure receipt"
    );
    let receipt: Value = serde_json::from_slice(&fs::read(&plan.receipt).unwrap()).unwrap();
    assert_eq!(receipt["status"], "references-pinned");
    assert_eq!(receipt["case"], "coverage-6565");
    assert_eq!(
        receipt["source_before_sha256"],
        receipt["source_after_sha256"]
    );
    assert_eq!(receipt["shared_admitted_bytes"], reserved);
    assert_eq!(receipt["references"].as_array().unwrap().len(), 2);
    assert!(receipt["application_qualified"].as_bool() == Some(false));
    drop(owner);
    assert_eq!(crate::resources::global().usage().0, before);
}
