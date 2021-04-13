use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use tokio;
use flate2::read::GzDecoder;
use tar::Archive;

use tonic::{Request, Response, Status};
use crate::protos::*;
use crate::common::*;

/// Frequency at which the host must send messages to the controller, in seconds.
pub const HEARTBEAT_INTERVAL: u64 = 10;

pub struct Host {
    /// Host ID (unique among hosts)
    id: u32,
    /// Secret host token, assigned after registering with controller
    host_token: Option<HostToken>,
    /// Karl path, likely ~/.karl
    karl_path: PathBuf,
    /// Computation request base, likely ~/.karl/<id>
    /// Computation root likely at ~/.karl/<id>/root/
    base_path: PathBuf,
    /// Controller address.
    controller: String,
}

#[tonic::async_trait]
impl karl_host_server::KarlHost for Host {
    async fn start_compute(
        &self, req: Request<ComputeRequest>,
    ) -> Result<Response<()>, Status> {
        let req = req.into_inner();

        // Notify the controller of the start and end of the request.
        crate::net::notify_start(&self.controller, self.id).await?;
        let _res = self.handle_compute(req);
        crate::net::notify_end(&self.controller, self.id).await?;

        Ok(Response::new(()))
    }

    async fn network(
        &self, req: Request<NetworkAccess>,
    ) -> Result<Response<NetworkAccessResult>, Status> {
        unimplemented!()
    }

    async fn get(
        &self, req: Request<GetData>,
    ) -> Result<Response<GetDataResult>, Status> {
        unimplemented!()
    }

    async fn put(
        &self, req: Request<PutData>,
    ) -> Result<Response<()>, Status> {
        unimplemented!()
    }

    async fn delete(
        &self, req: Request<DeleteData>,
    ) -> Result<Response<()>, Status> {
        unimplemented!()
    }

    async fn state(
        &self, req: Request<StateChange>,
    ) -> Result<Response<()>, Status> {
        unimplemented!()
    }
}

/// Unpackage the bytes of the tarred and gzipped request to the base path.
/// This is the input root.
///
/// Creates the root path directory if it does not already exist.
fn unpack_request(req: &ComputeRequest, root: &Path) -> Result<(), Error> {
    let now = Instant::now();
    let tar = GzDecoder::new(&req.package[..]);
    let mut archive = Archive::new(tar);
    fs::create_dir_all(root).unwrap();
    archive.unpack(root).map_err(|e| format!("malformed tar.gz: {:?}", e))?;
    info!("=> unpacked request to {:?}: {} s", root, now.elapsed().as_secs_f32());
    Ok(())
}

impl Drop for Host {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.base_path);
    }
}

impl Host {
    /// Generate a new host with a random ID.
    pub fn new(
        karl_path: PathBuf,
        controller: &str,
    ) -> Self {
        use rand::Rng;
        let id: u32 = rand::thread_rng().gen();
        let base_path = karl_path.join(id.to_string());
        Self {
            id,
            host_token: None,
            karl_path,
            base_path,
            controller: controller.to_string(),
        }
    }

    /// Spawns a background process that sends heartbeats to the controller
    /// at the HEARTBEAT_INTERVAL.
    ///
    /// The constructor creates a directory at the <KARL_PATH> if it does
    /// not already exist. The working directory for any computation is at
    /// <KARL_PATH>/<LISTENER_ID>. When not doing computation, the working
    /// directory must be at <KARL_PATH>.
    ///
    /// Parameters:
    /// - port - The port to listen on.
    /// - password - The password to register with the controller.
    pub async fn start(&mut self, port: u16, password: &str) -> Result<(), Status> {
        // Create the <KARL_PATH> if it does not already exist.
        fs::create_dir_all(&self.karl_path).unwrap();
        // Set the current working directory to the <KARL_PATH>.
        std::env::set_current_dir(&self.karl_path).unwrap();
        debug!("create karl_path {:?}", &self.karl_path);

        let controller_addr = self.controller.clone();
        let host_token = crate::net::register_host(
            &self.controller, self.id, port, password).await?.into_inner().host_token;
        tokio::spawn(async move {
            // Every HEARTBEAT_INTERVAL seconds, this process wakes up
            // sends a heartbeat message to the controller.
            loop {
                tokio::time::sleep(Duration::from_secs(HEARTBEAT_INTERVAL)).await;
                debug!("heartbeat {}", &host_token);
                let res = crate::net::heartbeat(
                    &controller_addr,
                    host_token.clone(),
                ).await;
                if let Err(e) = res {
                    warn!("error sending heartbeat: {}", e);
                };
            }
        });
        Ok(())
    }

    /// Handle a compute request.
    ///
    /// The client must be verified by the caller.
    fn handle_compute(
        &self,
        req: ComputeRequest,
    ) -> Result<(), Error> {
        info!("handling compute (len {}):\n\
            command => {:?} {:?}\n\
            envs => {:?}\n\
            file_perm => {:?}\n\
            network_perm => {:?}",
            req.package.len(), req.binary_path, req.args,
            req.envs, req.file_perm, req.network_perm);
        let now = Instant::now();

        let root_path = self.base_path.join("root");
        unpack_request(&req, &root_path)?;
        let binary_path = Path::new(&req.binary_path).to_path_buf();
        info!("=> preprocessing: {} s", now.elapsed().as_secs_f32());

        let _res = crate::runtime::run(
            binary_path,
            req.args,
            req.envs,
            &self.karl_path,
            &self.base_path,
        )?;

        // Reset the root for the next computation.
        std::env::set_current_dir(&self.karl_path).unwrap();
        if let Err(e) = std::fs::remove_dir_all(&self.base_path) {
            error!("error resetting root: {:?}", e);
        }
        let now = Instant::now();
        info!(
            "reset directory at {:?} => {} s",
            self.base_path,
            now.elapsed().as_secs_f32(),
        );
        Ok(())
    }
}
