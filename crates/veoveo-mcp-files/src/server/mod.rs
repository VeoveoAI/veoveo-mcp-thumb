//! The hosted process: register with the Hub, then serve MCP until SIGTERM or Ctrl-C. Process stop
//! preserves native resume credentials. The server has no tables and starts no Task.
mod config;
mod registration;

use std::net::SocketAddr;

use anyhow::{Context, Result};
use clap::Parser;
use veoveo_hub_sdk::{
    PublicDeployment, TelemetryGuard,
    apps::AppHtml,
    hosting::{Hosted, HostedServer, ServerCalls},
    init_server_telemetry,
    registration::{ClientMetadata, client::Registration, statement::ServerKeyPair},
};

use crate::{
    hub::Hub,
    mcp::{self, FilesMcp},
    thumbnail::Thumbnails,
};

use config::Args;

pub async fn run() -> Result<()> {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let _ = dotenvy::dotenv();
    let args = Args::parse();
    let deployment = args.public_deployment()?;
    let declaration = mcp::registration(&deployment, args.deployment_id)?;
    mcp::REGISTRATION
        .set(declaration.clone())
        .map_err(|_| anyhow::anyhow!("the registration was declared twice"))?;
    mcp::force_setup();
    if args.describe_registration {
        let metadata = registration::prepare(&args, declaration)?;
        serde_json::to_writer_pretty(std::io::stdout().lock(), &metadata)?;
        println!();
        return Ok(());
    }
    let _telemetry: TelemetryGuard =
        init_server_telemetry("veoveo-mcp-files", "info,veoveo_mcp_files=debug")?;
    let app = AppHtml::load(&args.app_html)?;
    let registration::Registered {
        client,
        registration,
        metadata,
        key,
        http,
    } = registration::register(&args, declaration).await?;
    // Process stop preserves saved native registration; explicit retirement and the
    // Hub's existing orphan grace remain separate.
    let issued = registration.clone();
    let mut served = None;
    let maintained = client
        .maintain(registration, async {
            served = Some(serve(&args, &deployment, &issued, &metadata, &key, http, app).await);
        })
        .await;
    if let Err(error) = maintained {
        tracing::warn!(%error, "registration lifecycle failed");
    }
    served.context("the registration ended before serving finished")?
}

/// Serve until SIGTERM or Ctrl-C.
async fn serve(
    args: &Args,
    deployment: &PublicDeployment,
    registration: &Registration,
    metadata: &ClientMetadata,
    key: &ServerKeyPair,
    http: reqwest::Client,
    app: AppHtml,
) -> Result<()> {
    let calls = ServerCalls::discover(registration, metadata, key, http.clone())
        .await
        .context("discovering the Hub's token endpoint")?;
    let hub = Hub::new(
        args.hub_issuer
            .as_ref()
            .context("serving requires a Hub issuer")?,
        http.clone(),
    )?;
    let thumbnails = Thumbnails::new(
        args.thumbnail_directory.clone(),
        args.work_directory.clone(),
        std::env::current_exe().context("finding this program")?,
        http.clone(),
    )
    .context("preparing the thumbnails' folders")?;
    let domain = FilesMcp::new(hub, app, thumbnails);
    let hosted = HostedServer::for_domain::<FilesMcp>()
        .deployment(deployment, args.allow_loopback_hosts)?
        .allowed_hosts(args.allowed_hosts.iter().cloned())
        .internal_trust(registration.access_tokens(), http)?
        .handler(move || Hosted::new(domain.clone()))
        .calls(calls)
        .build();
    hosted
        .serve(SocketAddr::from(([0, 0, 0, 0], args.port)))
        .await
}
