use std::ffi::OsStr;
use std::fs::File;
use std::os::unix::process::CommandExt;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::{scope, sleep};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const IMAGE: &str = concat!("ghcr.io/codeh007/mtm-gh-runner:", env!("CARGO_PKG_VERSION"));
const DIND: &str =
    "docker:29.3.0-dind@sha256:1ba18449911d01f477a3fc104123c85d677addc60701b14b3fcb5381f9c4a537";
const PREFIX: &str = "mtm-gh-runner";
const CPU_SHARES: &str = "512";
const LOCK: &str = "/tmp/mtm-gh-runner.lock";

static STOPPING: AtomicBool = AtomicBool::new(false);

type Result<T> = std::result::Result<T, String>;

struct RunnerConfig {
    repository: String,
    labels: Vec<String>,
    runner_group_id: u64,
}

impl RunnerConfig {
    fn from_env() -> Result<Self> {
        let repository = std::env::var("MTM_GH_RUNNER_REPOSITORY")
            .map_err(|_| "MTM_GH_RUNNER_REPOSITORY is required")?;
        let labels = std::env::var("MTM_GH_RUNNER_LABELS")
            .map_err(|_| "MTM_GH_RUNNER_LABELS is required")?;
        let runner_group_id = std::env::var("MTM_GH_RUNNER_GROUP_ID")
            .map_err(|_| "MTM_GH_RUNNER_GROUP_ID is required")?;
        Self::parse(&repository, &labels, &runner_group_id)
    }

