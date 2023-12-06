//! The logic for the Wasmer CLI tool.

#[cfg(target_os = "linux")]
use crate::commands::Binfmt;
#[cfg(feature = "compiler")]
use crate::commands::Compile;
#[cfg(any(feature = "static-artifact-create", feature = "wasmer-artifact-create"))]
use crate::commands::CreateExe;
#[cfg(feature = "wast")]
use crate::commands::Wast;
use crate::commands::{
    Add, Cache, Config, Init, Inspect, Login, Package, Publish, Run, SelfUpdate, Validate, Whoami,
};
#[cfg(feature = "static-artifact-create")]
use crate::commands::{CreateObj, GenCHeader};
use crate::error::PrettyError;
use clap::{CommandFactory, Parser};
use std::env;
use std::fs::OpenOptions;
use std::io::Write;
use std::time::SystemTime;
use wasmer_edge_cli::cmd::CliCommand;

/// The main function for the Wasmer CLI tool.
pub fn wasmer_main() {
    // We allow windows to print properly colors
    #[cfg(windows)]
    colored::control::set_virtual_terminal(true).unwrap();

    PrettyError::report(wasmer_main_inner())
}

fn wasmer_main_inner() -> Result<(), anyhow::Error> {
    // find out which benchmark we are running
    let bmark = match env::var("WABENCHMARK") {
        Ok(s) => s,
        Err(_) => String::from("unknown"),
    };

    let ts = match SystemTime::now().duration_since(SystemTime::UNIX_EPOCH) {
        Ok(n) => n.as_micros(),
        Err(_) => panic!("Cannot get timestamp"),
    };

    let filename = env::var("WABENCH_FILE");
    match filename {
        Ok(f) => {
            let mut file = OpenOptions::new()
                .create(true)
                .write(true)
                .append(true)
                .open(f)
                .unwrap();
            writeln!(file, "WABENCH, {}, Wasmer start, timestamp, {}", bmark, ts)?;
        }
        Err(_) => println!("WABENCH, {}, Wasmer start, timestamp, {}", bmark, ts),
    };

    if is_binfmt_interpreter() {
        Run::from_binfmt_args().execute(crate::logging::Output::default());
    }

    match Args::try_parse() {
        Ok(args) => args.execute(),
        Err(e) => {
            let might_be_wasmer_run = matches!(
                e.kind(),
                clap::error::ErrorKind::InvalidSubcommand | clap::error::ErrorKind::UnknownArgument
            );

            if might_be_wasmer_run {
                if let Ok(run) = Run::try_parse() {
                    // Try to parse the command using the `wasmer some/package`
                    // shorthand. Note that this has discoverability issues
                    // because it's not shown as part of the main argument
                    // parser's help, but that's fine.
                    let output = crate::logging::Output::default();
                    output.initialize_logging();
                    run.execute(output);
                }
            }

            e.exit();
        }
    }
}

/// Command-line arguments for the Wasmer CLI.
#[derive(Parser, Debug)]
#[clap(author, version)]
#[clap(disable_version_flag = true)] // handled manually
#[cfg_attr(feature = "headless", clap(
    name = "wasmer-headless",
    about = concat!("wasmer-headless ", env!("CARGO_PKG_VERSION")),
))]
#[cfg_attr(not(feature = "headless"), clap(
    name = "wasmer",
    about = concat!("wasmer ", env!("CARGO_PKG_VERSION")),
))]
pub struct Args {
    /// Print version info and exit.
    #[clap(short = 'V', long)]
    version: bool,
    #[clap(flatten)]
    output: crate::logging::Output,
    #[clap(subcommand)]
    cmd: Option<Cmd>,
}

