use apex_exec::hybrid::{FileDispositionKind, OrgInventory};
use std::path::Path;
use std::process::Command;

const REPRESENTATIVE_ROOT: &str = "benchmarks/milestone22/nebula-logger/nebula-logger/core";

#[test]
fn representative_project_has_complete_file_and_component_accounting() {
    let inventory = OrgInventory::capture(REPRESENTATIVE_ROOT, "local").unwrap();
    let accounting = &inventory.accounting;

    assert_eq!(accounting.package_files.total, 1_055);
    assert_eq!(accounting.package_files.percentage, 100.0);
    assert_eq!(accounting.components.supported, inventory.components.len());
    assert_eq!(accounting.components.percentage, 100.0);
    assert_eq!(accounting.catalog_types.total, 548);
    assert_eq!(accounting.unclassified_files, 0);
    assert_eq!(
        accounting.recognized_files
            + accounting.intentional_non_metadata_files
            + accounting.unsupported_metadata_files,
        1_055
    );
    assert!(accounting.dispositions.iter().all(|disposition| {
        matches!(
            disposition.kind,
            FileDispositionKind::RecognizedMetadata
                | FileDispositionKind::IntentionalNonMetadata
                | FileDispositionKind::UnsupportedMetadata
        )
    }));
}

#[test]
fn representative_multipart_custom_metadata_names_are_lossless() {
    let inventory = OrgInventory::capture(REPRESENTATIVE_ROOT, "local").unwrap();
    let record = inventory
        .components
        .iter()
        .find(|component| {
            component.metadata_type == "CustomMetadata"
                && component.full_name == "LoggerParameter.CallStatusApi"
        })
        .expect("multipart Custom Metadata record must be inventoried");
    assert!(record.files.iter().any(|path| {
        path.ends_with(Path::new(
            "customMetadata/LoggerParameter.CallStatusApi.md-meta.xml",
        ))
    }));
}

#[test]
fn unsupported_package_files_receive_an_explicit_reason() {
    let fixture =
        std::env::temp_dir().join(format!("apex-exec-m26-unsupported-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&fixture);
    let path = fixture.join("apexexecunknowns/Feature.apexexecunknown-meta.xml");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, "<Future/>").unwrap();

    let inventory = OrgInventory::capture(&fixture, "local").unwrap();
    assert_eq!(inventory.accounting.package_files.total, 1);
    assert_eq!(
        inventory.accounting.unsupported_metadata_files, 1,
        "{:?}",
        inventory.accounting.dispositions
    );
    assert_eq!(inventory.accounting.unclassified_files, 0);
    assert!(
        inventory.accounting.dispositions[0]
            .reason
            .contains("no convention")
    );
    std::fs::remove_dir_all(fixture).unwrap();
}

#[test]
fn metadata_inventory_cli_writes_the_profiled_report() {
    let output = std::env::temp_dir().join(format!(
        "apex-exec-m26-cli-inventory-{}.json",
        std::process::id()
    ));
    let result = Command::new(env!("CARGO_BIN_EXE_apex-exec"))
        .args([
            "metadata",
            "inventory",
            REPRESENTATIVE_ROOT,
            "--api-version",
            "65.0",
            "--output",
            output.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(
        String::from_utf8_lossy(&result.stdout)
            .contains("1055/1055 files accounted, 876/876 components accounted")
    );
    let report: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&output).unwrap()).unwrap();
    assert_eq!(report["accounting"]["profile"], "salesforce-api-65.0");
    assert_eq!(report["accounting"]["unclassifiedFiles"], 0);
    std::fs::remove_file(output).unwrap();
}
