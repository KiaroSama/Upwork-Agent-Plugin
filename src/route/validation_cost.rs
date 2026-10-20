//! Diagnostic cost of real route validation, with no production instrumentation.
use super::Route;
use crate::config::{self, Mode, Profile};
use cpu_time::ProcessTime;
use std::{fs, sync::atomic::Ordering, time::Instant};

struct Sample {
    wall: Instant,
    cpu: ProcessTime,
    reads: usize,
}
impl Sample {
    fn start(route: &Route) -> Self {
        Self {
            wall: Instant::now(),
            cpu: ProcessTime::try_now().expect("Process CPU clock must be available"),
            reads: route.snapshot_reads.load(Ordering::Relaxed),
        }
    }
    fn finish(
        self,
        route: &Route,
        profiles: usize,
        sample: usize,
        workload: &str,
    ) -> serde_json::Value {
        let cpu = self.cpu.try_elapsed().expect("Process CPU clock failed");
        serde_json::json!({
            "profiles": profiles, "sample": sample, "workload": workload,
            "elapsed_us": self.wall.elapsed().as_micros(), "cpu_us": cpu.as_micros(),
            "snapshot_reads": route.snapshot_reads.load(Ordering::Relaxed) - self.reads,
            "cpu_scope": "isolated test process including runtime; other nextest processes excluded",
            "read_scope": "snapshot read attempts, not kernel syscalls or physical disk reads",
            "workers": 1, "os": std::env::consts::OS, "arch": std::env::consts::ARCH,
            "available_parallelism": std::thread::available_parallelism().unwrap().get(),
            "cache_condition": "same-file repeated warm reads"
        })
    }
}

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
            let start = Sample::start(&route);
            for _ in 0..8 {
                route.check().unwrap();
            }
            let row = start.finish(&route, profiles, sample, "sequential-checks");
            assert_eq!(row["snapshot_reads"], 8);
            rows.push(row);
            let before = route.snapshot_reads.load(Ordering::Relaxed);
            route.guard(async {}).await.unwrap();
            assert_eq!(route.snapshot_reads.load(Ordering::Relaxed) - before, 3);
            let start = Sample::start(&route);
            // This models idle work across watcher ticks, not readiness waiting.
            tokio::time::timeout(
                std::time::Duration::from_secs(2),
                route.guard(async {
                    tokio::time::sleep(std::time::Duration::from_millis(225)).await;
                }),
            )
            .await
            .unwrap()
            .unwrap();
            let row = start.finish(&route, profiles, sample, "idle-guard-225ms");
            assert!(row["snapshot_reads"].as_u64().unwrap() >= 3);
            rows.push(row);
            let start = Sample::start(&route);
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
            let row = start.finish(&route, profiles, sample, "two-concurrent-guards");
            assert!(row["snapshot_reads"].as_u64().unwrap() >= 6);
            rows.push(row);
        }
        let route = Route::new(path.clone()).unwrap();
        config::update(&path, |value| {
            value.mode = Mode::Blocked;
            Ok(())
        })
        .unwrap();
        assert!(route.check().is_err());
        let reads = route.snapshot_reads.load(Ordering::Relaxed);
        config::update(&path, |value| {
            value.mode = Mode::Direct;
            Ok(())
        })
        .unwrap();
        assert!(route.clone().check().is_err(), "Revocation remains latched");
        assert_eq!(route.snapshot_reads.load(Ordering::Relaxed), reads);
    }
    fs::write(
        ".ci-work/route-validation-cost.json",
        serde_json::to_vec_pretty(&rows).unwrap(),
    )
    .unwrap();
}
