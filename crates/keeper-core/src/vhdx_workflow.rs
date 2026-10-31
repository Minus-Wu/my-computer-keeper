use crate::vhdx::validate_compact_path;
#[cfg(windows)]
use crate::vhdx::{ensure_not_in_use, run_elevated_worker};
use crate::{KeeperError, KeeperResult};
use serde::{Deserialize, Serialize};
use std::sync::{Mutex, MutexGuard};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VhdxCompactState {
    Idle,
    Running,
    Succeeded,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StepState {
    Pending,
    Running,
    Succeeded,
    Skipped,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompactStep {
    pub id: String,
    pub state: StepState,
    pub message: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CompactEnvironment {
    pub docker_running: bool,
    pub wsl_distributions: Vec<String>,
    #[serde(skip)]
    docker_exe: Option<std::path::PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VhdxCompactStatus {
    pub state: VhdxCompactState,
    pub path: String,
    pub size_before_bytes: u64,
    pub size_after_bytes: Option<u64>,
    pub message: String,
    pub steps: Vec<CompactStep>,
    pub progress_percent: Option<u8>,
    pub compact_completed: bool,
    pub recovery_required: bool,
    pub environment: CompactEnvironment,
}

struct Job {
    status: VhdxCompactStatus,
    restore_wsl: Vec<String>,
    restore_docker: bool,
}

static JOB: Mutex<Option<Job>> = Mutex::new(None);

fn lock_job() -> MutexGuard<'static, Option<Job>> {
    JOB.lock().unwrap_or_else(|e| e.into_inner())
}

fn initial_status(path: &str, size: u64) -> VhdxCompactStatus {
    VhdxCompactStatus {
        state: VhdxCompactState::Running,
        path: path.to_string(),
        size_before_bytes: size,
        size_after_bytes: None,
        message: String::new(),
        steps: [
            "inspect",
            "stop_docker",
            "stop_wsl",
            "compact",
            "start_wsl",
            "start_docker",
        ]
        .into_iter()
        .map(|id| CompactStep {
            id: id.into(),
            state: StepState::Pending,
            message: String::new(),
        })
        .collect(),
        progress_percent: None,
        compact_completed: false,
        recovery_required: false,
        environment: CompactEnvironment::default(),
    }
}

pub fn compact_vhdx_status() -> VhdxCompactStatus {
    lock_job()
        .as_ref()
        .map(|j| j.status.clone())
        .unwrap_or_else(|| {
            let mut status = initial_status("", 0);
            status.state = VhdxCompactState::Idle;
            status
        })
}

pub fn inspect_compact_environment() -> KeeperResult<CompactEnvironment> {
    NativeEnvironment
        .inspect()
        .map_err(KeeperError::SystemError)
}

pub fn start_compact_vhdx(path: &str) -> KeeperResult<VhdxCompactStatus> {
    let size = validate_compact_path(path)?;
    let status = initial_status(path, size);
    {
        let mut job = lock_job();
        if job.as_ref().is_some_and(|j| {
            j.status.state == VhdxCompactState::Running || j.status.recovery_required
        }) {
            return Err(KeeperError::SystemError(
                "請先等待目前作業完成並復原環境，再開始新的壓縮。".into(),
            ));
        }
        *job = Some(Job {
            status: status.clone(),
            restore_wsl: Vec::new(),
            restore_docker: false,
        });
    }
    std::thread::spawn(|| {
        let mut job = Job {
            status: compact_vhdx_status(),
            restore_wsl: Vec::new(),
            restore_docker: false,
        };
        run_workflow(&mut job, &NativeEnvironment, publish);
        publish(&job);
    });
    Ok(status)
}

pub fn retry_vhdx_restore() -> KeeperResult<VhdxCompactStatus> {
    let mut retry = {
        let mut guard = lock_job();
        let job = guard
            .as_mut()
            .ok_or_else(|| KeeperError::SystemError("沒有需要復原的壓縮作業。".into()))?;
        if job.status.state == VhdxCompactState::Running || !job.status.recovery_required {
            return Err(KeeperError::SystemError("沒有需要復原的壓縮作業。".into()));
        }
        job.status.state = VhdxCompactState::Running;
        Job {
            status: job.status.clone(),
            restore_wsl: job.restore_wsl.clone(),
            restore_docker: job.restore_docker,
        }
    };
    let status = retry.status.clone();
    std::thread::spawn(move || {
        restore_environment(&mut retry, &NativeEnvironment, &publish);
        finish(&mut retry);
        publish(&retry);
    });
    Ok(status)
}

fn publish(job: &Job) {
    *lock_job() = Some(Job {
        status: job.status.clone(),
        restore_wsl: job.restore_wsl.clone(),
        restore_docker: job.restore_docker,
    });
}

trait Environment {
    fn inspect(&self) -> Result<CompactEnvironment, String>;
    fn stop_docker(&self, environment: &CompactEnvironment) -> Result<(), String>;
    fn stop_wsl(&self) -> Result<(), String>;
    fn compact(&self, path: &str, progress: &dyn Fn(u8)) -> Result<(), String>;
    fn start_wsl(&self, distribution: &str) -> Result<(), String>;
    fn start_docker(&self, environment: &CompactEnvironment) -> Result<(), String>;
}

fn step(job: &mut Job, id: &str, state: StepState, message: String) {
    let step = job
        .status
        .steps
        .iter_mut()
        .find(|s| s.id == id)
        .expect("known workflow step");
    step.state = state;
    step.message = message;
}

fn run_step(
    job: &mut Job,
    id: &str,
    publish: &impl Fn(&Job),
    action: impl FnOnce() -> Result<(), String>,
) -> bool {
    step(job, id, StepState::Running, String::new());
    publish(job);
    match action() {
        Ok(()) => {
            step(job, id, StepState::Succeeded, String::new());
            true
        }
        Err(error) => {
            step(job, id, StepState::Failed, error);
            false
        }
    }
}

fn run_workflow(job: &mut Job, environment: &impl Environment, publish: impl Fn(&Job)) {
    step(job, "inspect", StepState::Running, String::new());
    publish(job);
    let snapshot = match environment.inspect() {
        Ok(snapshot) => snapshot,
        Err(error) => {
            step(job, "inspect", StepState::Failed, error);
            finish(job);
            return;
        }
    };
    job.status.environment = snapshot.clone();
    step(job, "inspect", StepState::Succeeded, String::new());
    job.restore_wsl = snapshot
        .wsl_distributions
        .iter()
        .filter(|name| !is_docker_distribution(name))
        .cloned()
        .collect();
    let mut ready = true;
    if snapshot.docker_running {
        // A failed stop can still partially shut down the environment.
        job.restore_docker = true;
        ready = run_step(job, "stop_docker", &publish, || {
            environment.stop_docker(&snapshot)
        });
    } else {
        step(job, "stop_docker", StepState::Skipped, String::new());
    }
    if ready {
        // Capture before Docker stops: its shutdown may also stop user distributions.
        ready = run_step(job, "stop_wsl", &publish, || environment.stop_wsl());
    }
    if ready {
        let path = job.status.path.clone();
        ready = run_step(job, "compact", &publish, || {
            environment.compact(&path, &|percent| {
                if let Some(job) = lock_job().as_mut() {
                    job.status.progress_percent = Some(percent);
                }
            })
        });
        job.status.compact_completed = ready;
        if ready {
            job.status.progress_percent = Some(100);
            match std::fs::metadata(&path) {
                Ok(metadata) => job.status.size_after_bytes = Some(metadata.len()),
                Err(error) => step(job, "compact", StepState::Failed, error.to_string()),
            }
        }
    }
    restore_environment(job, environment, &publish);
    finish(job);
}

fn restore_environment(job: &mut Job, environment: &impl Environment, publish: &impl Fn(&Job)) {
    if job.restore_wsl.is_empty() {
        if job
            .status
            .steps
            .iter()
            .find(|s| s.id == "start_wsl")
            .is_some_and(|s| s.state != StepState::Succeeded)
        {
            step(job, "start_wsl", StepState::Skipped, String::new());
        }
    } else {
        let mut failed = Vec::new();
        let distributions = job.restore_wsl.clone();
        run_step(job, "start_wsl", publish, || {
            for distribution in &distributions {
                if let Err(error) = environment.start_wsl(distribution) {
                    failed.push((distribution.clone(), error));
                }
            }
            if failed.is_empty() {
                Ok(())
            } else {
                Err(failed
                    .iter()
                    .map(|(name, error)| format!("{name}: {error}"))
                    .collect::<Vec<_>>()
                    .join("\n"))
            }
        });
        job.restore_wsl = failed.into_iter().map(|(name, _)| name).collect();
    }
    if job.restore_docker {
        let snapshot = job.status.environment.clone();
        if run_step(job, "start_docker", publish, || {
            environment.start_docker(&snapshot)
        }) {
            job.restore_docker = false;
        }
    } else if job
        .status
        .steps
        .iter()
        .find(|s| s.id == "start_docker")
        .is_some_and(|s| s.state != StepState::Succeeded)
    {
        step(job, "start_docker", StepState::Skipped, String::new());
    }
    job.status.recovery_required = !job.restore_wsl.is_empty() || job.restore_docker;
}

fn finish(job: &mut Job) {
    for step in &mut job.status.steps {
        if step.state == StepState::Pending {
            step.state = StepState::Skipped;
        }
    }
    let errors = job
        .status
        .steps
        .iter()
        .filter(|s| s.state == StepState::Failed)
        .map(|s| s.message.clone())
        .collect::<Vec<_>>();
    job.status.message = errors.join("\n");
    job.status.state =
        if job.status.compact_completed && errors.is_empty() && !job.status.recovery_required {
            VhdxCompactState::Succeeded
        } else {
            VhdxCompactState::Failed
        };
}

fn is_docker_distribution(name: &str) -> bool {
    name.eq_ignore_ascii_case("docker-desktop") || name.eq_ignore_ascii_case("docker-desktop-data")
}

struct NativeEnvironment;

#[cfg(windows)]
mod windows {
    use super::*;
    use std::os::windows::process::CommandExt;
    use std::path::{Path, PathBuf};
    use std::process::{Command, Stdio};
    use std::time::{Duration, Instant};

    fn decode(bytes: &[u8]) -> String {
        if bytes.starts_with(&[0xff, 0xfe]) || bytes.get(1) == Some(&0) {
            let bytes = bytes.strip_prefix(&[0xff, 0xfe]).unwrap_or(bytes);
            String::from_utf16_lossy(
                &bytes
                    .as_chunks::<2>()
                    .0
                    .iter()
                    .map(|b| u16::from_le_bytes(*b))
                    .collect::<Vec<_>>(),
            )
        } else {
            String::from_utf8_lossy(bytes).into_owned()
        }
    }

    fn run(exe: &Path, args: &[&str], seconds: u64) -> Result<String, String> {
        let stdout = tempfile::tempfile().map_err(|e| e.to_string())?;
        let stderr = tempfile::tempfile().map_err(|e| e.to_string())?;
        let mut child = Command::new(exe)
            .args(args)
            .creation_flags(0x08000000)
            .stdin(Stdio::null())
            .stdout(stdout.try_clone().map_err(|e| e.to_string())?)
            .stderr(stderr.try_clone().map_err(|e| e.to_string())?)
            .spawn()
            .map_err(|e| format!("{}: {e}", exe.display()))?;
        let started = Instant::now();
        let status = loop {
            if let Some(status) = child.try_wait().map_err(|e| e.to_string())? {
                break status;
            }
            if started.elapsed() > Duration::from_secs(seconds) {
                child.kill().map_err(|e| e.to_string())?;
                child.wait().map_err(|e| e.to_string())?;
                return Err(format!("指令逾時：{} {}", exe.display(), args.join(" ")));
            }
            std::thread::sleep(Duration::from_millis(200));
        };
        use std::io::{Read, Seek, SeekFrom};
        fn read(mut file: std::fs::File) -> Result<String, String> {
            file.seek(SeekFrom::Start(0)).map_err(|e| e.to_string())?;
            let mut bytes = Vec::new();
            file.read_to_end(&mut bytes).map_err(|e| e.to_string())?;
            Ok(decode(&bytes))
        }
        let out = read(stdout)?;
        let err = read(stderr)?;
        if status.success() {
            Ok(out)
        } else {
            Err(format!(
                "{} {} ({status}): {out} {err}",
                exe.display(),
                args.join(" ")
            ))
        }
    }

    fn wsl_exe() -> PathBuf {
        PathBuf::from(std::env::var_os("SystemRoot").unwrap_or_else(|| "C:\\Windows".into()))
            .join("System32")
            .join("wsl.exe")
    }

    fn running_distributions() -> Result<Vec<String>, String> {
        let exe = wsl_exe();
        if !exe.is_file() {
            return Ok(Vec::new());
        }
        Ok(run(&exe, &["--list", "--running", "--quiet"], 30)?
            .lines()
            .map(|s| s.trim().trim_start_matches('\u{feff}').to_string())
            .filter(|s| !s.is_empty())
            .collect())
    }

    fn docker_processes() -> Result<Vec<PathBuf>, String> {
        let mut system = sysinfo::System::new();
        system.refresh_processes(sysinfo::ProcessesToUpdate::All, true);
        system
            .processes()
            .values()
            .filter(|p| {
                p.name()
                    .to_string_lossy()
                    .eq_ignore_ascii_case("Docker Desktop.exe")
                    || p.name()
                        .to_string_lossy()
                        .eq_ignore_ascii_case("com.docker.backend.exe")
            })
            .map(|p| {
                p.exe()
                    .map(Path::to_path_buf)
                    .ok_or_else(|| "找不到 Docker Desktop 執行檔，無法安全復原環境。".to_string())
            })
            .collect()
    }

    fn docker_cli(snapshot: &CompactEnvironment) -> Result<PathBuf, String> {
        snapshot
            .docker_exe
            .as_ref()
            .and_then(|exe| exe.parent())
            .map(|dir| dir.join("resources").join("bin").join("docker.exe"))
            .filter(|path| path.is_file())
            .ok_or_else(|| "找不到 Docker Desktop 指令工具，請確認安裝完整。".into())
    }

    impl Environment for NativeEnvironment {
        fn inspect(&self) -> Result<CompactEnvironment, String> {
            let processes = docker_processes()?;
            let running = !processes.is_empty();
            let exe = processes
                .iter()
                .find(|p| {
                    p.file_name()
                        .is_some_and(|n| n.eq_ignore_ascii_case("Docker Desktop.exe"))
                })
                .cloned()
                .or_else(|| {
                    processes.first().and_then(|p| p.parent()).and_then(|p| {
                        p.ancestors()
                            .take(3)
                            .map(|dir| dir.join("Docker Desktop.exe"))
                            .find(|p| p.is_file())
                    })
                });
            if running && exe.is_none() {
                return Err("找不到 Docker Desktop 執行檔，無法安全復原環境。".into());
            }
            Ok(CompactEnvironment {
                docker_running: running,
                docker_exe: exe,
                wsl_distributions: running_distributions()?,
            })
        }

        fn stop_docker(&self, snapshot: &CompactEnvironment) -> Result<(), String> {
            let cli = docker_cli(snapshot)?;
            run(&cli, &["desktop", "stop", "--timeout", "60"], 75)
                .map_err(|e| format!("無法正常結束 Docker Desktop；請確認版本支援 docker desktop stop，或結束後在精靈重試：{e}"))?;
            let started = Instant::now();
            while !docker_processes()?.is_empty() {
                if started.elapsed() > Duration::from_secs(30) {
                    return Err("Docker Desktop 仍在執行，已中止壓縮。".into());
                }
                std::thread::sleep(Duration::from_millis(500));
            }
            Ok(())
        }

        fn stop_wsl(&self) -> Result<(), String> {
            let exe = wsl_exe();
            if !exe.is_file() {
                return Ok(());
            }
            run(&exe, &["--shutdown"], 60)?;
            if !running_distributions()?.is_empty() {
                return Err("WSL 仍在執行，已中止壓縮。".into());
            }
            Ok(())
        }

        fn compact(&self, path: &str, progress: &dyn Fn(u8)) -> Result<(), String> {
            ensure_not_in_use(Path::new(path)).map_err(|e| e.to_string())?;
            run_elevated_worker(path, progress)
        }

        fn start_wsl(&self, distribution: &str) -> Result<(), String> {
            run(
                &wsl_exe(),
                &["--distribution", distribution, "--exec", "/bin/true"],
                60,
            )?;
            if !running_distributions()?
                .iter()
                .any(|name| name == distribution)
            {
                return Err("WSL 啟動後未偵測到執行中的發行版。".into());
            }
            Ok(())
        }

        fn start_docker(&self, snapshot: &CompactEnvironment) -> Result<(), String> {
            let cli = docker_cli(snapshot)?;
            if docker_processes()?.is_empty() {
                let exe = snapshot.docker_exe.as_ref().ok_or_else(|| {
                    "找不到 Docker Desktop 執行檔，無法安全復原環境。".to_string()
                })?;
                Command::new(exe)
                    .creation_flags(0x08000000)
                    .stdin(Stdio::null())
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .spawn()
                    .map_err(|e| e.to_string())?;
            }
            let started = Instant::now();
            loop {
                match run(
                    &cli,
                    &[
                        "--host",
                        "npipe:////./pipe/docker_engine",
                        "info",
                        "--format",
                        "{{.ServerVersion}}",
                    ],
                    10,
                ) {
                    Ok(_) => return Ok(()),
                    Err(error) if started.elapsed() > Duration::from_secs(120) => {
                        return Err(format!("Docker Desktop 尚未就緒：{error}"))
                    }
                    Err(_) => std::thread::sleep(Duration::from_secs(2)),
                }
            }
        }
    }

    #[cfg(test)]
    mod tests {
        use super::decode;
        #[test]
        fn decodes_wsl_unicode_output() {
            let text = "Ubuntu\r\n\u{6e2c}\u{8a66}\r\n";
            let bytes = text
                .encode_utf16()
                .flat_map(u16::to_le_bytes)
                .collect::<Vec<_>>();
            assert_eq!(decode(&bytes), text);
            assert_eq!(decode(b"Ubuntu\n"), "Ubuntu\n");
        }
    }
}

#[cfg(not(windows))]
impl Environment for NativeEnvironment {
    fn inspect(&self) -> Result<CompactEnvironment, String> {
        Err("虛擬磁碟壓縮僅支援 Windows 系統。".into())
    }
    fn stop_docker(&self, _: &CompactEnvironment) -> Result<(), String> {
        self.inspect().map(|_| ())
    }
    fn stop_wsl(&self) -> Result<(), String> {
        self.inspect().map(|_| ())
    }
    fn compact(&self, _: &str, _: &dyn Fn(u8)) -> Result<(), String> {
        self.inspect().map(|_| ())
    }
    fn start_wsl(&self, _: &str) -> Result<(), String> {
        self.inspect().map(|_| ())
    }
    fn start_docker(&self, _: &CompactEnvironment) -> Result<(), String> {
        self.inspect().map(|_| ())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    struct Fake {
        calls: RefCell<Vec<String>>,
        failures: Vec<String>,
        docker: bool,
    }
    impl Fake {
        fn call(&self, name: &str) -> Result<(), String> {
            self.calls.borrow_mut().push(name.into());
            if self.failures.iter().any(|f| f == name) {
                Err(name.into())
            } else {
                Ok(())
            }
        }
    }
    impl Environment for Fake {
        fn inspect(&self) -> Result<CompactEnvironment, String> {
            self.call("inspect")?;
            Ok(CompactEnvironment {
                docker_running: self.docker,
                wsl_distributions: vec!["Ubuntu".into(), "docker-desktop".into()],
                docker_exe: None,
            })
        }
        fn stop_docker(&self, _: &CompactEnvironment) -> Result<(), String> {
            self.call("stop_docker")
        }
        fn stop_wsl(&self) -> Result<(), String> {
            self.call("stop_wsl")
        }
        fn compact(&self, _: &str, _: &dyn Fn(u8)) -> Result<(), String> {
            self.call("compact")
        }
        fn start_wsl(&self, name: &str) -> Result<(), String> {
            assert_eq!(name, "Ubuntu");
            self.call("start_wsl")
        }
        fn start_docker(&self, _: &CompactEnvironment) -> Result<(), String> {
            self.call("start_docker")
        }
    }

    fn test_job(path: &str) -> Job {
        Job {
            status: initial_status(path, 10),
            restore_wsl: Vec::new(),
            restore_docker: false,
        }
    }

    #[test]
    fn completes_in_order_and_skips_docker_internal_distribution() {
        let file = tempfile::NamedTempFile::new().unwrap();
        let mut job = test_job(file.path().to_str().unwrap());
        let fake = Fake {
            calls: RefCell::new(Vec::new()),
            failures: Vec::new(),
            docker: true,
        };
        run_workflow(&mut job, &fake, |_| {});
        assert_eq!(
            *fake.calls.borrow(),
            [
                "inspect",
                "stop_docker",
                "stop_wsl",
                "compact",
                "start_wsl",
                "start_docker"
            ]
        );
        assert_eq!(job.status.state, VhdxCompactState::Succeeded);
        assert_eq!(job.status.progress_percent, Some(100));
    }

    #[test]
    fn failure_restores_environment_and_retry_never_compacts_again() {
        for failure in ["stop_wsl", "compact"] {
            let mut job = test_job("unused");
            let fake = Fake {
                calls: RefCell::new(Vec::new()),
                failures: vec![failure.into(), "start_wsl".into()],
                docker: true,
            };
            run_workflow(&mut job, &fake, |_| {});
            assert_eq!(job.status.state, VhdxCompactState::Failed);
            assert!(job.status.recovery_required);
            assert!(!job.restore_docker);
            assert!(fake.calls.borrow().iter().any(|c| c == "start_docker"));
            let retry = Fake {
                calls: RefCell::new(Vec::new()),
                failures: Vec::new(),
                docker: true,
            };
            restore_environment(&mut job, &retry, &|_| {});
            finish(&mut job);
            assert_eq!(*retry.calls.borrow(), ["start_wsl"]);
            assert!(!job.status.recovery_required);
            assert_eq!(job.status.state, VhdxCompactState::Failed);
        }
    }

    #[test]
    fn failed_docker_stop_does_not_shutdown_wsl_or_compact() {
        let mut job = test_job("unused");
        let fake = Fake {
            calls: RefCell::new(Vec::new()),
            failures: vec!["stop_docker".into()],
            docker: true,
        };
        run_workflow(&mut job, &fake, |_| {});
        assert_eq!(
            *fake.calls.borrow(),
            ["inspect", "stop_docker", "start_wsl", "start_docker"]
        );
        assert_eq!(job.status.state, VhdxCompactState::Failed);
    }

    #[test]
    fn does_not_start_docker_when_it_was_not_running() {
        let file = tempfile::NamedTempFile::new().unwrap();
        let mut job = test_job(file.path().to_str().unwrap());
        let fake = Fake {
            calls: RefCell::new(Vec::new()),
            failures: Vec::new(),
            docker: false,
        };
        run_workflow(&mut job, &fake, |_| {});
        assert_eq!(
            *fake.calls.borrow(),
            ["inspect", "stop_wsl", "compact", "start_wsl"]
        );
        assert_eq!(job.status.state, VhdxCompactState::Succeeded);
    }

    #[test]
    fn successful_compaction_can_retry_failed_restoration_to_completion() {
        let file = tempfile::NamedTempFile::new().unwrap();
        let mut job = test_job(file.path().to_str().unwrap());
        let fake = Fake {
            calls: RefCell::new(Vec::new()),
            failures: vec!["start_docker".into()],
            docker: true,
        };
        run_workflow(&mut job, &fake, |_| {});
        assert!(job.status.compact_completed);
        assert!(job.status.recovery_required);
        assert_eq!(job.status.state, VhdxCompactState::Failed);
        let retry = Fake {
            calls: RefCell::new(Vec::new()),
            failures: Vec::new(),
            docker: true,
        };
        restore_environment(&mut job, &retry, &|_| {});
        finish(&mut job);
        assert_eq!(*retry.calls.borrow(), ["start_docker"]);
        assert_eq!(job.status.state, VhdxCompactState::Succeeded);
        assert!(!job.status.recovery_required);
    }

    #[test]
    fn preflight_failure_changes_nothing() {
        let mut job = test_job("unused");
        let fake = Fake {
            calls: RefCell::new(Vec::new()),
            failures: vec!["inspect".into()],
            docker: true,
        };
        run_workflow(&mut job, &fake, |_| {});
        assert_eq!(*fake.calls.borrow(), ["inspect"]);
        assert!(!job.status.recovery_required);
    }
}
