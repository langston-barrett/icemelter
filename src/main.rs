use std::collections::HashMap;
use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::process::Stdio;
use std::time::Duration;

use anyhow::{Context, Result};
use clap::Parser;
use clap_verbosity_flag::{InfoLevel, Verbosity};
use regex::Regex;
use tracing::debug;
use tracing::error;
use tracing::info;
use tracing_subscriber::fmt::format::FmtSpan;
use treereduce::Check;
use treereduce::CmdCheck;
use treereduce::Config;
use treereduce::NodeTypes;
use treereduce::Original;

mod formatter;
mod github;

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
        long,
        value_name = "REGEX",
        default_value_t = String::from(r"(internal compiler error:|error: the compiler unexpectedly panicked\. this is a bug\.)")
    )]
    interesting_stderr: String,

    /// Regex to match *uninteresting* stderr, overrides interesting regex
    #[arg(long, value_name = "REGEX", requires = "interesting_stderr")]
    uninteresting_stderr: Option<String>,

    /// Number of threads
    #[arg(short, long, default_value_t = num_cpus::get())]
    jobs: usize,

    /// Also output markdown
    #[arg(long)]
    markdown: bool,

    /// Where to save reduced test case
    #[arg(short, long, default_value_os = "melted.rs")]
    output: PathBuf,

    /// Timeout (ms)
    #[arg(long, default_value_t = 2000)]
    timeout: u64,

    #[clap(flatten)]
    verbose: Verbosity<InfoLevel>,

    /// Rust source file that causes the ICE, or rust-lang/rust issue number
    #[arg(value_name = "ICE", required = true)]
    source: String,

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

fn retrieve(source: &str) -> Result<String> {
    let issue_number_rx =
        Regex::new(r"^#(\d+)").context("Internal error: bad issue number regex")?;
    match issue_number_rx.find(source) {
        None => {
            debug!("Source looks like a file");
            read_file(source)
        }
        Some(m) => {
            debug!("Source looks like an issue number");
            let issue_number_str = m.as_str();
            debug!("Match: {}", issue_number_str);
            let issue_number = issue_number_str[1..]
                .parse::<usize>()
                .context("Internal error: Couldn't extract number from issue number regex")?;
            let gh_config = github::Config::from_env().with_context(|| {
                format!("Missing {} environment variable", github::Config::ENV_VAR)
            })?;
            let issue = github::get_issue(&gh_config, issue_number)
                .context("Failed to retrieve issue from Github")?;
            debug_assert_eq!(issue.number, issue_number);
            let mut reproduction = Vec::new();
            let mut in_code_section = false;
            let mut in_code = false;
            for line in issue.body.lines() {
                if in_code {
                    if line.starts_with("```") {
                        in_code = false;
                        continue;
                    }
                    reproduction.push(line);
                }
                if line.starts_with("### Code") {
                    in_code_section = true;
                } else if line.starts_with('#') && in_code_section {
                    in_code_section = false;
                }
                if (line.starts_with("```rust") || line.starts_with("```Rust")) && in_code_section {
                    in_code = true;
                }
            }
            let reproduction_str = reproduction.join("\n");
            debug!("Reproduction:\n{}", reproduction_str);
            Ok(reproduction_str)
        }
    }
}

fn parse(language: tree_sitter::Language, code: &str) -> Result<tree_sitter::Tree> {
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(language)
        .context("Failed to set tree-sitter parser language")?;
    parser.parse(code, None).context("Failed to parse code")
}

