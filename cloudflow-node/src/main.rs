use anyhow::Result;
use clap::*;
use cloudflow::*;

use log::*;

fn main() -> Result<()> {
    let args = parse_args();
    let (mount_path, fuse_uid, fuse_gid, elevate, level) = extract_args(&args)?;

    if elevate {
        sudo::escalate_if_needed().expect("failed to elevate privileges");
        info!("Elevated privileges!");
    }

    simplelog::TermLogger::init(
        level.to_level_filter(),
        simplelog::Config::default(),
        simplelog::TerminalMode::Stdout,
        simplelog::ColorChoice::Auto,
    )
    .unwrap();

    let node = create_node();

    // Add custom plugin
    cloudflow_minidump::on_node(&node, Default::default());

    if let Some(mount_path) = mount_path {
        println!("Mounting FUSE filesystem on {}", mount_path);
        std::fs::create_dir_all(mount_path)?;
        filer_fuse::mount(
            node,
            mount_path,
            sudo::check() == sudo::RunningAs::Root,
            fuse_uid.unwrap_or(0),
            fuse_gid.unwrap_or(0),
        )?;
    }

    println!("Initialized!");

    loop {}
}

fn parse_args() -> ArgMatches {
    Command::new("cloudflow")
        .version(crate_version!())
        .author(crate_authors!())
        .arg(Arg::new("verbose").short('v').action(ArgAction::Count))
        .arg(
            Arg::new("fuse")
                .long("fuse")
                .short('f')
                .action(ArgAction::SetTrue)
                .required(false),
        )
        .arg(
            Arg::new("fuse-mount")
                .long("fuse-mount")
                .short('F')
                .action(ArgAction::Set)
                .required(false),
        )
        .arg(
            Arg::new("fuse-uid")
                .long("fuse-uid")
                .short('u')
                .action(ArgAction::Set)
                .required(false),
        )
        .arg(
            Arg::new("fuse-gid")
                .long("fuse-gid")
                .short('g')
                .action(ArgAction::Set)
                .required(false),
        )
        .arg(
            Arg::new("elevate")
                .long("elevate")
                .short('e')
                .action(ArgAction::SetTrue)
                .required(false),
        )
        .get_matches()
}

fn extract_args(
    matches: &ArgMatches,
) -> Result<(Option<&str>, Option<u32>, Option<u32>, bool, log::Level)> {
    // set log level
    let level = match matches.get_count("verbose") {
        0 => Level::Error,
        1 => Level::Warn,
        2 => Level::Info,
        3 => Level::Debug,
        4 => Level::Trace,
        _ => Level::Trace,
    };

    let fuse_mount = if matches.get_flag("fuse") {
        matches
            .get_one::<String>("fuse-mount")
            .map(String::as_str)
            .or(Some("/cloudflow"))
    } else {
        None
    };

    let fuse_uid = matches
        .get_one::<String>("fuse-uid")
        .map(|s| s.parse())
        .transpose()?;
    let fuse_gid = matches
        .get_one::<String>("fuse-gid")
        .map(|s| s.parse())
        .transpose()?;

    let elevate = matches.get_flag("elevate");

    Ok((fuse_mount, fuse_uid, fuse_gid, elevate, level))
}