impl Args {
    fn execute(self) -> Result<(), anyhow::Error> {
        let Args {
            cmd,
            version,
            output,
        } = self;

        output.initialize_logging();

        if version {
            return print_version(output.is_verbose());
        }

        match cmd {
            Some(Cmd::Run(options)) => options.execute(output),
            Some(Cmd::SelfUpdate(options)) => options.execute(),
            Some(Cmd::Cache(cache)) => cache.execute(),
            Some(Cmd::Validate(validate)) => validate.execute(),
            #[cfg(feature = "compiler")]
            Some(Cmd::Compile(compile)) => compile.execute(),
            #[cfg(any(feature = "static-artifact-create", feature = "wasmer-artifact-create"))]
            Some(Cmd::CreateExe(create_exe)) => create_exe.execute(),
            #[cfg(feature = "static-artifact-create")]
            Some(Cmd::CreateObj(create_obj)) => create_obj.execute(),
            Some(Cmd::Config(config)) => config.execute(),
            Some(Cmd::Inspect(inspect)) => inspect.execute(),
            Some(Cmd::Init(init)) => init.execute(),
            Some(Cmd::Login(login)) => login.execute(),
            Some(Cmd::Publish(publish)) => publish.execute(),
            Some(Cmd::Package(cmd)) => match cmd {
                Package::Download(cmd) => cmd.execute(),
                Package::Build(cmd) => cmd.execute(),
            },
            Some(Cmd::Container(cmd)) => match cmd {
                crate::commands::Container::Unpack(cmd) => cmd.execute(),
            },
            /*
            Some(Cmd::Connect(connect)) => connect.execute(),
            */
            #[cfg(feature = "static-artifact-create")]
            Some(Cmd::GenCHeader(gen_heder)) => gen_heder.execute(),
            #[cfg(feature = "wast")]
            Some(Cmd::Wast(wast)) => wast.execute(),
            #[cfg(target_os = "linux")]
            Some(Cmd::Binfmt(binfmt)) => binfmt.execute(),
            Some(Cmd::Whoami(whoami)) => whoami.execute(),
            Some(Cmd::Add(install)) => install.execute(),

            // Deploy commands.
            Some(Cmd::Deploy(c)) => c.run(),
            Some(Cmd::App(apps)) => apps.run(),
            Some(Cmd::Ssh(ssh)) => ssh.run(),
            Some(Cmd::Namespace(namespace)) => namespace.run(),
            None => {
                Args::command().print_long_help()?;
                // Note: clap uses an exit code of 2 when CLI parsing fails
                std::process::exit(2);
            }
        }
    }
}

#[derive(Parser, Debug)]
/// The options for the wasmer Command Line Interface
enum Cmd {
    /// Login into a wasmer.io-like registry
    Login(Login),

    /// Login into a wasmer.io-like registry
    #[clap(name = "publish")]
    Publish(Publish),

    /// Wasmer cache
    Cache(Cache),

    /// Validate a WebAssembly binary
    Validate(Validate),

    /// Compile a WebAssembly binary
    #[cfg(feature = "compiler")]
    Compile(Compile),

    /// Compile a WebAssembly binary into a native executable
    ///
    /// To use, you need to set the `WASMER_DIR` environment variable
    /// to the location of your Wasmer installation. This will probably be `~/.wasmer`. It
    /// should include a `lib`, `include` and `bin` subdirectories. To create an executable
    /// you will need `libwasmer`, so by setting `WASMER_DIR` the CLI knows where to look for
    /// header files and libraries.
    ///
    /// Example usage:
    ///
    /// ```text
    /// $ # in two lines:
    /// $ export WASMER_DIR=/home/user/.wasmer/
    /// $ wasmer create-exe qjs.wasm -o qjs.exe # or in one line:
    /// $ WASMER_DIR=/home/user/.wasmer/ wasmer create-exe qjs.wasm -o qjs.exe
    /// $ file qjs.exe
    /// qjs.exe: ELF 64-bit LSB pie executable, x86-64 ...
    /// ```
    ///
    /// ## Cross-compilation
    ///
    /// Accepted target triple values must follow the
    /// ['target_lexicon'](https://crates.io/crates/target-lexicon) crate format.
    ///
    /// The recommended targets we try to support are:
    ///
    /// - "x86_64-linux-gnu"
    /// - "aarch64-linux-gnu"
    /// - "x86_64-apple-darwin"
    /// - "arm64-apple-darwin"
    #[cfg(any(feature = "static-artifact-create", feature = "wasmer-artifact-create"))]
    #[clap(name = "create-exe", verbatim_doc_comment)]
    CreateExe(CreateExe),