    fn parse(repository: &str, labels: &str, runner_group_id: &str) -> Result<Self> {
        let mut parts = repository.split('/');
        let owner = parts.next().unwrap_or_default();
        let name = parts.next().unwrap_or_default();
        if owner.is_empty() || name.is_empty() || parts.next().is_some() {
            return Err(format!(
                "MTM_GH_RUNNER_REPOSITORY={repository}: want owner/repo"
            ));
        }
        let valid_owner = owner
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-');
        let valid_name = name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'));
        if !valid_owner || !valid_name || name == "." || name == ".." {
            return Err(format!(
                "MTM_GH_RUNNER_REPOSITORY={repository}: want owner/repo"
            ));
        }

        let labels = labels
            .split(',')
            .map(str::trim)
            .map(str::to_string)
            .collect::<Vec<_>>();
        if labels.is_empty() || labels.iter().any(String::is_empty) {
            return Err(
                "MTM_GH_RUNNER_LABELS must contain non-empty comma-separated labels".into(),
            );
        }

        let runner_group_id = runner_group_id
            .parse::<u64>()
            .ok()
            .filter(|id| *id > 0)
            .ok_or(format!(
                "MTM_GH_RUNNER_GROUP_ID={runner_group_id}: want a positive integer"
            ))?;

        Ok(Self {
            repository: repository.to_string(),
            labels,
            runner_group_id,
        })
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args == ["--version"] {
        println!("mtm-gh-runner {}", env!("CARGO_PKG_VERSION"));
        return;
    }
    if args != ["run"] {
        eprintln!("usage: mtm-gh-runner run");
        std::process::exit(2);
    }
    if let Err(error) = run() {
        eprintln!("mtm-gh-runner: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    if let Err(error) = dotenvy::dotenv()
        && !error.not_found()
    {
        return Err(format!(".env: {error}"));
    }
    let config = RunnerConfig::from_env()?;
    let slots = concurrency()?;
    let host = hostname()?;
    let lock = File::create(LOCK).map_err(|error| format!("{LOCK}: {error}"))?;
    lock.try_lock()
        .map_err(|_| {
            format!(
                "another instance is running (holds {LOCK}); stop it with `pkill -INT -f 'release/mtm-gh-runner run'`, then start again"
            )
        })?;
    ctrlc::set_handler(move || {
        STOPPING.store(true, Ordering::SeqCst);
        for slot in 0..slots {
            let _ = quiet("docker", &["kill", "--signal", "INT", &runner(slot)]);
        }
    })
    .map_err(|error| error.to_string())?;
    let leftovers = capture(
        "docker",
        [
            "ps",
            "--all",
            "--quiet",
            "--filter",
            &format!("name=^{PREFIX}"),
        ],
    )?;
    if !leftovers.is_empty() {
        capture(
            "docker",
            ["rm", "--force", "--volumes"]
                .into_iter()
                .chain(leftovers.lines()),
        )?;
    }
    eprintln!("mtm-gh-runner: {slots} slot(s)");
    scope(|threads| {
        for slot in 0..slots {
            let host = &host;
            let config = &config;
            threads.spawn(move || serve(host, config, slot));
        }
    });
    Ok(())
}

fn concurrency() -> Result<usize> {
    if let Ok(value) = std::env::var("MTM_GH_RUNNER_CONCURRENCY") {
        return value.parse().ok().filter(|&slots| slots > 0).ok_or(format!(
            "MTM_GH_RUNNER_CONCURRENCY={value}: want a positive integer"
        ));
    }
    let cpus = std::thread::available_parallelism().map_or(1, |cpus| cpus.get());
    let memory_gib = std::fs::read_to_string("/proc/meminfo")
        .ok()
        .and_then(|meminfo| {
            meminfo
                .lines()
                .find_map(|line| line.strip_prefix("MemTotal:"))?
                .trim()
                .trim_end_matches("kB")
                .trim()
                .parse::<usize>()
                .ok()
        })
        .map_or(0, |kib| kib >> 20);
    Ok((cpus / 2).min(memory_gib / 8).max(1))
}

fn serve(host: &str, config: &RunnerConfig, slot: usize) {
    while !STOPPING.load(Ordering::SeqCst) {
        if let Err(error) = iteration(host, config, slot) {
            eprintln!("mtm-gh-runner[{slot}]: {error}; retrying in 30s");
            for _ in 0..30 {
                if STOPPING.load(Ordering::SeqCst) {
                    break;
                }
                sleep(Duration::from_secs(1));
            }
        }
    }
}

fn iteration(host: &str, config: &RunnerConfig, slot: usize) -> Result<()> {
    let (id, jit) = jit_config(config, &format!("{host}-{slot}-{}", unix_seconds()))?;
    let session = session(slot, &jit);
    let _ = quiet(
        "gh",
        &[
            "api",
            "--method",
            "DELETE",
            &format!("repos/{}/actions/runners/{id}", config.repository),
        ],
    );
    let removed = capture(
        "docker",
        ["rm", "--force", "--volumes", &runner(slot), &dind(slot)],
    );
    session.and(removed).map(drop)
}

fn runner(slot: usize) -> String {
    format!("{PREFIX}-{slot}")
}

fn dind(slot: usize) -> String {
    format!("{PREFIX}-{slot}-dind")
}

fn jit_config(config: &RunnerConfig, name: &str) -> Result<(String, String)> {
    let mut request = vec![
        "api".to_string(),
        "--method".to_string(),
        "POST".to_string(),
        format!(
            "repos/{}/actions/runners/generate-jitconfig",
            config.repository
        ),
        "-f".to_string(),
        format!("name={name}"),
        "-F".to_string(),
        format!("runner_group_id={}", config.runner_group_id),
        "--jq".to_string(),
        ".runner.id, .encoded_jit_config".to_string(),
    ];
    request.extend(
        config
            .labels
            .iter()
            .flat_map(|label| ["-f".to_string(), format!("labels[]={label}")]),
    );
    let output = capture("gh", request)?;
    let (id, jit) = output
        .split_once('\n')
        .ok_or("unexpected generate-jitconfig response")?;
    Ok((id.to_string(), jit.to_string()))
}

fn session(slot: usize, jit: &str) -> Result<()> {
    let dind = dind(slot);
    capture(
        "docker",
        [
            "run",
            "--detach",
            "--privileged",
            "--name",
            &dind,
            "--memory",
            "2g",
            "--cpu-shares",
            CPU_SHARES,
            DIND,
            "dockerd",
            "--host=unix:///var/run/docker.sock",
            "--host=tcp://127.0.0.1:2375",
            "--tls=false",
        ],
    )?;
    let ready = (0..60).any(|_| {
        let up = quiet("docker", &["exec", &dind, "docker", "info"]);
        if !up {
            sleep(Duration::from_secs(1));
        }
        up
    });
    if !ready {
        return Err("dind did not become ready".into());
    }
    if STOPPING.load(Ordering::SeqCst) {
        return Ok(());
    }
    let status = Command::new("docker")
        .args([
            "run",
            "--name",
            &runner(slot),
            "--network",
            &format!("container:{dind}"),
            "--volume",
            &format!("{PREFIX}-cache-{slot}:/opt/cache"),
            "--memory",
            "6g",
            "--cpu-shares",
            CPU_SHARES,
            "--env",
            "DOCKER_HOST=tcp://127.0.0.1:2375",
            "--env",
            "ACTIONS_RUNNER_PRINT_LOG_TO_STDOUT=0",
            "--env",
            "ACTIONS_RUNNER_INPUT_JITCONFIG",
            IMAGE,
            "/home/runner/run.sh",
        ])
        .env("ACTIONS_RUNNER_INPUT_JITCONFIG", jit)
        .process_group(0)
        .status()
        .map_err(|error| format!("docker: {error}"))?;
    if status.success() || STOPPING.load(Ordering::SeqCst) {
        Ok(())
    } else {
        Err(format!("runner exited with {status}"))
    }
}

fn quiet(program: &str, args: &[&str]) -> bool {
    Command::new(program)
        .args(args)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

fn capture<I, S>(program: &str, args: I) -> Result<String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let output = Command::new(program)
        .args(args)
        .stderr(Stdio::inherit())
        .output()
        .map_err(|error| format!("{program}: {error}"))?;
    if !output.status.success() {
        return Err(format!("{program} exited with {}", output.status));
    }
    String::from_utf8(output.stdout)
        .map(|stdout| stdout.trim().to_string())
        .map_err(|error| error.to_string())
}

fn hostname() -> Result<String> {
    std::fs::read_to_string("/proc/sys/kernel/hostname")
        .map(|name| name.trim().to_string())
        .map_err(|error| format!("hostname: {error}"))
}

fn unix_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs())
}

#[cfg(test)]
mod tests {
    use super::RunnerConfig;

    #[test]
    fn parses_repository_labels_and_group() {
        let config = RunnerConfig::parse("owner/repo", "self-hosted, linux, x64", "7").unwrap();

        assert_eq!(config.repository, "owner/repo");
        assert_eq!(config.labels, ["self-hosted", "linux", "x64"]);
        assert_eq!(config.runner_group_id, 7);
    }

    #[test]
    fn rejects_invalid_repository() {
        assert!(RunnerConfig::parse("owner", "linux", "1").is_err());
        assert!(RunnerConfig::parse("owner/repo/extra", "linux", "1").is_err());
        assert!(RunnerConfig::parse("owner/repo?target=other", "linux", "1").is_err());
    }

    #[test]
    fn rejects_empty_labels_and_invalid_group() {
        assert!(RunnerConfig::parse("owner/repo", "linux,,x64", "1").is_err());
        assert!(RunnerConfig::parse("owner/repo", "linux", "0").is_err());
    }
}
