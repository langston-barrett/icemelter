use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::time::Duration;

use anyhow::{Context, Result};
use clap::Parser;
use clap_verbosity_flag::{InfoLevel, Verbosity};
use regex::Regex;
use treereduce::Check;
use treereduce::CmdCheck;
use treereduce::Config;
use treereduce::NodeTypes;
use treereduce::Original;

/// A tool to minimize Rust files that trigger internal compiler errors (ICEs)
#[derive(Clone, Debug, clap::Parser)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Run a single thread and show stdout, stderr of rustc
    #[arg(short, long)]
    debug: bool,

    /// Regex to match stderr
    #[arg(
        help_heading = "Interestingness check options",
        long,
        value_name = "REGEX",
        default_value_t = String::from("internal compiler error:")
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

// TODO: Collect initial warnings/errors from stderr
fn check_initial_ice(chk: &CmdCheck, src: &[u8]) -> Result<()> {
    if !chk
        .interesting(src)
        .context("Failed to check that initial input caused an ICE")?
    {
        eprintln!("The file doesn't seem to produce an ICE.");
        std::process::exit(1);
    }
    Ok(())
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

pub fn main() -> Result<()> {
    let args = Args::parse();
    let language = tree_sitter_rust::language();
    let rs = read_file(&args.file)?;
    let chk = check(
        args.debug,
        Duration::from_millis(args.timeout),
        args.check,
        Some(args.interesting_stderr),
        args.uninteresting_stderr,
    )?;
    check_initial_ice(&chk, rs.as_bytes())?;
    let node_types = NodeTypes::new(tree_sitter_rust::NODE_TYPES).unwrap();
    let tree = parse(language, &rs).unwrap();
    match treereduce::treereduce_multi_pass(
        language,
        node_types,
        Original::new(tree, rs.into_bytes()),
        Config {
            check: chk,
            jobs: args.jobs,
            min_reduction: 2,
            replacements: HashMap::new(),
        },
        Some(8),
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
