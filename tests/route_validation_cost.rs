//! Diagnostic measurements, never a timing ratio or a reason to weaken route checks.
use std::{fs, time::Instant};
use upwork_connector::{
    config::{self, Mode, Profile},
    route::Route,
};

#[tokio::test]
async fn measure_small_and_large_route_checks_with_revocation_controls() {
    fs::create_dir_all(".ci-work").unwrap();
    let root = tempfile::tempdir_in(".ci-work").unwrap();
    let mut rows = Vec::new();
    for profiles in [1, 8192] {
        let path = root.path().join(format!("{profiles}/connections.json"));
        config::update(&path, |value| {
            value.mode = Mode::Direct;
            value.profiles = (0..profiles)
                .map(|index| Profile {
                    name: format!("fixture-{index}"),
                    url: format!("http://fixture.invalid:{}", 1024 + index),
                    label: None,
                })
                .collect();
            Ok(())
        })
        .unwrap();
        for sample in 0..3 {
            let route = Route::new(path.clone()).unwrap();
            let start = Instant::now();
            for _ in 0..8 {
                route.check().unwrap();
            }
            rows.push(
                serde_json::json!({"profiles":profiles,"sample":sample,"checks":8,
                "workload":"sequential-checks","elapsed_us":start.elapsed().as_micros(),"workers":1,
                "os":std::env::consts::OS,"arch":std::env::consts::ARCH,
                "cache_condition":"same-file repeated warm reads","cpu_time":"not measured","snapshot_reads":"not instrumented"}),
            );
            route.guard(async {}).await.unwrap();
            let start = Instant::now();
            // Model a genuinely idle operation across watcher ticks, not service readiness.
            tokio::time::timeout(
                std::time::Duration::from_secs(2),
                route.guard(async {
                    tokio::time::sleep(std::time::Duration::from_millis(225)).await;
                }),
            )
            .await
            .unwrap()
            .unwrap();
            rows.push(serde_json::json!({"profiles":profiles,"sample":sample,
                "workload":"idle-guard-225ms","elapsed_us":start.elapsed().as_micros(),"workers":1,
                "os":std::env::consts::OS,"arch":std::env::consts::ARCH,
                "cache_condition":"same-file repeated warm reads","cpu_time":"not measured",
                "snapshot_reads":"not instrumented"}));
            let start = Instant::now();
            let gate = tokio::sync::Barrier::new(2);
            let one = route.guard(async {
                gate.wait().await;
            });
            let two = route.guard(async {
                gate.wait().await;
            });
            let (one, two) = tokio::time::timeout(std::time::Duration::from_secs(2), async {
                tokio::join!(one, two)
            })
            .await
            .unwrap();
            one.unwrap();
            two.unwrap();
            rows.push(serde_json::json!({"profiles":profiles,"sample":sample,
                "workload":"two-concurrent-guards","elapsed_us":start.elapsed().as_micros(),"workers":1,
                "guard_tasks":2,"cpu_time":"not measured","snapshot_reads":"not instrumented"}));
        }
        let route = Route::new(path.clone()).unwrap();
        config::update(&path, |value| {
            value.mode = Mode::Blocked;
            Ok(())
        })
        .unwrap();
        assert!(route.check().is_err());
        config::update(&path, |value| {
            value.mode = Mode::Direct;
            Ok(())
        })
        .unwrap();
        assert!(route.check().is_err(), "Revocation remains latched");
    }
    fs::write(
        ".ci-work/route-validation-cost.json",
        serde_json::to_vec_pretty(&rows).unwrap(),
    )
    .unwrap();
}
