//! Thin bollard-backed helpers for the discrete Docker Engine operations the
//! orchestrator needs outside of `docker compose` — idempotently ensuring the
//! per-instance external networks/volumes. (Compose orchestration itself stays
//! on the CLI; bollard has no compose support.)

use anyhow::{Context, Result, bail, ensure};
use bollard::Docker;
use bollard::models::{Network, NetworkCreateRequest, VolumeCreateRequest};
use bollard::query_parameters::ListNetworksOptionsBuilder;
use macro_env_var::maybe_env_var;

/// Run a future to completion on a throwaway current-thread runtime (xtask's
/// flow is synchronous).
fn block_on<T>(fut: impl std::future::Future<Output = Result<T>>) -> Result<T> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .context("building tokio runtime")?
        .block_on(fut)
}

maybe_env_var! {
    struct UsePodman;
}
fn connect() -> Result<Docker> {
    match UsePodman::new()
        .map(|p| p.parse::<bool>().unwrap())
        .unwrap_or(false)
    {
        true => Docker::connect_with_podman_defaults().context("connecting to the Docker daemon"),
        false => Docker::connect_with_local_defaults().context("connecting to the Docker daemon"),
    }
}

/// Idempotently create a bridge network (no-op if exactly one already exists).
pub fn ensure_network(name: &str) -> Result<()> {
    block_on(async {
        let docker = connect()?;

        match exact_network_count(&docker, name).await? {
            0 => {}
            1 => return Ok(()),
            count => bail!(
                "found {count} Docker networks named {name}; refusing to choose an ambiguous network"
            ),
        }

        match docker
            .create_network(NetworkCreateRequest {
                name: name.to_string(),
                ..Default::default()
            })
            .await
        {
            Ok(_) => {}
            Err(e) if already_exists(&e) => {}
            Err(e) => return Err(e).with_context(|| format!("creating network {name}")),
        };

        let count = exact_network_count(&docker, name).await?;
        ensure!(
            count == 1,
            "found {count} Docker networks named {name} after creation; a concurrent creator made the network ambiguous"
        );
        Ok(())
    })
}

async fn exact_network_count(docker: &Docker, name: &str) -> Result<usize> {
    let filters = std::collections::HashMap::from([("name", vec![name])]);
    let options = ListNetworksOptionsBuilder::new().filters(&filters).build();
    let networks = docker
        .list_networks(Some(options))
        .await
        .with_context(|| format!("listing networks named {name}"))?;
    Ok(count_exact_network_names(&networks, name))
}

fn count_exact_network_names(networks: &[Network], name: &str) -> usize {
    networks
        .iter()
        .filter(|network| network.name.as_deref() == Some(name))
        .count()
}

/// Idempotently create a named volume (no-op if it already exists).
pub fn ensure_volume(name: &str) -> Result<()> {
    block_on(async {
        let docker = connect()?;
        match docker
            .create_volume(VolumeCreateRequest {
                name: Some(name.to_string()),
                ..Default::default()
            })
            .await
        {
            Ok(_) => Ok(()),
            Err(e) if already_exists(&e) => Ok(()),
            Err(e) => Err(e).with_context(|| format!("creating volume {name}")),
        }
    })
}

fn already_exists(e: &bollard::errors::Error) -> bool {
    let s = e.to_string();
    s.contains("already exists") || s.contains("409")
}

#[cfg(test)]
mod test {
    use super::*;

    fn network(name: Option<&str>) -> Network {
        Network {
            name: name.map(str::to_string),
            ..Default::default()
        }
    }

    #[test]
    fn exact_network_count_ignores_partial_name_matches() {
        let networks = [
            network(Some("databases-inimoa-dx1")),
            network(Some("databases-inimoa-dx1-old")),
            network(Some("prefix-databases-inimoa-dx1")),
            network(None),
        ];

        assert_eq!(
            count_exact_network_names(&networks, "databases-inimoa-dx1"),
            1
        );
    }

    #[test]
    fn exact_network_count_exposes_ambiguous_duplicates() {
        let networks = [
            network(Some("auth-inimoa-dx1")),
            network(Some("auth-inimoa-dx1")),
        ];

        assert_eq!(count_exact_network_names(&networks, "auth-inimoa-dx1"), 2);
    }
}

/// A container belonging to a compose project, as `status-local` reports it.
pub struct ProjectContainer {
    /// Container name (without the leading slash).
    pub name: String,
    /// Whether the container is currently running.
    pub running: bool,
    /// Human-readable status, e.g. `Up 3 hours (healthy)`.
    pub status: String,
    /// Published host ports, sorted and deduplicated.
    pub host_ports: Vec<u16>,
}

/// List every container (running or not) labeled with the compose `project`.
pub fn project_containers(project: &str) -> Result<Vec<ProjectContainer>> {
    use bollard::models::ContainerSummaryStateEnum;
    use bollard::query_parameters::ListContainersOptionsBuilder;

    block_on(async {
        let docker = connect()?;
        let filters = std::collections::HashMap::from([(
            "label",
            vec![format!("com.docker.compose.project={project}")],
        )]);
        let options = ListContainersOptionsBuilder::new()
            .all(true)
            .filters(&filters)
            .build();
        let mut containers: Vec<ProjectContainer> = docker
            .list_containers(Some(options))
            .await
            .context("listing containers")?
            .into_iter()
            .map(|c| {
                let name = c
                    .names
                    .unwrap_or_default()
                    .first()
                    .map(|n| n.trim_start_matches('/').to_string())
                    .or(c.id)
                    .unwrap_or_default();
                let mut host_ports: Vec<u16> = c
                    .ports
                    .unwrap_or_default()
                    .iter()
                    .filter_map(|p| p.public_port)
                    .collect();
                host_ports.sort_unstable();
                host_ports.dedup();
                ProjectContainer {
                    name,
                    running: c.state == Some(ContainerSummaryStateEnum::RUNNING),
                    status: c.status.unwrap_or_default(),
                    host_ports,
                }
            })
            .collect();
        containers.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(containers)
    })
}
