use std::collections::HashMap;
use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;
use std::time::Duration;

use anyhow::{Context, Result};
use clap::Parser;
use clap_verbosity_flag::{InfoLevel, Verbosity};
use regex::Regex;
use tracing::debug;
use tracing_subscriber::fmt::format::FmtSpan;
use treereduce::Check;
use treereduce::CmdCheck;
use treereduce::Config;
use treereduce::NodeTypes;
use treereduce::Original;

mod formatter;

/// A tool to minimize Rust files that trigger internal compiler errors (ICEs)
#[derive(Clone, Debug, clap::Parser)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Allow introducing type/syntax/borrow errors to achieve smaller tests
    #[arg(long)]
    allow_errors: bool,

    /// Run a single thread and show stdout, stderr of rustc
    #[arg(short, long)]
    debug: bool,

    /// Regex to match stderr
    #[arg(
        help_heading = "Interestingness check options",
        long,
        value_name = "REGEX",
        default_value_t = String::from("(internal compiler error:|error: the compiler unexpectedly panicked. this is a bug.)")
    )]
    interesting_stderr: String,

    /// Regex to match *uninteresting* stderr, overrides interesting regex
    #[arg(
        help_heading = "Interestingness check options",
        long,
        value_name = "REGEX",
        requires = "interesting_stderr"
    )]
    uninteresting_stderr: Option<String>,

    /// Number of threads
    #[arg(short, long, default_value_t = num_cpus::get())]
    jobs: usize,

    /// Directory to output to
    #[arg(short, long, default_value_os = "melted.rs")]
    output: PathBuf,

    /// Timeout (ms)
    #[arg(long, default_value_t = 2000)]
    timeout: u64,

    #[clap(flatten)]
    verbose: Verbosity<InfoLevel>,

    /// Rust source file that causes the ICE
    #[arg(value_name = "RUSTSRC", required = true)]
    file: String,

    /// rustc command line (without the file)
    #[arg(value_name = "CMD", default_values_t = vec![String::from("rustc")], num_args = 1..)]
    check: Vec<String>,
}

#[inline]
fn log_tracing_level(level: &log::Level) -> tracing::Level {
    match level {
        log::Level::Trace => tracing::Level::TRACE,
        log::Level::Debug => tracing::Level::DEBUG,
        log::Level::Info => tracing::Level::INFO,
        log::Level::Warn => tracing::Level::WARN,
        log::Level::Error => tracing::Level::ERROR,
    }
}

#[inline]
fn init_tracing(args: &Args) {
    let builder = tracing_subscriber::fmt::fmt()
        .with_span_events(FmtSpan::ENTER | FmtSpan::CLOSE)
        .with_target(false)
        .with_max_level(log_tracing_level(
            &args.verbose.log_level().unwrap_or(log::Level::Info),
        ));
    builder.event_format(formatter::TerseFormatter).init();
}

fn read_file(file: &str) -> Result<String> {
    fs::read_to_string(file).with_context(|| format!("Failed to read file {}", file))
}

fn parse(language: tree_sitter::Language, code: &str) -> Result<tree_sitter::Tree> {
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(language)
        .context("Failed to set tree-sitter parser language")?;
    parser.parse(code, None).context("Failed to parse code")
}

fn check_initial_ice(chk: &CmdCheck, src: &[u8]) -> Result<Vec<String>> {
    let state = chk
        .start(src)
        .context("Failed to check that initial input caused an ICE")?;
    let (interesting, _status, _stdout, stderr_bytes) = chk
        .wait_with_output(state)
        .context("Failed to check that initial input caused an ICE")?;
    if !interesting {
        eprintln!("The file doesn't seem to produce an ICE.");
        std::process::exit(1);
    }
    let error_code_regex =
        Regex::new(r"(?m)^error\[E(?P<code>\d\d\d\d)\]: ").context("Internal error: Bad regex?")?;
    let stderr = String::from_utf8_lossy(&stderr_bytes);
    let mut error_codes = Vec::new();
    for capture in error_code_regex.captures_iter(&stderr) {
        error_codes.push(String::from(
            capture
                .name("code")
                .context("Internal error: bad capture group name")?
                .as_str(),
        ));
    }
    Ok(error_codes)
}