fn check_initial_ice(chk: &CmdCheck, src: &[u8]) -> Result<(Vec<String>, String)> {
    debug!("Doing initial check for ICE");
    let state = chk
        .start(src)
        .context("Failed to check that initial input caused an ICE")?;
    let (interesting, _status, _stdout, stderr_bytes) = chk
        .wait_with_output(state)
        .context("Failed to check that initial input caused an ICE")?;
    if !interesting {
        error!("The file doesn't seem to produce an ICE.");
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
    Ok((error_codes, String::from(stderr)))
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
        error!("Internal error: empty interestingness check!");
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
    // Last is E0789, this should be safe for a bit...
    // https://doc.rust-lang.org/error_codes/error-index.html
    for n in 0..1000 {
        let code = format!("{:0>4}", n);
        if !codes.contains(&code) {
            rx += &format!("|{code}");
        }
    }
    rx += r")\]: ";
    // error: internal...
    // error: the compiler...
    format!(r"(^error: [^it]|{})", rx)
}

fn reduce(rs: &str, jobs: usize, chk: CmdCheck) -> Result<Vec<u8>> {
    let language = tree_sitter_rust::language();
    let node_types = NodeTypes::new(tree_sitter_rust::NODE_TYPES).unwrap();
    let tree = parse(language, rs).unwrap();
    let reduce_config = Config {
        check: chk,
        jobs,
        min_reduction: 1,
        replacements: HashMap::new(),
    };
    let (reduced, _) = treereduce::treereduce_multi_pass(
        language,
        node_types,
        Original::new(tree, rs.as_bytes().to_vec()),
        reduce_config,
        None, // max passes
    )
    .context("Failed when reducing the program")?;
    Ok(reduced.text)
}

// NB: errors from this function are ignored as non-fatal
// TODO: Strip leading/trailing whitespace
fn fmt(check: &CmdCheck, file: &[u8]) -> Result<Option<Vec<u8>>> {
    debug!("Formatting reduced file with rustfmt");
    let tmp = tempfile::Builder::new()
        .prefix("icemelter")
        .suffix(".rs")
        .tempfile()?;
    let path = tmp.path();
    std::fs::write(path, file)?;
    Command::new("rustfmt")
        .arg(path)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()?;
    let formatted = std::fs::read(path)?;
    if check.interesting(&formatted)? {
        Ok(Some(formatted))
    } else {
        Ok(None)
    }
}

fn markdown(to: PathBuf, file: Vec<u8>) -> Result<()> {
    let s = String::from_utf8(file).context("When writing Markdown")?;
    std::fs::write(
        &to,
        format!(
            "Minimized with [Icemelter](https://github.com/langston-barrett/icemelter):
```rust
{}
```

<details><summary>Details</summary>
<p>

Icemelter version: v{}

@rustbot label +S-bug-has-mcve

</p>
</details>

            ",
            s,
            env!("CARGO_PKG_VERSION")
        ),
    )
    .with_context(|| format!("When writing Markdown report to {}", to.display()))
}

pub fn main() -> Result<()> {
    let args = Args::parse();
    init_tracing(&args);
    let timeout = Duration::from_millis(args.timeout);

    debug!("Step 1/4: Retrieving...");
    let rs = retrieve(&args.source)?;

    debug!("Step 2/4: Configuring...");
    let initial_check = check(
        args.debug,
        timeout,
        args.check.clone(),
        Some(args.interesting_stderr.clone()),
        args.uninteresting_stderr.clone(),
    )?;
    let uninteresting_stderr = if args.allow_errors {
        args.uninteresting_stderr
    } else {
        let (error_codes, initial_stderr) = check_initial_ice(&initial_check, rs.as_bytes())?;
        for error_code in &error_codes {
            debug!("Found error code {}", error_code);
        }
        let fresh_error_regex = error_regex(HashSet::from_iter(error_codes));
        let uninteresting_regex = match args.uninteresting_stderr {
            Some(u) => format!("(?m)({}|{})", u, fresh_error_regex),
            None => format!("(?m){}", fresh_error_regex),
        };
        debug!("Initial stderr: {}", initial_stderr);
        debug!("Error regex: {}", uninteresting_regex);
        debug_assert!(!Regex::new(&uninteresting_regex)
            .unwrap()
            .is_match(&initial_stderr));
        Some(uninteresting_regex)
    };

    debug!("Step 3/4: Reducing...");
    let chk = check(
        args.debug,
        timeout,
        args.check,
        Some(args.interesting_stderr),
        uninteresting_stderr,
    )?;
    let reduced =
        reduce(&rs, args.jobs, chk.clone()).context("Failed when reducing the program")?;
    if reduced == rs.as_bytes() {
        if args.allow_errors {
            info!("Unable to reduce! Sorry.");
            info!("If you think this test case is reducible, please file an issue!");
        } else {
            info!("Unable to reduce, try --allow-errors.");
        }
    }

    debug!("Step 4/4: Formatting...");
    let formatted = match fmt(&chk, &reduced) {
        Err(_) | Ok(None) => {
            debug!("Failed to reduce with rustfmt");
            reduced
        }
        Ok(Some(formatted)) => formatted,
    };
    std::fs::write(&args.output, &formatted)
        .with_context(|| format!("Failed to write reduced file to {}", args.output.display()))?;
    info!("Reduced file written to {}", args.output.display());

    if args.markdown {
        markdown(args.output.with_extension("md"), formatted)?;
    }

    Ok(())
}
