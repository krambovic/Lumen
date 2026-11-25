use std::{fs, path::Path, time::Duration};

use anyhow::Result;
use lumen_core::{
    constants::set_base_dir,
    geodata_resources::{GEODATA_METADATA_NAME, china_singbox_rule_sets},
    models::RoutingSettings,
    routing_presets::{ROUTING_PRESET_BLOCKED_CN, build_routing_preset},
    routing_runtime::apply_singbox_gui_routing,
};
use lumen_engines::core_resource_updater::{
    ResourceUpdateEvent, ResourceUpdateOptions, ResourceUpdateStatus, ResourceUpdateWorker,
    UpdateCancellation, regional_geodata_installed, sha256_file,
};
use serde_json::json;

#[tokio::test]
async fn china_preset_uses_packaged_geodata_without_downloading() -> Result<()> {
    let root = tempfile::tempdir()?;
    let app = root.path().join("app");
    fs::create_dir(&app)?;
    fs::write(app.join("portable"), b"")?;
    let bundle = root.path().join("core/geodata/china");
    fs::create_dir_all(bundle.join("rule-sets"))?;
    let rules = china_singbox_rule_sets();
    let mut hashes = serde_json::Map::new();
    let mut rule_hashes = serde_json::Map::new();
    let cancellation = UpdateCancellation::default();
    let write = |path: &Path, bytes: &[u8]| -> Result<_> {
        fs::write(path, bytes)?;
        Ok(json!(sha256_file(path, &cancellation)?))
    };
    hashes.insert("geoip.dat".into(), write(&bundle.join("geoip.dat"), &[1; 2048])?);
    hashes.insert("geosite.dat".into(), write(&bundle.join("geosite.dat"), &[2; 2048])?);
    let mut rule = [0; 80];
    rule[..3].copy_from_slice(b"SRS");
    for key in rules.keys() {
        let name = format!("{}.srs", key.replace(':', "-"));
        rule_hashes.insert(name.clone(), write(&bundle.join("rule-sets").join(name), &rule)?);
    }
    fs::write(
        bundle.join(GEODATA_METADATA_NAME),
        json!({
            "schema": 3, "region": "china", "version": "test",
            "rule_sets": rules, "sha256": hashes, "rule_set_sha256": rule_hashes,
        })
        .to_string(),
    )?;
    assert!(set_base_dir(app));
    let mut worker = ResourceUpdateWorker::start(ResourceUpdateOptions {
        kind: "geodata".into(),
        region: "china".into(),
        prefer_bundled_geodata: true,
        ..Default::default()
    });
    let result = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            match worker.events.recv().await {
                Some(ResourceUpdateEvent::RequestDisconnect(confirm)) => {
                    let _ = confirm.send(true);
                }
                Some(ResourceUpdateEvent::Done(result)) => return Ok(result),
                Some(ResourceUpdateEvent::Progress(_)) => {}
                None => anyhow::bail!("worker stopped"),
            }
        }
    })
    .await??;
    assert_eq!(result.status, ResourceUpdateStatus::Updated, "{}", result.message);
    assert!(regional_geodata_installed("china", None, None));
    let routing = build_routing_preset(&RoutingSettings::default(), ROUTING_PRESET_BLOCKED_CN);
    let mut config = json!({});
    apply_singbox_gui_routing(&mut config, &routing.to_dict())?;
    let rules = config["route"]["rule_set"].as_array().unwrap();
    let facebook = rules.iter().find(|rule| rule["tag"] == "geoip-facebook").unwrap();
    assert_eq!(facebook["format"], "binary");
    assert_eq!(
        Path::new(facebook["path"].as_str().unwrap()),
        root.path().join("app/data/geodata/rule-sets/geoip-facebook.srs")
    );
    assert!(rules.iter().all(|rule| Path::new(rule["path"].as_str().unwrap()).is_file()));
    assert!(!root.path().join("core/geosite.dat").exists());
    Ok(())
}
