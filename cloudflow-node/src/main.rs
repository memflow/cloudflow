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
        .arg(Arg::new("verbose").short('v').multiple_occurrences(true))
        .arg(Arg::new("fuse").long("fuse").short('f').required(false))
        .arg(
            Arg::new("fuse-mount")
                .long("fuse-mount")
                .short('F')
                .takes_value(true)
                .required(false),
        )
        .arg(
            Arg::new("fuse-uid")
                .long("fuse-uid")
                .short('u')
                .takes_value(true)
                .required(false),
        )
        .arg(
            Arg::new("fuse-gid")
                .long("fuse-gid")
                .short('g')
                .takes_value(true)
                .required(false),
        )
        .arg(
            Arg::new("elevate")
                .long("elevate")
                .short('e')
                .required(false),
        )
        .get_matches()
}

fn extract_args(
    matches: &ArgMatches,
) -> Result<(Option<&str>, Option<u32>, Option<u32>, bool, log::Level)> {
    // set log level
    let level = match matches.occurrences_of("verbose") {
        0 => Level::Error,
        1 => Level::Warn,
        2 => Level::Info,
        3 => Level::Debug,
        4 => Level::Trace,
        _ => Level::Trace,
    };

    let fuse_mount = matches.value_of("fuse-mount").or_else(|| {
        if matches.occurrences_of("fuse") > 0 {
            Some("/cloudflow")
        } else {
            None
        }
    });

    let fuse_uid = matches.value_of("fuse-uid").map(str::parse).transpose()?;
    let fuse_gid = matches.value_of("fuse-gid").map(str::parse).transpose()?;

    let elevate = matches.occurrences_of("elevate") > 0;

    Ok((fuse_mount, fuse_uid, fuse_gid, elevate, level))
}
