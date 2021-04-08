use std::fs;
use std::thread;
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use std::sync::{Arc, Mutex};

use tokio::runtime::Runtime;
use flate2::read::GzDecoder;
use tar::Archive;

use protobuf::{self, Message, ProtobufEnum};
use crate::{packet, protos, protos::MessageType};
use crate::common::{
    Error, Token, RequestToken,
};

/// Frequency at which the host must send messages to the controller, in seconds.
pub const HEARTBEAT_INTERVAL: u64 = 10;

pub struct Host {
    /// Node/service ID
    id: u32,
    /// Karl path, likely ~/.karl
    karl_path: PathBuf,
    /// Computation request base, likely ~/.karl/<id>
    /// Computation root likely at ~/.karl/<id>/root/
    base_path: PathBuf,
    port: u16,
    rt: Runtime,
    /// Controller address.
    controller: String,
    /// Request token, or none if the host is processing a ComputeRequest.
    /// The host will only accept a ComputeRequest if it includes the token.
    ///
    /// The time is when the request token was last renewed.
    token: Arc<Mutex<(Option<RequestToken>, Instant)>>,
}

/// Unpackage the bytes of the tarred and gzipped request to the base path.
/// This is the input root.
///
/// Creates the root path directory if it does not already exist.
fn unpack_request(req: &protos::ComputeRequest, root: &Path) -> Result<(), Error> {
    let now = Instant::now();
    let tar = GzDecoder::new(&req.get_package()[..]);
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
        port: u16,
        controller: &str,
    ) -> Self {
        use rand::Rng;
        let id: u32 = rand::thread_rng().gen();
        let base_path = karl_path.join(id.to_string());
        Self {
            id,
            karl_path,
            base_path,
            port,
            rt: Runtime::new().unwrap(),
            controller: controller.to_string(),
            token: Arc::new(Mutex::new((Some(Token::gen()), Instant::now()))),
        }
    }

    /// Spawns a background process that sends heartbeats to the controller
    /// at the HEARTBEAT_INTERVAL and processes incoming connections for
    /// the following messages:
    /// - PingRequest
    /// - ComputeRequest
    ///
    /// The constructor creates a directory at the <KARL_PATH> if it does
    /// not already exist. The working directory for any computation is at
    /// <KARL_PATH>/<LISTENER_ID>. When not doing computation, the working
    /// directory must be at <KARL_PATH>.
    ///
    /// Parameters:
    /// - password - The password to register with the controller.
    pub fn start(&mut self, password: &str) -> Result<(), Error> {
        // Create the <KARL_PATH> if it does not already exist.
        fs::create_dir_all(&self.karl_path).unwrap();
        // Set the current working directory to the <KARL_PATH>.
        std::env::set_current_dir(&self.karl_path).unwrap();
        debug!("create karl_path {:?}", &self.karl_path);

        let listener = TcpListener::bind(format!("0.0.0.0:{}", self.port))?;
        self.port = listener.local_addr()?.port();
        info!("ID {} listening on port {}", self.id, self.port);
        crate::net::register_host(&self.controller, self.id, self.port, password);

        let token_lock = self.token.clone();
        let controller_addr = self.controller.clone();
        let host_id = self.id.clone();
        self.rt.spawn(async move {
            // Every HEARTBEAT_INTERVAL seconds, this process wakes up
            // sends a heartbeat message to the controller.
            //
            // The host renews the request token with every heartbeat except
            // in the following two cirucmstance:
            // 1) The last message sent was a NotifyStart, and thus the host
            //    is actively processing a request.
            // 2) It has been less than HEARTBEAT_INTERVAL-1 seconds since
            //    renewing the token, which should only occur if the last
            //    message sent was a NotifyEnd.
            //
            // Note request tokens are also sent with NotifyEnd messages.
            //
            // Note that the host may generate a new request token in the time
            // that the controller allocates a host token and the client sends
            // a ComputeRequest. In that case, the client must retry the
            // HostRequest with the controller (equivalent to client timeout).
            // This mechanism protects the host from clients that reserve
            // hosts with the controller and fails to make a ComputeRequest.
            loop {
                let mut token = token_lock.lock().unwrap();
                let active_request = token.0.is_none();
                let recent_renewal = token.1.elapsed().as_secs_f32() < (HEARTBEAT_INTERVAL-1) as f32;
                let token_to_send = if !active_request && !recent_renewal {
                    // Renew the token if not processing a request or if it
                    // has been less than HEARTBEAT_INTERVAL-1 seconds since
                    // renewing the token.
                    let token_to_send = Token::gen();
                    debug!("heartbeat {:?}", &token_to_send);
                    *token = (Some(token_to_send.clone()), Instant::now());
                    Some(token_to_send)
                } else {
                    None
                };
                crate::net::heartbeat(&controller_addr, host_id, token_to_send);
                drop(token);
                thread::sleep(Duration::from_secs(HEARTBEAT_INTERVAL));
            }
        });

        for stream in listener.incoming() {
            let stream = stream?;
            debug!("incoming stream {:?}", stream.peer_addr());
            let now = Instant::now();
            if let Err(e) = self.handle_client(stream) {
                error!("{:?}", e);
            }
            warn!("total: {} s", now.elapsed().as_secs_f32());
        }
        Ok(())
    }

    /// Handle a ping request.
    fn handle_ping(&mut self, _req: protos::PingRequest) -> protos::PingResult {
        protos::PingResult::default()
    }

    /// Validate and use the request token.
    ///
    /// If the token is valid, "uses" the token by invalidating it for
    /// future requests, indicating that the host has an active request.
    /// If not, returns Error::InvalidRequestToken.
    fn use_request_token(&mut self, token: &RequestToken) -> Result<(), Error> {
        let mut real_token = self.token.lock().unwrap();
        if let Some(real_token) = &real_token.0 {
            if token != real_token {
                warn!("active token {:?} != client's token {:?}", token, real_token);
                return Err(Error::InvalidRequestToken("tokens do not match".to_string()));
            }
        } else {
            warn!("no active request token, either the host has not sent its \
                initial token to the controller and a malicious client has \
                found the host anyway, or there is a bug regenerating the \
                token after processing a request.");
            return Err(Error::InvalidRequestToken("no active token".to_string()));
        }

        // Invalidate the token for future requests and notify start.
        *real_token = (None, Instant::now());
        Ok(())
    }

    /// Handle a compute request.
    ///
    /// The client must be verified by the caller.
    fn handle_compute(
        &mut self,
        req: protos::ComputeRequest,
    ) -> Result<protos::ComputeResult, Error> {
        info!("handling compute from {:?}: (len {}) storage={}",
            req.client_id, req.package.len(), req.storage);
        let now = Instant::now();

        let root_path = self.base_path.join("root");
        unpack_request(&req, &root_path)?;
        let binary_path = Path::new(req.get_binary_path()).to_path_buf();
        info!("=> preprocessing: {} s", now.elapsed().as_secs_f32());

        let res = crate::runtime::run(
            binary_path,
            req.get_args().to_vec(),
            req.get_envs().to_vec(),
            &self.karl_path,
            &self.base_path,
            req.get_client_id(),
            req.get_storage(),
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
        Ok(res)
    }

    /// Handle an incoming TCP stream.
    fn handle_client(&mut self, mut stream: TcpStream) -> Result<(), Error> {
        // Read the computation request from the TCP stream.
        let now = Instant::now();
        debug!("reading packet");
        let (header, buf) = packet::read(&mut stream, 1)?.remove(0);
        debug!("=> {} s ({} bytes)", now.elapsed().as_secs_f32(), buf.len());

        // Deploy the request to correct handler.
        let ty = MessageType::from_i32(
            header.ty).ok_or(Error::InvalidPacketType(header.ty))?;
        let (res_bytes, ty) = match ty {
            MessageType::PING_REQUEST => {
                debug!("deserialize packet");
                let now = Instant::now();
                let req = protobuf::parse_from_bytes::<protos::PingRequest>(&buf[..])
                    .map_err(|e| Error::SerializationError(format!("{:?}", e)))?;
                debug!("=> {} s", now.elapsed().as_secs_f32());
                let res = self.handle_ping(req);
                debug!("=> {:?}", res);
                debug!("serialize packet");
                let now = Instant::now();
                let res_bytes = res.write_to_bytes()
                    .map_err(|e| Error::SerializationError(format!("{:?}", e)))?;
                debug!("=> {} s", now.elapsed().as_secs_f32());
                (res_bytes, MessageType::PING_RESULT)
            },
            MessageType::COMPUTE_REQUEST => {
                debug!("deserialize packet");
                let now = Instant::now();
                let req = protobuf::parse_from_bytes::<protos::ComputeRequest>(&buf[..])
                    .map_err(|e| Error::SerializationError(format!("{:?}", e)))?;
                debug!("=> {} s", now.elapsed().as_secs_f32());

                // Verify that the compute request includes a valid request token.
                // Notify the controller of the start and end of the request, if
                // the request token is valid and the host processes compute.
                self.use_request_token(&Token(req.request_token.clone()))?;
                crate::net::notify_start(&self.controller, self.id, req.client_id.clone());
                let res = self.handle_compute(req);

                // Notify end and create a new token.
                let mut token = self.token.lock().unwrap();
                *token = (Some(Token::gen()), Instant::now());
                crate::net::notify_end(&self.controller, self.id, token.0.clone().unwrap());
                drop(token);

                debug!("=> {:?}", res);
                debug!("serialize packet");
                let now = Instant::now();
                let res_bytes = res?.write_to_bytes()
                    .map_err(|e| Error::SerializationError(format!("{:?}", e)))?;
                debug!("=> {} s", now.elapsed().as_secs_f32());
                (res_bytes, MessageType::COMPUTE_RESULT)
            },
            ty => return Err(Error::InvalidPacketType(ty.value())),
        };

        // Return the result to sender.
        debug!("writing packet");
        let now = Instant::now();
        packet::write(&mut stream, ty, &res_bytes)?;
        debug!("=> {} s", now.elapsed().as_secs_f32());
        Ok(())
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use tempdir::TempDir;

    fn init_host() -> (TempDir, Host) {
        let karl_path = TempDir::new("karl").unwrap();
        let host = Host::new(
            karl_path.path().to_path_buf(),
            8080,
            "1.2.3.4:8000",
        );
        (karl_path, host)
    }

    /// Helper function to get token from host.
    fn get_token(host: &Host) -> Option<RequestToken> {
        host.token.lock().unwrap().0.clone()
    }

    /// Helper function to set token in host.
    fn set_token(host: &mut Host, token: &Token) {
        host.token.lock().unwrap().0 = Some(token.clone());
    }

    #[test]
    fn no_active_token_fails() {
        let (_karl_path, mut h) = init_host();
        let token = Token("abc".to_string());
        assert!(get_token(&h).is_some());
        h.token.lock().unwrap().0 = None;
        assert!(get_token(&h).is_none());
        assert!(h.use_request_token(&token).is_err());
    }

    #[test]
    fn non_matching_token_fails() {
        let (_karl_path, mut h) = init_host();
        let token1 = Token("abc".to_string());
        let token2 = Token("def".to_string());
        set_token(&mut h, &token1);
        assert!(get_token(&h).is_some());
        assert!(h.use_request_token(&token2).is_err(), "non matching token fails");
        assert!(h.use_request_token(&token1).is_ok(), "matching token succeeds");
    }

    #[test]
    fn matching_token_invalidates_old_token() {
        let (_karl_path, mut h) = init_host();
        let token = Token("abc".to_string());
        set_token(&mut h, &token);
        assert!(get_token(&h).is_some());
        assert!(h.use_request_token(&token).is_ok(), "token succeeds the first time");
        assert!(get_token(&h).is_none());
        assert!(h.use_request_token(&token).is_err(), "token fails the second time");
    }
}
