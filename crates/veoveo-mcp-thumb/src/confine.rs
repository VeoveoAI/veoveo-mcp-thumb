//! The `confine` subcommand: a renderer's narrower sandbox, inside the one OpenShell gives the
//! server. A renderer parses a stranger's file, so it may be made to run anything. Before it starts
//! this process limits itself with Landlock, which needs no privilege and which every child
//! inherits, and then becomes it: it reads the system's programs and its own job's folder, writes
//! only that folder, opens no TCP connection and listens on no port. So the server's key, the
//! other jobs' files and the network stay out of its reach, and Landlock also keeps it from
//! reading the server's memory or environment.
//!
//! ```sh
//! veoveo-mcp-thumb confine JOB [--gpu] [--tmp] -- PROGRAM [ARGUMENT]...
//! ```

#[cfg(target_os = "linux")]
use anyhow::Context as _;
use anyhow::{Result, bail};

/// What a renderer reads: programs, libraries, fonts, its configuration and the system's facts.
#[cfg(target_os = "linux")]
const READABLE: [&str; 9] = [
    "/usr",
    "/lib",
    "/lib64",
    "/bin",
    "/sbin",
    "/etc",
    "/proc",
    "/sys",
    "/dev/urandom",
];
/// The GPU's device nodes and its driver's socket, for a renderer that draws on it.
#[cfg(target_os = "linux")]
const GPU: [&str; 7] = [
    "/dev/nvidiactl",
    "/dev/nvidia0",
    "/dev/nvidia-uvm",
    "/dev/nvidia-uvm-tools",
    "/dev/nvidia-modeset",
    "/dev/dri",
    "/run/nvidia-persistenced",
];
/// The most memory a renderer that draws no GPU may map, past which it fails rather than take the
/// sandbox's memory from the server.
#[cfg(target_os = "linux")]
const MAX_ADDRESS_SPACE: u64 = 6 * 1024 * 1024 * 1024;

/// What the subcommand's arguments say.
#[derive(Debug, PartialEq, Eq)]
struct Asked<'a> {
    job: &'a str,
    gpu: bool,
    /// It may also write `/tmp`. LibreOffice puts its one pipe there, wherever else it is told to
    /// work, and fails without it.
    tmp: bool,
    program: &'a str,
    arguments: &'a [String],
}

fn parse(args: &[String]) -> Result<Asked<'_>> {
    let usage = "confine JOB [--gpu] [--tmp] -- PROGRAM [ARGUMENT]...";
    let Some(separator) = args.iter().position(|arg| arg == "--") else {
        bail!("usage: {usage}");
    };
    let (before, after) = args.split_at(separator);
    let (job, gpu, tmp) = match before {
        [job] => (job, false, false),
        [job, flag] if flag == "--gpu" => (job, true, false),
        [job, flag] if flag == "--tmp" => (job, false, true),
        _ => bail!("usage: {usage}"),
    };
    let Some((program, arguments)) = after[1..].split_first() else {
        bail!("usage: {usage}");
    };
    Ok(Asked {
        job,
        gpu,
        tmp,
        program,
        arguments,
    })
}

/// Limits this process as `args` says, then becomes the renderer. It returns only by failing, and
/// a renderer never runs unlimited, because no limit is optional here.
pub fn run(args: &[String]) -> Result<()> {
    let asked = parse(args)?;
    become_renderer(&asked)
}

#[cfg(target_os = "linux")]
fn become_renderer(asked: &Asked<'_>) -> Result<()> {
    use std::os::unix::process::CommandExt as _;

    limit(asked).context("limiting the renderer")?;
    let error = std::process::Command::new(asked.program)
        .args(asked.arguments)
        .exec();
    bail!("starting {}: {error}", asked.program);
}

#[cfg(not(target_os = "linux"))]
fn become_renderer(_: &Asked<'_>) -> Result<()> {
    bail!("a renderer is confined with Landlock, which only Linux has");
}

#[cfg(target_os = "linux")]
fn limit(asked: &Asked<'_>) -> Result<()> {
    use landlock::{
        ABI, Access, AccessFs, AccessNet, CompatLevel, Compatible, RestrictionStatus, Ruleset,
        RulesetAttr, RulesetCreatedAttr, RulesetStatus, path_beneath_rules,
    };
    use rustix::process::{Resource, Rlimit, setrlimit};

    let abi = ABI::V4;
    let everything = AccessFs::from_all(abi);
    let reading = AccessFs::from_read(abi);
    let exists = |paths: &[&'static str]| -> Vec<&'static str> {
        paths
            .iter()
            .copied()
            .filter(|path| std::path::Path::new(path).exists())
            .collect()
    };
    let mut created = Ruleset::default()
        // Refused, rather than quietly less, on a kernel that cannot do all of this.
        .set_compatibility(CompatLevel::HardRequirement)
        .handle_access(everything)?
        .handle_access(AccessNet::BindTcp | AccessNet::ConnectTcp)?
        .create()?
        .add_rules(path_beneath_rules(exists(&READABLE), reading))?
        .add_rules(path_beneath_rules([asked.job], everything))?
        .add_rules(path_beneath_rules(["/dev/null"], everything))?;
    if asked.tmp {
        created = created.add_rules(path_beneath_rules(["/tmp"], everything))?;
    }
    if asked.gpu {
        created = created.add_rules(path_beneath_rules(exists(&GPU), everything))?;
    } else {
        let limit = Rlimit {
            current: Some(MAX_ADDRESS_SPACE),
            maximum: Some(MAX_ADDRESS_SPACE),
        };
        setrlimit(Resource::As, limit)?;
    }
    setrlimit(
        Resource::Core,
        Rlimit {
            current: Some(0),
            maximum: Some(0),
        },
    )?;
    let RestrictionStatus { ruleset, .. } = created.restrict_self()?;
    anyhow::ensure!(
        ruleset == RulesetStatus::FullyEnforced,
        "Landlock enforced only part of the limits"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strings(args: &[&str]) -> Vec<String> {
        args.iter().map(|arg| (*arg).to_owned()).collect()
    }

    #[test]
    fn the_arguments_name_a_job_and_a_program() {
        let args = strings(&["/j", "--", "convert", "a", "--", "b"]);
        let asked = parse(&args).unwrap();
        assert_eq!(asked.job, "/j");
        assert!(!asked.gpu);
        assert_eq!(asked.program, "convert");
        assert_eq!(asked.arguments, ["a", "--", "b"]);

        let args = strings(&["/j", "--gpu", "--", "f3d"]);
        let asked = parse(&args).unwrap();
        assert!(asked.gpu);
        assert!(!asked.tmp);
        assert!(asked.arguments.is_empty());

        let args = strings(&["/j", "--tmp", "--", "soffice"]);
        let asked = parse(&args).unwrap();
        assert!(asked.tmp);
        assert!(!asked.gpu);

        for wrong in [
            strings(&["/j", "convert"]),
            strings(&["--", "convert"]),
            strings(&["/j", "--"]),
            strings(&["/j", "--network", "--", "curl"]),
        ] {
            assert!(parse(&wrong).is_err(), "{wrong:?}");
        }
    }
}