#[allow(clippy::too_many_arguments)]
fn check(
    debug: bool,
    timeout: Duration,
    check: Vec<String>,
    interesting_stderr: Option<String>,
    uninteresting_stderr: Option<String>,
) -> Result<CmdCheck> {
    if check.is_empty() {
        eprintln!("Internal error: empty interestingness check!");
        std::process::exit(1);
    }
    let mut argv = check;
    let cmd = argv[0].clone();
    argv.remove(0);
    argv.push(String::from("@@.rs"));
    let stderr_regex = match &interesting_stderr {
        Some(r) => Some(Regex::new(r).context("Invalid interesting stderr regex")?),
        None => None,
    };
    let un_stderr_regex = match &uninteresting_stderr {
        Some(r) => Some(Regex::new(r).context("Invalid uninteresting stderr regex")?),
        None => None,
    };
    Ok(CmdCheck::new(
        cmd,
        argv.iter().map(|s| s.to_string()).collect(),
        Vec::new(), // interesting exit codes
        None,
        None, // interesting stdout regex
        stderr_regex,
        None, // uninteresting stdout regex
        un_stderr_regex,
        debug,
        debug,
        Some(timeout),
    ))
}

// Regex to match errors other than those in the set
fn error_regex(codes: HashSet<String>) -> String {
    let mut rx = String::from(r"^error\[E(0000");
    for n in 0..2000 {
        let code = format!("{:0>4}", n);
        if !codes.contains(&code) {
            rx += &format!("|{code}");
        }
    }
    rx += r")\]: ";
    format!(r"(^error: [^i]|{})", rx)
}

pub fn main() -> Result<()> {
    let args = Args::parse();
    init_tracing(&args);
    let language = tree_sitter_rust::language();
    let rs = read_file(&args.file)?;
    let timeout = Duration::from_millis(args.timeout);
    let initial_check = check(
        args.debug,
        timeout,
        args.check.clone(),
        Some(args.interesting_stderr.clone()),
        args.uninteresting_stderr.clone(),
    )?;

    // New check: Don't introduce spurrious errors
    let uninteresting_stderr = if args.allow_errors {
        args.uninteresting_stderr
    } else {
        let error_codes = check_initial_ice(&initial_check, rs.as_bytes())?;
        for error_code in &error_codes {
            debug!("Found error code {}", error_code);
        }
        let fresh_error_regex = error_regex(HashSet::from_iter(error_codes));
        let uninteresting_regex = match args.uninteresting_stderr {
            Some(u) => format!("(?m)({}|{})", u, fresh_error_regex),
            None => format!("(?m){}", fresh_error_regex),
        };
        debug!("Error regex: {}", uninteresting_regex);
        Some(uninteresting_regex)
    };
    let chk = check(
        args.debug,
        timeout,
        args.check,
        Some(args.interesting_stderr),
        uninteresting_stderr,
    )?;

    let node_types = NodeTypes::new(tree_sitter_rust::NODE_TYPES).unwrap();
    let tree = parse(language, &rs).unwrap();
    match treereduce::treereduce_multi_pass(
        language,
        node_types,
        Original::new(tree, rs.into_bytes()),
        Config {
            check: chk,
            jobs: args.jobs,
            min_reduction: 1,
            replacements: HashMap::new(),
        },
        None, // max passes
    ) {
        Err(e) => eprintln!("Failed to reduce! {e}"),
        Ok((reduced, _)) => {
            std::fs::write(&args.output, reduced.text).with_context(|| {
                format!("Failed to write reduced file to {}", args.output.display())
            })?;
        }
    }

    Ok(())
}
