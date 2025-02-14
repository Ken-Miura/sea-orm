use clap::Parser;
use dotenv::dotenv;
use std::{error::Error, fmt::Display, process::exit};
use tracing_subscriber::{prelude::*, EnvFilter};

use sea_orm::{Database, DbConn};
use sea_orm_cli::{run_migrate_generate, run_migrate_init, MigrateSubcommands};

use super::MigratorTrait;

const MIGRATION_DIR: &str = "./";

pub async fn run_cli<M>(migrator: M)
where
    M: MigratorTrait,
{
    dotenv().ok();
    let url = std::env::var("DATABASE_URL").expect("Environment variable 'DATABASE_URL' not set");
    let db = &Database::connect(&url).await.unwrap();
    let cli = Cli::parse();

    run_migrate(migrator, db, cli.command, cli.verbose)
        .await
        .unwrap_or_else(handle_error);
}

pub async fn run_migrate<M>(
    _: M,
    db: &DbConn,
    command: Option<MigrateSubcommands>,
    verbose: bool,
) -> Result<(), Box<dyn Error>>
where
    M: MigratorTrait,
{
    let filter = match verbose {
        true => "debug",
        false => "sea_orm_migration=info",
    };

    let filter_layer = EnvFilter::try_new(filter).unwrap();

    if verbose {
        let fmt_layer = tracing_subscriber::fmt::layer();
        tracing_subscriber::registry()
            .with(filter_layer)
            .with(fmt_layer)
            .init()
    } else {
        let fmt_layer = tracing_subscriber::fmt::layer()
            .with_target(false)
            .with_level(false)
            .without_time();
        tracing_subscriber::registry()
            .with(filter_layer)
            .with(fmt_layer)
            .init()
    };

    match command {
        Some(MigrateSubcommands::Fresh) => M::fresh(db).await?,
        Some(MigrateSubcommands::Refresh) => M::refresh(db).await?,
        Some(MigrateSubcommands::Reset) => M::reset(db).await?,
        Some(MigrateSubcommands::Status) => M::status(db).await?,
        Some(MigrateSubcommands::Up { num }) => M::up(db, Some(num)).await?,
        Some(MigrateSubcommands::Down { num }) => M::down(db, Some(num)).await?,
        Some(MigrateSubcommands::Init) => run_migrate_init(MIGRATION_DIR)?,
        Some(MigrateSubcommands::Generate {
            migration_name,
            universal_time,
        }) => run_migrate_generate(MIGRATION_DIR, &migration_name, universal_time)?,
        _ => M::up(db, None).await?,
    };

    Ok(())
}

#[derive(Parser)]
#[clap(version)]
pub struct Cli {
    #[clap(action, short = 'v', long, global = true, help = "Show debug messages")]
    verbose: bool,

    #[clap(subcommand)]
    command: Option<MigrateSubcommands>,
}

fn handle_error<E>(error: E)
where
    E: Display,
{
    eprintln!("{}", error);
    exit(1);
}