    /// Compile a WebAssembly binary into an object file
    ///
    /// To use, you need to set the `WASMER_DIR` environment variable to the location of your
    /// Wasmer installation. This will probably be `~/.wasmer`. It should include a `lib`,
    /// `include` and `bin` subdirectories. To create an object you will need `libwasmer`, so by
    /// setting `WASMER_DIR` the CLI knows where to look for header files and libraries.
    ///
    /// Example usage:
    ///
    /// ```text
    /// $ # in two lines:
    /// $ export WASMER_DIR=/home/user/.wasmer/
    /// $ wasmer create-obj qjs.wasm --object-format symbols -o qjs.obj # or in one line:
    /// $ WASMER_DIR=/home/user/.wasmer/ wasmer create-exe qjs.wasm --object-format symbols -o qjs.obj
    /// $ file qjs.obj
    /// qjs.obj: ELF 64-bit LSB relocatable, x86-64 ...
    /// ```
    ///
    /// ## Cross-compilation
    ///
    /// Accepted target triple values must follow the
    /// ['target_lexicon'](https://crates.io/crates/target-lexicon) crate format.
    ///
    /// The recommended targets we try to support are:
    ///
    /// - "x86_64-linux-gnu"
    /// - "aarch64-linux-gnu"
    /// - "x86_64-apple-darwin"
    /// - "arm64-apple-darwin"
    #[cfg(feature = "static-artifact-create")]
    #[structopt(name = "create-obj", verbatim_doc_comment)]
    CreateObj(CreateObj),

    /// Generate the C static_defs.h header file for the input .wasm module
    #[cfg(feature = "static-artifact-create")]
    GenCHeader(GenCHeader),

    /// Get various configuration information needed
    /// to compile programs which use Wasmer
    Config(Config),

    /// Update wasmer to the latest version
    #[clap(name = "self-update")]
    SelfUpdate(SelfUpdate),

    /// Inspect a WebAssembly file
    Inspect(Inspect),

    /// Initializes a new wasmer.toml file
    #[clap(name = "init")]
    Init(Init),

    /// Run spec testsuite
    #[cfg(feature = "wast")]
    Wast(Wast),

    /// Unregister and/or register wasmer as binfmt interpreter
    #[cfg(target_os = "linux")]
    Binfmt(Binfmt),

    /// Shows the current logged in user for the current active registry
    Whoami(Whoami),

    /// Add a Wasmer package's bindings to your application.
    Add(Add),

    /// Run a WebAssembly file or Wasmer container.
    #[clap(alias = "run-unstable")]
    Run(Run),

    #[clap(subcommand)]
    Package(crate::commands::Package),

    #[clap(subcommand)]
    Container(crate::commands::Container),

    // Edge commands
    /// Deploy apps to Wasmer Edge.
    Deploy(wasmer_edge_cli::cmd::deploy::CmdDeploy),

    /// Manage deployed Edge apps.
    #[clap(subcommand, alias = "apps")]
    App(wasmer_edge_cli::cmd::app::CmdApp),

    /// Run commands/packages on Wasmer Edge in an interactive shell session.
    Ssh(wasmer_edge_cli::cmd::ssh::CmdSsh),

    /// Manage Wasmer namespaces.
    #[clap(subcommand, alias = "namespaces")]
    Namespace(wasmer_edge_cli::cmd::namespace::CmdNamespace),
}

fn is_binfmt_interpreter() -> bool {
    cfg_if::cfg_if! {
        if #[cfg(target_os = "linux")] {
            // Note: we'll be invoked by the kernel as Binfmt::FILENAME
            let binary_path = match std::env::args_os().next() {
                Some(path) => std::path::PathBuf::from(path),
                None => return false,
            };
            binary_path.file_name().and_then(|f| f.to_str()) == Some(Binfmt::FILENAME)
        } else {
            false
        }
    }
}

fn print_version(verbose: bool) -> Result<(), anyhow::Error> {
    if !verbose {
        println!("wasmer {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }

    println!(
        "wasmer {} ({} {})",
        env!("CARGO_PKG_VERSION"),
        env!("WASMER_BUILD_GIT_HASH_SHORT"),
        env!("WASMER_BUILD_DATE")
    );
    println!("binary: {}", env!("CARGO_PKG_NAME"));
    println!("commit-hash: {}", env!("WASMER_BUILD_GIT_HASH"));
    println!("commit-date: {}", env!("WASMER_BUILD_DATE"));
    println!("host: {}", target_lexicon::HOST);

    let mut compilers = Vec::<&'static str>::new();
    if cfg!(feature = "singlepass") {
        compilers.push("singlepass");
    }
    if cfg!(feature = "cranelift") {
        compilers.push("cranelift");
    }
    if cfg!(feature = "llvm") {
        compilers.push("llvm");
    }
    println!("compiler: {}", compilers.join(","));

    Ok(())
}
